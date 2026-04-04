use axum::http::StatusCode;
use serde::Serialize;

use crate::error::AppError;

pub fn canonicalize(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize(&map[&k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize).collect())
        }
        _ => v.clone(),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn payload_hash<T: Serialize>(req: &T) -> Result<String, AppError> {
    let v = serde_json::to_value(req).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let canon = canonicalize(&v);
    let bytes = serde_json::to_vec(&canon).map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

pub fn hash_percent(s: &str) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut hasher = fnv::FnvHasher::default();
    s.as_bytes().hash(&mut hasher);
    (hasher.finish() as u32) % 100
}

pub fn fmt_rfc3339(dt: time::OffsetDateTime) -> String {
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

// Keep the old StatusCode-based signature available during migration.
// Handlers that haven't migrated to AppError yet can use this.
pub fn payload_hash_compat<T: Serialize>(req: &T) -> Result<String, StatusCode> {
    payload_hash(req).map_err(|_| StatusCode::BAD_REQUEST)
}
