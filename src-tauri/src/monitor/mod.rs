//! Activity monitor: idle/input/active-window sampling + 5-minute heartbeats.
//!
//! Runs only while the timer is active (started from `sign_in`, stopped from
//! `sign_out`). Two workers share one `Bucket`:
//!   - a dedicated OS thread sampling every 1s (Win32 calls are blocking + some
//!     handles are `!Send`, so they stay off the async reactor);
//!   - a tokio task that every 5 min snapshots+resets the bucket and POSTs it,
//!     gated by the tenant `activity_monitoring` feature flag (fail-open).

mod active_window;
mod idle;
mod input;
mod screenshot;
mod session;

pub use idle::idle_seconds;
pub use session::{session_info, SessionInfo};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::api::ApiClient;
use crate::queue;
use input::InputTracker;

/// Sample the active window every N seconds (matches the Go 5s window tick).
const WINDOW_SAMPLE_EVERY: u32 = 5;
/// Cap distinct apps per bucket to avoid unbounded growth / hot partitions.
const MAX_APPS_PER_BUCKET: usize = 30;
/// Heartbeat cadence.
const HEARTBEAT_SECS: u64 = 300;
/// Screenshot cadence base + jitter (anti-evasion): random in [9min, 10min).
const SCREENSHOT_BASE_SECS: u64 = 540;
const SCREENSHOT_JITTER_SECS: u64 = 60;
/// Idle threshold: > this many idle seconds counts the second as idle.
const IDLE_THRESHOLD_SECS: u64 = 2;
/// Ignore per-second input deltas above this — guards against counter spikes.
const SPIKE_CAP: u32 = 1000;

#[derive(Default)]
struct Bucket {
    keyboard_count: u64,
    mouse_count: u64,
    active_seconds: u64,
    idle_seconds: u64,
    app_usage: HashMap<String, u64>,
}

/// Handle to a running monitor. Dropping/stopping it ends all workers.
pub struct MonitorHandle {
    running: Arc<AtomicBool>,
    tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
}

impl MonitorHandle {
    /// Stop all workers. The sampling thread exits within ~1s of the flag flip;
    /// the async tasks are aborted immediately.
    pub fn stop(self) {
        self.running.store(false, Ordering::SeqCst);
        for task in self.tasks {
            task.abort();
        }
    }
}

/// Start the monitor. Returns a handle the caller stores in `AppState`.
pub fn start(app: &AppHandle) -> MonitorHandle {
    let running = Arc::new(AtomicBool::new(true));
    let bucket = Arc::new(Mutex::new(Bucket::default()));
    // Shared so heartbeat + screenshot workers coordinate one network:error /
    // network:restored pair rather than emitting duplicates.
    let had_failure = Arc::new(AtomicBool::new(false));

    // 1s sampling worker on a dedicated OS thread (blocking Win32 + !Send).
    {
        let running = running.clone();
        let bucket = bucket.clone();
        std::thread::Builder::new()
            .name("taskflow-monitor".into())
            .spawn(move || sample_loop(running, bucket))
            .expect("spawn monitor thread");
    }

    let mut tasks = Vec::new();

    // 5-min heartbeat worker.
    tasks.push({
        let (running, bucket, had_failure, app) =
            (running.clone(), bucket.clone(), had_failure.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            heartbeat_loop(app, running, bucket, had_failure).await;
        })
    });

    // Jittered screenshot worker. Shares `bucket` so a capture can be
    // early-flushed with its screenshot_url attached (see screenshot_loop).
    tasks.push({
        let (running, bucket, had_failure, app) =
            (running.clone(), bucket.clone(), had_failure.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            screenshot_loop(app, running, bucket, had_failure).await;
        })
    });

    MonitorHandle { running, tasks }
}

/// Emit `network:error` on the first failure of a run of failures.
fn mark_failure(app: &AppHandle, had_failure: &AtomicBool) {
    if !had_failure.swap(true, Ordering::SeqCst) {
        crate::events::emit_network_error(app, "Connection issue — data will retry");
    }
}

