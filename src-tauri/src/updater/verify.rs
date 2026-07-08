//! Update integrity: SHA-256 checksum + ed25519 signature over `SHA256SUMS`.

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Embedded release public key file. First non-comment, non-empty line is the
/// base64 key; empty means no key is configured (see `release.pub`).
const RELEASE_PUBKEY_FILE: &str = include_str!("../../release.pub");

fn pubkey_b64() -> &'static str {
    for line in RELEASE_PUBKEY_FILE.lines() {
        let l = line.trim();
        if !l.is_empty() && !l.starts_with('#') {
            return l;
        }
    }
    ""
}

/// Whether a release public key is configured (signature checks are possible).
pub fn pubkey_present() -> bool {
    !pubkey_b64().is_empty()
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Look up the expected hash for `filename` in a `SHA256SUMS` body. Lines are
/// `<hex>  [*]<name>`; blanks and `#` comments are skipped; match is
/// case-insensitive on the filename.
pub fn expected_hash(sha256sums: &str, filename: &str) -> Option<String> {
    for line in sha256sums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name.eq_ignore_ascii_case(filename) {
            return Some(hash.to_lowercase());
        }
    }
    None
}

/// Verify the ed25519 signature (base64) of `body` against the embedded key.
pub fn verify_signature(body: &[u8], signature_b64: &[u8]) -> Result<(), String> {
    let key_raw = base64::engine::general_purpose::STANDARD
        .decode(pubkey_b64())
        .map_err(|e| format!("release pubkey base64: {e}"))?;
    let key: [u8; 32] = key_raw
        .as_slice()
        .try_into()
        .map_err(|_| "release pubkey wrong length".to_string())?;
    let vk = VerifyingKey::from_bytes(&key).map_err(|e| format!("bad release pubkey: {e}"))?;

    let sig_text = std::str::from_utf8(signature_b64).unwrap_or("").trim();
    let sig_raw = base64::engine::general_purpose::STANDARD
        .decode(sig_text)
        .map_err(|e| format!("signature base64: {e}"))?;
    let sig_bytes: [u8; 64] = sig_raw
        .as_slice()
        .try_into()
        .map_err(|_| "signature wrong length".to_string())?;
    let sig = Signature::from_bytes(&sig_bytes);

    vk.verify(body, &sig)
        .map_err(|_| "signature does not verify against release public key".to_string())
}
