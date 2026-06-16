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
