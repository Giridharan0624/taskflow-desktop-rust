//! Encrypted-at-rest token persistence via the OS keyring.
//!
//! Windows Credential Manager / macOS Keychain / Linux Secret Service, through
//! `keyring-rs`. Two portability concerns are handled here:
//!
//!   1. **Size** — Cognito ID + refresh tokens exceed the Windows credential
//!      blob limit (~2.5 KB), so every value is base64-encoded and split into
//!      fixed-size chunks stored as `key.0..key.N` plus a `key.count` entry
//!      (mirrors the Go app's chunking scheme).
//!   2. **At-rest encryption** — each backend encrypts per-user natively
//!      (Credential Manager uses DPAPI under the hood, Keychain/Secret Service
//!      likewise). NOTE: the Go app adds an *extra* explicit DPAPI wrap on
//!      Windows; that belt-and-suspenders layer is deferred (see M-hardening) —
//!      tokens are still encrypted at rest by Credential Manager today.

use base64::Engine;

use crate::error::AppError;

const SERVICE: &str = "TaskFlowDesktop";
/// Base64 chunk length — comfortably under the Windows credential blob cap.
const CHUNK: usize = 2000;

fn entry(account: &str) -> Result<keyring::Entry, AppError> {
    keyring::Entry::new(SERVICE, account)
        .map_err(|e| AppError::Message(format!("keyring unavailable: {e}")))
}

/// Store `value` under `key`, chunked. Overwrites any previous value.
pub fn store(key: &str, value: &str) -> Result<(), AppError> {
    // Clear any stale chunks from a longer previous value first.
    delete(key);

    let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
    let chunks: Vec<&str> = split_chunks(&encoded, CHUNK);

    entry(&count_key(key))?
        .set_password(&chunks.len().to_string())
        .map_err(|e| AppError::Message(format!("keyring write failed: {e}")))?;

    for (i, chunk) in chunks.iter().enumerate() {
        entry(&chunk_key(key, i))?
            .set_password(chunk)
            .map_err(|e| AppError::Message(format!("keyring write failed: {e}")))?;
    }
    Ok(())
}

/// Load a chunked value, or `None` if absent/unreadable.
pub fn load(key: &str) -> Option<String> {
    let count: usize = entry(&count_key(key)).ok()?.get_password().ok()?.parse().ok()?;
    let mut encoded = String::new();
    for i in 0..count {
        let part = entry(&chunk_key(key, i)).ok()?.get_password().ok()?;
        encoded.push_str(&part);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .ok()?;
    String::from_utf8(bytes).ok()
}

/// Remove all chunks for `key`. Best-effort; missing entries are ignored.
pub fn delete(key: &str) {
    let count: usize = entry(&count_key(key))
        .ok()
        .and_then(|e| e.get_password().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    for i in 0..count {
        if let Ok(e) = entry(&chunk_key(key, i)) {
            let _ = e.delete_credential();
        }
    }
    if let Ok(e) = entry(&count_key(key)) {
        let _ = e.delete_credential();
    }
}

fn count_key(key: &str) -> String {
    format!("{key}.count")
}

fn chunk_key(key: &str, i: usize) -> String {
    format!("{key}.{i}")
}

fn split_chunks(s: &str, size: usize) -> Vec<&str> {
    if s.is_empty() {
        return vec![""];
    }
    let bytes = s.as_bytes();
    (0..bytes.len())
        .step_by(size)
        .map(|start| {
            let end = (start + size).min(bytes.len());
            // base64 output is pure ASCII, so byte slicing is char-safe.
            std::str::from_utf8(&bytes[start..end]).expect("base64 is ascii")
        })
        .collect()
}
