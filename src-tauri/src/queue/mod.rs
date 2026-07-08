//! On-disk offline persistence under the app-data dir.
//!
//! Best-effort throughout: every failure logs and degrades rather than
//! propagating — a machine that can't write its cache must still run. Holds the
//! heartbeat backlog (JSONL), the screenshot backlog (jpeg + json sidecar), and
//! the tasks cache. `clear_all` backs the `clear_local_cache` command and never
//! touches keyring tokens.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::api::Task;

fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

fn queue_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = data_dir(app)?.join("queue");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

// --- heartbeat backlog (JSON lines) ---------------------------------------

pub fn enqueue_heartbeat(app: &AppHandle, payload: &Value) {
    let Some(dir) = queue_dir(app) else { return };
    let Ok(line) = serde_json::to_string(payload) else {
        return;
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("heartbeats.jsonl"))
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Take all queued heartbeats (removing the file). The caller re-enqueues any
/// that fail to send.
pub fn take_heartbeats(app: &AppHandle) -> Vec<Value> {
    let Some(dir) = queue_dir(app) else {
        return Vec::new();
    };
    let path = dir.join("heartbeats.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let _ = std::fs::remove_file(&path);
    content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

// --- screenshot backlog (jpeg + json sidecar) -----------------------------

#[derive(Serialize, Deserialize)]
struct ShotMeta {
    filename: String,
}

pub struct QueuedShot {
    pub jpeg: Vec<u8>,
    pub filename: String,
    jpg_path: PathBuf,
    json_path: PathBuf,
}

pub fn enqueue_screenshot(app: &AppHandle, jpeg: &[u8], filename: &str) {
    let Some(dir) = queue_dir(app) else { return };
    let sdir = dir.join("screenshots");
    let _ = std::fs::create_dir_all(&sdir);
    let id: String = filename
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' { c } else { '_' })
        .collect();
    let _ = std::fs::write(sdir.join(format!("{id}.jpg")), jpeg);
    if let Ok(meta) = serde_json::to_vec(&ShotMeta {
        filename: filename.to_string(),
    }) {
        let _ = std::fs::write(sdir.join(format!("{id}.json")), meta);
    }
}

pub fn list_screenshots(app: &AppHandle) -> Vec<QueuedShot> {
    let Some(dir) = queue_dir(app) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir.join("screenshots")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let json_path = entry.path();
        if json_path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&json_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<ShotMeta>(&bytes) else {
            continue;
        };
        let jpg_path = json_path.with_extension("jpg");
        if let Ok(jpeg) = std::fs::read(&jpg_path) {
            out.push(QueuedShot {
                jpeg,
                filename: meta.filename,
                jpg_path,
                json_path,
            });
        }
    }
    out
}

pub fn remove_screenshot(shot: &QueuedShot) {
    let _ = std::fs::remove_file(&shot.jpg_path);
    let _ = std::fs::remove_file(&shot.json_path);
}

// --- tasks cache ----------------------------------------------------------

pub fn save_tasks(app: &AppHandle, tasks: &[Task]) {
    let Some(dir) = data_dir(app) else { return };
    if let Ok(bytes) = serde_json::to_vec(tasks) {
        let _ = std::fs::write(dir.join("tasks_cache.json"), bytes);
    }
}

pub fn load_tasks(app: &AppHandle) -> Option<Vec<Task>> {
    let dir = data_dir(app)?;
    let bytes = std::fs::read(dir.join("tasks_cache.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// --- clear all (keeps keyring tokens) -------------------------------------

pub fn clear_all(app: &AppHandle) {
    let Some(dir) = data_dir(app) else { return };
    let _ = std::fs::remove_dir_all(dir.join("queue"));
    let _ = std::fs::remove_file(dir.join("tasks_cache.json"));
}