/// Emit `network:restored` when the first success after failures lands.
fn mark_success(app: &AppHandle, had_failure: &AtomicBool) {
    if had_failure.swap(false, Ordering::SeqCst) {
        crate::events::emit_network_restored(app);
    }
}

fn sample_loop(running: Arc<AtomicBool>, bucket: Arc<Mutex<Bucket>>) {
    let mut tracker = InputTracker::new();
    let (mut last_kb, mut last_ms) = (0u32, 0u32);
    let mut first = true;
    let mut tick: u32 = 0;

    while running.load(Ordering::Relaxed) {
        let idle = idle::idle_seconds();
        let (kb, ms) = tracker.poll();

        {
            let mut b = bucket.lock().unwrap();
            if !first {
                let kbd = wrap_delta(last_kb, kb);
                let msd = wrap_delta(last_ms, ms);
                if kbd < SPIKE_CAP {
                    b.keyboard_count += kbd as u64;
                }
                if msd < SPIKE_CAP {
                    b.mouse_count += msd as u64;
                }
            }
            if idle > IDLE_THRESHOLD_SECS {
                b.idle_seconds += 1;
            } else {
                b.active_seconds += 1;
            }
        }
        last_kb = kb;
        last_ms = ms;
        first = false;

        tick = tick.wrapping_add(1);
        if tick % WINDOW_SAMPLE_EVERY == 0 {
            if let Some(app_name) = active_window::active_app() {
                let mut b = bucket.lock().unwrap();
                let known = b.app_usage.contains_key(&app_name);
                if b.app_usage.len() < MAX_APPS_PER_BUCKET || known {
                    *b.app_usage.entry(app_name).or_insert(0) += WINDOW_SAMPLE_EVERY as u64;
                }
            }
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}

async fn heartbeat_loop(
    app: AppHandle,
    running: Arc<AtomicBool>,
    bucket: Arc<Mutex<Bucket>>,
    had_failure: Arc<AtomicBool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Consume the immediate first tick so the first send is a full period in.
    interval.tick().await;

    while running.load(Ordering::Relaxed) {
        interval.tick().await;
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let api = app.state::<ApiClient>();
        if !api.activity_monitoring_enabled() {
            continue; // tenant has monitoring off — keep sampling, skip sending
        }

        // Drain any backlog from previous offline periods first.
        for queued in queue::take_heartbeats(&app) {
            match api.send_heartbeat(queued.clone()).await {
                Ok(()) => mark_success(&app, &had_failure),
                Err(_) => {
                    queue::enqueue_heartbeat(&app, &queued);
                    mark_failure(&app, &had_failure);
                }
            }
        }

        // Send the current bucket.
        let payload = {
            let mut b = bucket.lock().unwrap();
            match snapshot(&b, None) {
                Some(p) => {
                    *b = Bucket::default();
                    Some(p)
                }
                None => None,
            }
        };
        if let Some(payload) = payload {
            match api.send_heartbeat(payload.clone()).await {
                Ok(()) => mark_success(&app, &had_failure),
                Err(e) => {
                    tracing::warn!(error = %e, "heartbeat send failed — queued");
                    queue::enqueue_heartbeat(&app, &payload);
                    mark_failure(&app, &had_failure);
                }
            }
        }
    }
}

async fn screenshot_loop(
    app: AppHandle,
    running: Arc<AtomicBool>,
    bucket: Arc<Mutex<Bucket>>,
    had_failure: Arc<AtomicBool>,
) {
    while running.load(Ordering::Relaxed) {
        // Recompute jitter each cycle (anti-evasion). rng is dropped before the
        // await so nothing non-Send crosses it.
        let wait = {
            let mut rng = rand::thread_rng();
            SCREENSHOT_BASE_SECS + rng.gen_range(0..SCREENSHOT_JITTER_SECS)
        };
        tokio::time::sleep(Duration::from_secs(wait)).await;
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let api = app.state::<ApiClient>();
        if !api.screenshots_enabled() {
            continue; // fail-closed: tenant hasn't opted in
        }

        // Retry any backlog first.
        for shot in queue::list_screenshots(&app) {
            match api.upload_screenshot(&shot.jpeg, &shot.filename).await {
                Ok(_) => {
                    queue::remove_screenshot(&shot);
                    mark_success(&app, &had_failure);
                }
                Err(_) => mark_failure(&app, &had_failure),
            }
        }

        // Capture on a blocking thread (GDI is blocking + its handles are
        // !Send). The lock is re-checked inside the closure for TOCTOU safety.
        let jpeg = tokio::task::spawn_blocking(|| {
            if screenshot::is_screen_locked() {
                None
            } else {
                screenshot::capture_jpeg()
            }
        })
        .await
        .ok()
        .flatten();

        if let Some(jpeg) = jpeg {
            let filename = format!(
                "screenshot_{}.jpg",
                chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
            );
            match api.upload_screenshot(&jpeg, &filename).await {
                Ok(url) => {
                    mark_success(&app, &had_failure);
                    // Early-flush: snapshot the current bucket WITH the
                    // screenshot_url attached and ship it immediately, then
                    // reset — so the shot links to the exact activity window
                    // it was captured in and the regular heartbeat doesn't
                    // double-count. Mirrors the Go app's sendCurrentBucket().
                    let payload = {
                        let mut b = bucket.lock().unwrap();
                        match snapshot(&b, Some(&url)) {
                            Some(p) => {
                                *b = Bucket::default();
                                Some(p)
                            }
                            None => None,
                        }
                    };
                    if let Some(payload) = payload {
                        match api.send_heartbeat(payload.clone()).await {
                            Ok(()) => mark_success(&app, &had_failure),
                            Err(e) => {
                                tracing::warn!(error = %e, "screenshot heartbeat send failed — queued");
                                queue::enqueue_heartbeat(&app, &payload);
                                mark_failure(&app, &had_failure);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "screenshot upload failed — queued");
                    queue::enqueue_screenshot(&app, &jpeg, &filename);
                    mark_failure(&app, &had_failure);
                }
            }
        }
    }
}

/// Build the heartbeat JSON, or None when there's nothing to send. Matches the
/// Go payload exactly: snake_case fields + RFC3339 UTC timestamp. When
/// `screenshot_url` is set it is attached to the payload, and the bucket is
/// emitted even with near-zero counts — a captured screenshot must always ship,
/// otherwise it orphans in S3 with no activity record referencing it.
fn snapshot(b: &Bucket, screenshot_url: Option<&str>) -> Option<Value> {
    let has_data = b.keyboard_count > 0
        || b.mouse_count > 0
        || b.active_seconds > 0
        || b.idle_seconds > 0
        || !b.app_usage.is_empty();
    if !has_data && screenshot_url.is_none() {
        return None;
    }

    let top_app = b
        .app_usage
        .iter()
        .max_by_key(|(_, secs)| **secs)
        .map(|(app, _)| app.clone())
        .unwrap_or_default();

    let mut payload = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "keyboard_count": b.keyboard_count,
        "mouse_count": b.mouse_count,
        "active_seconds": b.active_seconds,
        "idle_seconds": b.idle_seconds,
        "top_app": top_app,
        "app_breakdown": b.app_usage,
    });
    if let Some(url) = screenshot_url {
        payload["screenshot_url"] = json!(url);
    }
    Some(payload)
}

/// Cumulative-counter delta with uint32 wrap handling (mirrors the Go formula).
fn wrap_delta(last: u32, cur: u32) -> u32 {
    if cur >= last {
        cur - last
    } else {
        (u32::MAX - last).wrapping_add(cur)
    }
}
