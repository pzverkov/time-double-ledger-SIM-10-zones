//! Centralized domain constants.

use std::time::Duration;

/// Cross-zone throttle is a percentage in `0..=THROTTLE_MAX_PCT`.
/// `THROTTLE_MAX_PCT` means no throttling (all requests pass); `0` blocks all.
pub const THROTTLE_MAX_PCT: i32 = 100;

/// Maximum wall-clock for an HTTP request before it is cut with 408.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Maximum wall-clock for the readiness DB check (bounds a busy/exhausted pool).
pub const READYZ_TIMEOUT: Duration = Duration::from_secs(2);

/// Per-publish timeout inside an outbox batch. Must stay below
/// `DB_IDLE_TX_TIMEOUT_MS` so a hung broker cannot pin the transaction's
/// connection until Postgres kills it.
pub const PUBLISH_TIMEOUT: Duration = Duration::from_secs(5);

/// Postgres `statement_timeout` applied to every pooled connection.
pub const DB_STATEMENT_TIMEOUT_MS: u32 = 15_000;

/// Postgres `idle_in_transaction_session_timeout` applied to every pooled
/// connection. Comfortably above `PUBLISH_TIMEOUT` * a small batch.
pub const DB_IDLE_TX_TIMEOUT_MS: u32 = 30_000;

/// How often the retention job prunes old rows.
pub const RETENTION_INTERVAL: Duration = Duration::from_secs(3600);

/// Published outbox rows and processed inbox rows older than this are deleted.
pub const RETENTION_SECS: i64 = 7 * 24 * 3600;

/// Dead-letter rows are kept longer for investigation.
pub const DLQ_RETENTION_SECS: i64 = 30 * 24 * 3600;

/// Known weak/example admin keys that must never guard a production deployment.
pub const WEAK_ADMIN_KEYS: &[&str] = &[
    "dev-admin-key",
    "test-admin-key",
    "changeme",
    "admin",
    "password",
];

/// Production safety check: when `APP_ENV=production`, the admin key must be set
/// and must not be a known weak default. Returns a human-readable error on a
/// misconfiguration so the binary can refuse to start. A no-op outside
/// production so the local demo keeps working with its dev defaults.
pub fn check_admin_key(app_env: &str, admin_key: Option<&str>) -> Result<(), String> {
    if app_env != "production" {
        return Ok(());
    }
    match admin_key {
        None | Some("") => Err("ADMIN_KEY must be set in production".into()),
        Some(k) if WEAK_ADMIN_KEYS.contains(&k) => Err(format!(
            "ADMIN_KEY is a known weak default ({k}); set a strong secret in production"
        )),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_key_check_is_noop_outside_production() {
        assert!(check_admin_key("development", None).is_ok());
        assert!(check_admin_key("", Some("dev-admin-key")).is_ok());
    }

    #[test]
    fn production_rejects_missing_or_weak_admin_key() {
        assert!(check_admin_key("production", None).is_err());
        assert!(check_admin_key("production", Some("")).is_err());
        assert!(check_admin_key("production", Some("dev-admin-key")).is_err());
        assert!(check_admin_key("production", Some("admin")).is_err());
    }

    #[test]
    fn production_accepts_a_strong_admin_key() {
        assert!(check_admin_key("production", Some("s3cr3t-long-random-value")).is_ok());
    }
}
