//! Orchestra Extension — Shared Constants
//!
//! Centralized timeout and cache-size constants used across the Orchestra extension.

// ─── Timeouts ─────────────────────────────────────────────────────────────────

/// Default timeout for verification-gate commands (ms)
pub const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 120_000;

/// Default timeout for the dynamic bash tool (seconds)
pub const DEFAULT_BASH_TIMEOUT_SECS: u64 = 120;

// ─── Cache Sizes ──────────────────────────────────────────────────────────────

/// Max directory-listing cache entries before eviction (#611)
pub const DIR_CACHE_MAX: usize = 200;

/// Max parse-cache entries before eviction
pub const CACHE_MAX: usize = 50;

// ─── Auto Mode ─────────────────────────────────────────────────────────────────

/// Throttle STATE.md rebuilds — at most once per 30 seconds
pub const STATE_REBUILD_MIN_INTERVAL_MS: u64 = 30_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_constants() {
        assert_eq!(DEFAULT_COMMAND_TIMEOUT_MS, 120_000);
        assert_eq!(DEFAULT_BASH_TIMEOUT_SECS, 120);
    }

    #[test]
    fn test_cache_constants() {
        assert_eq!(DIR_CACHE_MAX, 200);
        assert_eq!(CACHE_MAX, 50);
    }

    #[test]
    fn test_auto_mode_constants() {
        assert_eq!(STATE_REBUILD_MIN_INTERVAL_MS, 30_000);
    }
}
