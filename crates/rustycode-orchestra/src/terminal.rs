//! Orchestra Terminal — Terminal capability detection for keyboard shortcut support.
//!
//! Ctrl+Alt shortcuts require the Kitty keyboard protocol or modifyOtherKeys.
//! Terminals that lack this support silently swallow the key combos.
//!
//! Matches orchestra-2's terminal.ts implementation.

use std::env;

// ─── Terminal Detection ───────────────────────────────────────────────────────

/// Terminals that don't support Ctrl+Alt shortcuts
const UNSUPPORTED_TERMS: &[&str] = &["apple_terminal", "warpterm"];

/// Check if the current terminal supports Ctrl+Alt keyboard shortcuts.
///
/// Returns `false` for known unsupported terminals (Apple Terminal, WarpTerm)
/// and JetBrains IDE terminals. Returns `true` for most modern terminals
/// that support the Kitty keyboard protocol or modifyOtherKeys.
///
/// # Returns
/// `true` if Ctrl+Alt shortcuts are supported, `false` otherwise
///
/// # Examples
/// ```
/// use rustycode_orchestra::terminal::supports_ctrl_alt_shortcuts;
///
/// let supported = supports_ctrl_alt_shortcuts();
/// if !supported {
///     println!("Ctrl+Alt shortcuts may not work in this terminal");
/// }
/// ```
pub fn supports_ctrl_alt_shortcuts() -> bool {
    supports_ctrl_alt_shortcuts_from_env(
        env::var("TERM_PROGRAM").ok().as_deref(),
        env::var("TERMINAL_EMULATOR").ok().as_deref(),
    )
}

/// Pure-logic version of [`supports_ctrl_alt_shortcuts`] that takes explicit
/// env-var values instead of reading from the process environment.
///
/// This exists so tests can pass values directly instead of mutating global
/// `std::env` vars, which causes race conditions under parallel test runners.
fn supports_ctrl_alt_shortcuts_from_env(
    term_program: Option<&str>,
    terminal_emulator: Option<&str>,
) -> bool {
    if let Some(term) = term_program {
        let term_lower = term.to_lowercase();
        if UNSUPPORTED_TERMS.iter().any(|t| term_lower.contains(t)) {
            return false;
        }
    }

    if let Some(emulator) = terminal_emulator {
        let emulator_lower = emulator.to_lowercase();
        if emulator_lower.contains("jetbrains") {
            return false;
        }
    }

    true
}

pub fn shortcut_desc(base: &str, fallback_cmd: &str) -> String {
    shortcut_desc_from_supported(supports_ctrl_alt_shortcuts(), base, fallback_cmd)
}

fn shortcut_desc_from_supported(supported: bool, base: &str, fallback_cmd: &str) -> String {
    if supported {
        base.to_string()
    } else {
        format!(
            "{} — shortcut may not work in this terminal, use {}",
            base, fallback_cmd
        )
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_ctrl_alt_shortcuts_default() {
        assert!(
            supports_ctrl_alt_shortcuts_from_env(None, None),
            "Should return true when no environment variables are set"
        );
    }

    #[test]
    fn test_supports_ctrl_alt_shortcuts_apple_terminal() {
        assert!(!supports_ctrl_alt_shortcuts_from_env(
            Some("Apple_Terminal"),
            None
        ));
    }

    #[test]
    fn test_supports_ctrl_alt_shortcuts_warpterm() {
        assert!(!supports_ctrl_alt_shortcuts_from_env(
            Some("WarpTerminal"),
            None
        ));
    }

    #[test]
    fn test_supports_ctrl_alt_shortcuts_jetbrains() {
        assert!(!supports_ctrl_alt_shortcuts_from_env(
            None,
            Some("JetBrains IDE")
        ));
    }

    #[test]
    fn test_supports_ctrl_alt_shortcuts_iterm() {
        assert!(supports_ctrl_alt_shortcuts_from_env(
            Some("iTerm.app"),
            None
        ));
    }

    #[test]
    fn test_shortcut_desc_supported() {
        let desc = shortcut_desc_from_supported(true, "Ctrl+Alt+S", "/save");
        assert_eq!(desc, "Ctrl+Alt+S");
    }

    #[test]
    fn test_shortcut_desc_unsupported() {
        let desc = shortcut_desc_from_supported(false, "Ctrl+Alt+S", "/save");
        assert!(desc.contains("shortcut may not work"));
        assert!(desc.contains("/save"));
    }

    #[test]
    fn test_shortcut_desc_default() {
        let desc = shortcut_desc_from_supported(true, "Ctrl+Alt+S", "/save");
        assert_eq!(desc, "Ctrl+Alt+S");
    }
}
