//! Authentication subsystem: Cognito flows + token lifecycle + persistence.
//!
//! `AuthManager` is registered as Tauri managed state and is the single owner
//! of session tokens and the in-flight `NEW_PASSWORD_REQUIRED` challenge. All
//! mutable state sits behind one `tokio::sync::Mutex`, which also gives us
//! singleflight refresh for free: concurrent callers of `valid_id_token()`
//! serialize on the lock, and the second one sees the token the first refreshed.

mod cognito;
mod keyring;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use cognito::{AuthOutcome, Cognito, RawTokens};

/// Keyring keys for the three persisted token fields + expiry.
const K_ID: &str = "id_token";
const K_ACCESS: &str = "access_token";
const K_REFRESH: &str = "refresh_token";
const K_EXPIRES: &str = "expires_at";

/// Refresh this many seconds before the real expiry to avoid edge races.
const EXPIRY_SKEW_SECS: u64 = 60;

/// Returned to the frontend from `login` / `set_new_password`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    /// When true, the UI must collect a new password and call `set_new_password`.
    pub requires_new_password: bool,
}

#[derive(Debug, Clone)]
struct Tokens {
    id_token: String,
    access_token: String,
    refresh_token: String,
    /// Unix seconds at which the access/ID tokens expire.
    expires_at: u64,
}

impl Tokens {
    fn is_expired(&self) -> bool {
        now_unix() + EXPIRY_SKEW_SECS >= self.expires_at
    }
}

struct Challenge {
    username: String,
    session: String,
}

#[derive(Default)]
struct Inner {
    tokens: Option<Tokens>,
    challenge: Option<Challenge>,
}

pub struct AuthManager {
    cognito: Cognito,
    inner: Mutex<Inner>,
}

impl AuthManager {
    pub fn new(region: &str, client_id: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build reqwest client");
        Self {
            cognito: Cognito::new(http, region, client_id),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Attempt to load a previously persisted session from the keyring. Called
    /// once at startup so a relaunch stays logged in. Validity is checked
    /// lazily on the first authenticated request (refresh-on-demand).
    pub async fn restore(&self) {
        let (Some(id_token), Some(access_token), Some(refresh_token), Some(expires)) = (
            keyring::load(K_ID),
            keyring::load(K_ACCESS),
            keyring::load(K_REFRESH),
            keyring::load(K_EXPIRES),
        ) else {
            return;
        };
        let expires_at = expires.parse().unwrap_or(0);
        let mut inner = self.inner.lock().await;
        inner.tokens = Some(Tokens {
            id_token,
            access_token,
            refresh_token,
            expires_at,
        });
        tracing::info!("restored persisted session from keyring");
    }

    /// `USER_PASSWORD_AUTH`. On success persists tokens; on first-login returns
    /// `requires_new_password` and stashes the challenge for `complete_new_password`.
    pub async fn login(&self, email: &str, password: &str) -> AppResult<LoginResult> {
        let username = email.trim();
        match self.cognito.initiate_password_auth(username, password).await? {
            AuthOutcome::Tokens(raw) => {
                self.set_session(username, raw, None).await?;
                Ok(LoginResult {
                    requires_new_password: false,
                })
            }
            AuthOutcome::NewPasswordRequired { session } => {
                let mut inner = self.inner.lock().await;
                inner.challenge = Some(Challenge {
                    username: username.to_string(),
                    session,
                });
                Ok(LoginResult {
                    requires_new_password: true,
                })
            }
        }
    }

    /// Complete the pending `NEW_PASSWORD_REQUIRED` challenge.
    pub async fn complete_new_password(&self, new_password: &str) -> AppResult<()> {
        let (username, session) = {
            let inner = self.inner.lock().await;
            let ch = inner
                .challenge
                .as_ref()
                .ok_or_else(|| AppError::Message("No password challenge in progress.".into()))?;
            (ch.username.clone(), ch.session.clone())
        };

        let raw = self
            .cognito
            .respond_new_password(&username, new_password, &session)
            .await?;
        self.set_session(&username, raw, None).await?;
        self.inner.lock().await.challenge = None;
        Ok(())
    }

    /// Clear the in-memory session and wipe persisted tokens.
    pub async fn logout(&self) {
        {
            let mut inner = self.inner.lock().await;
            inner.tokens = None;
            inner.challenge = None;
        }
        keyring::delete(K_ID);
        keyring::delete(K_ACCESS);
        keyring::delete(K_REFRESH);
        keyring::delete(K_EXPIRES);
    }

    /// Whether a session currently exists (does not check token validity).
    pub async fn is_authenticated(&self) -> bool {
        self.inner.lock().await.tokens.is_some()
    }

    /// Guard for post-auth commands. Returns `NotAuthenticated` if no session.
    /// (Reserved primitive — the API layer currently gates via `valid_id_token`.)
    #[allow(dead_code)]
    pub async fn require_auth(&self) -> AppResult<()> {
        if self.is_authenticated().await {
            Ok(())
        } else {
            Err(AppError::NotAuthenticated)
        }
    }

    /// Return a valid ID token, refreshing if it has expired. This is the entry
    /// point the API client (M2) calls before every request. Singleflight is
    /// implicit: the lock serializes concurrent callers so only one refresh
    /// round-trips Cognito. On refresh failure the session is cleared and
    /// `Unauthorized` is returned so the caller can emit `auth:expired`.
    #[allow(dead_code)] // consumed by the API client in M2
    pub async fn valid_id_token(&self) -> AppResult<String> {
        let mut inner = self.inner.lock().await;
        let tokens = inner.tokens.as_ref().ok_or(AppError::NotAuthenticated)?;

        if !tokens.is_expired() {
            return Ok(tokens.id_token.clone());
        }

        let refresh_token = tokens.refresh_token.clone();
        match self.cognito.refresh(&refresh_token).await {
            Ok(raw) => {
                // Refresh responses omit the refresh token — keep the old one.
                let updated = build_tokens(raw, Some(refresh_token));
                persist(&updated);
                let id = updated.id_token.clone();
                inner.tokens = Some(updated);
                Ok(id)
            }
            Err(_) => {
                inner.tokens = None;
                drop(inner);
                self.logout().await;
                Err(AppError::Unauthorized)
            }
        }
    }

    async fn set_session(
        &self,
        _username: &str,
        raw: RawTokens,
        prior_refresh: Option<String>,
    ) -> AppResult<()> {
        let tokens = build_tokens(raw, prior_refresh);
        persist(&tokens);
        self.inner.lock().await.tokens = Some(tokens);
        Ok(())
    }
}

fn build_tokens(raw: RawTokens, prior_refresh: Option<String>) -> Tokens {
    let refresh_token = raw
        .refresh_token
        .or(prior_refresh)
        .unwrap_or_default();
    Tokens {
        id_token: raw.id_token,
        access_token: raw.access_token,
        refresh_token,
        expires_at: now_unix() + raw.expires_in.max(0) as u64,
    }
}

/// Best-effort persistence to the keyring. Failures are logged, not fatal —
/// the session still works for this run, it just won't survive a restart.
fn persist(t: &Tokens) {
    let writes = [
        (K_ID, t.id_token.as_str()),
        (K_ACCESS, t.access_token.as_str()),
        (K_REFRESH, t.refresh_token.as_str()),
    ];
    for (key, val) in writes {
        if let Err(e) = keyring::store(key, val) {
            tracing::warn!(key, error = %e, "failed to persist token to keyring");
            return;
        }
    }
    let _ = keyring::store(K_EXPIRES, &t.expires_at.to_string());
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
