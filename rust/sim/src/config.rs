//! Centralized domain constants.

/// Cross-zone throttle is a percentage in `0..=THROTTLE_MAX_PCT`.
/// `THROTTLE_MAX_PCT` means no throttling (all requests pass); `0` blocks all.
pub const THROTTLE_MAX_PCT: i32 = 100;
