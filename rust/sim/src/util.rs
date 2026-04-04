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

/// Deterministic hash to percentage (0-99).
/// FNV-1a 32-bit on raw bytes, matching Go's fnv.New32a() + Write([]byte(s)) + Sum32().
pub fn hash_percent(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261; // FNV offset basis
    for &b in s.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16_777_619); // FNV prime
    }
    h % 100
}

pub fn fmt_rfc3339(dt: time::OffsetDateTime) -> String {
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cross-language parity: these values must match Go's hashPercent output.
    // Verified via: go run with fnv.New32a().Write([]byte(s)); h.Sum32() % 100
    #[test]
    fn hash_percent_matches_go() {
        assert_eq!(hash_percent("req-0001"), 73);
        assert_eq!(hash_percent("test-req-001"), 22);
        assert_eq!(hash_percent("abc"), 31);
        assert_eq!(hash_percent(""), 61);
    }

    #[test]
    fn hash_percent_range() {
        for i in 0..1000 {
            let s = format!("req-{i:04}");
            let p = hash_percent(&s);
            assert!(p < 100, "hash_percent({s}) = {p}, expected < 100");
        }
    }

    #[test]
    fn hash_percent_deterministic() {
        let a = hash_percent("same-input");
        let b = hash_percent("same-input");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_percent_distribution() {
        let mut buckets = [0u32; 10];
        for i in 0..10_000 {
            let s = format!("load-test-{i}");
            let p = hash_percent(&s) / 10;
            buckets[p as usize] += 1;
        }
        // each decile should get roughly 1000; fail if any is < 500 (extreme skew)
        for (i, count) in buckets.iter().enumerate() {
            assert!(*count > 500, "bucket {i} has only {count} entries, distribution is skewed");
        }
    }

    #[test]
    fn canonicalize_stable_key_order() {
        let a = serde_json::json!({"b": 2, "a": 1});
        let b = serde_json::json!({"a": 1, "b": 2});
        assert_eq!(
            serde_json::to_string(&canonicalize(&a)).unwrap(),
            serde_json::to_string(&canonicalize(&b)).unwrap()
        );
    }

    #[test]
    fn canonicalize_nested() {
        let v = serde_json::json!({"z": {"b": 1, "a": 2}, "a": [3, 2, 1]});
        let c = canonicalize(&v);
        let s = serde_json::to_string(&c).unwrap();
        // keys sorted: a before z, nested b before... wait, a before b
        assert_eq!(s, r#"{"a":[3,2,1],"z":{"a":2,"b":1}}"#);
    }

    #[test]
    fn payload_hash_deterministic() {
        #[derive(serde::Serialize)]
        struct Req { id: String, amount: i64 }
        let r1 = Req { id: "x".into(), amount: 100 };
        let r2 = Req { id: "x".into(), amount: 100 };
        assert_eq!(payload_hash(&r1).unwrap(), payload_hash(&r2).unwrap());
    }

    #[test]
    fn payload_hash_different_input() {
        #[derive(serde::Serialize)]
        struct Req { id: String, amount: i64 }
        let r1 = Req { id: "x".into(), amount: 100 };
        let r2 = Req { id: "x".into(), amount: 200 };
        assert_ne!(payload_hash(&r1).unwrap(), payload_hash(&r2).unwrap());
    }

    #[test]
    fn sha256_hex_known_value() {
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}
