//! Deterministic immutable-term evidence for domain audit events, not authentication.
use super::*;
use sha2::{Digest, Sha256};
pub fn record_fingerprint<T: Serialize>(record: &T) -> Result<String> {
    let bytes = serde_json::to_vec(record).map_err(|e| error(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
