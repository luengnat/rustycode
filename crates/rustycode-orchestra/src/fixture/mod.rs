// Fixture-based testing system
//
// Inspired by Autonomous Mode's fixture provider - enables fast, deterministic tests
// without LLM calls by recording and replaying agent conversations.
//
// ## Usage
//
// ```rust
// use rustycode_orchestra::fixture::{FixtureProvider, FixtureMode};
//
// // Replay mode - use recorded responses
// let provider = FixtureProvider::new(
//     FixtureMode::Replay,
//     "tests/fixtures/recordings"
// );
//
// // Record mode - capture real LLM responses
// let provider = FixtureProvider::new(
//     FixtureMode::Record,
//     "tests/fixtures/recordings"
// );
// ```

pub mod provider;
pub mod recorder;
pub mod types;

pub use provider::FixtureProvider;
pub use recorder::FixtureRecorder;
pub use types::*;

/// Mode of operation for fixture testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FixtureMode {
    /// Replay recorded responses without LLM calls (fast, deterministic)
    Replay,
    /// Record LLM responses for future replay
    Record,
    /// Normal LLM mode (no fixture interaction)
    Off,
}

impl FixtureMode {
    /// Detect fixture mode from environment variable
    pub fn from_env() -> Self {
        match std::env::var("Orchestra_FIXTURE_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "replay" => FixtureMode::Replay,
            "record" => FixtureMode::Record,
            _ => FixtureMode::Off,
        }
    }
}

/// Get the fixture recordings directory
pub fn fixture_dir() -> std::path::PathBuf {
    std::env::var("Orchestra_FIXTURE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("tests/fixtures/recordings"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_mode_from_env() {
        assert_eq!(FixtureMode::from_env(), FixtureMode::Off);

        std::env::set_var("Orchestra_FIXTURE_MODE", "replay");
        assert_eq!(FixtureMode::from_env(), FixtureMode::Replay);

        std::env::set_var("Orchestra_FIXTURE_MODE", "record");
        assert_eq!(FixtureMode::from_env(), FixtureMode::Record);

        std::env::set_var("Orchestra_FIXTURE_MODE", "invalid");
        assert_eq!(FixtureMode::from_env(), FixtureMode::Off);

        std::env::remove_var("Orchestra_FIXTURE_MODE");
    }

    #[test]
    fn test_fixture_dir() {
        assert_eq!(
            fixture_dir(),
            std::path::PathBuf::from("tests/fixtures/recordings")
        );

        std::env::set_var("Orchestra_FIXTURE_DIR", "/tmp/fixtures");
        assert_eq!(fixture_dir(), std::path::PathBuf::from("/tmp/fixtures"));

        std::env::remove_var("Orchestra_FIXTURE_DIR");
    }
}
