//! Help text — CLI help and usage documentation.
//!
//! Provides help text for the Orchestra CLI including main help and
//! subcommand-specific help.
//!
//! Matches orchestra-2's help-text.ts implementation.

/// Get help text for a subcommand
///
/// # Arguments
/// * `subcommand` - Subcommand name (e.g., "config", "update", "sessions")
///
/// # Returns
/// Help text for the subcommand, or None if not found
///
/// # Examples
/// ```
/// use rustycode_orchestra::help_text::get_subcommand_help;
///
/// if let Some(help) = get_subcommand_help("config") {
///     println!("{}", help);
/// }
/// ```
pub fn get_subcommand_help(subcommand: &str) -> Option<&'static str> {
    match subcommand {
        "config" | "setup" => Some(CONFIG_HELP),
        "update" => Some(UPDATE_HELP),
        "sessions" => Some(SESSIONS_HELP),
        "worktree" | "wt" => Some(WORKTREE_HELP),
        "headless" => Some(HEADLESS_HELP),
        _ => None,
    }
}

/// Format main help text
///
/// # Arguments
/// * `version` - Version string to display
///
/// # Returns
/// Formatted main help text
///
/// # Examples
/// ```
/// use rustycode_orchestra::help_text::format_main_help;
///
/// let help = format_main_help("1.0.0");
/// println!("{}", help);
/// ```
pub fn format_main_help(version: &str) -> String {
    format!(
        "Orchestra v{} — Get Shit Done\n\n\
         Usage: orchestra [options] [message...]\n\n\
         Options:\n\
           --mode <text|json|rpc|mcp> Output mode (default: interactive)\n\
           --print, -p              Single-shot print mode\n\
           --continue, -c           Resume the most recent session\n\
           --worktree, -w [name]    Start in an isolated worktree (auto-named if omitted)\n\
           --model <id>             Override model (e.g. claude-opus-4-6)\n\
           --no-session             Disable session persistence\n\
           --extension <path>       Load additional extension\n\
           --tools <a,b,c>          Restrict available tools\n\
           --list-models [search]   List available models and exit\n\
           --version, -v            Print version and exit\n\
           --help, -h               Print this help and exit\n\n\
         Subcommands:\n\
           config                   Re-run the setup wizard\n\
           update                   Update Orchestra to the latest version\n\
           sessions                 List and resume a past session\n\
           worktree <cmd>           Manage worktrees (list, merge, clean, remove)\n\
           headless [cmd] [args]    Run /orchestra commands without TUI (default: auto)\n\n\
         Run orchestra <subcommand> --help for subcommand-specific help.\n",
        version
    )
}

/// Format subcommand help text
///
/// # Arguments
/// * `subcommand` - Subcommand name
/// * `version` - Version string to display
///
/// # Returns
/// Formatted subcommand help text, or None if subcommand not found
///
/// # Examples
/// ```
/// use rustycode_orchestra::help_text::format_subcommand_help;
///
/// if let Some(help) = format_subcommand_help("config", "1.0.0") {
///     println!("{}", help);
/// }
/// ```
pub fn format_subcommand_help(subcommand: &str, version: &str) -> Option<String> {
    let help_text = get_subcommand_help(subcommand)?;
    Some(format!(
        "Orchestra v{} — Get Shit Done\n\n{}\n",
        version, help_text
    ))
}

// ─── Help Text Constants ───────────────────────────────────────────────────────

/// Config subcommand help
const CONFIG_HELP: &str = "\
Usage: orchestra config\n\n\
Re-run the interactive setup wizard to configure:\n\
  - LLM provider (Anthropic, OpenAI, Google, etc.)\n\
  - Web search provider (Brave, Tavily, built-in)\n\
  - Remote questions (Discord, Slack, Telegram)\n\
  - Tool API keys (Context7, Jina, Groq)\n\
All steps are skippable and can be changed later with /login or /search-provider.";

/// Update subcommand help
const UPDATE_HELP: &str = "\
Usage: orchestra update\n\n\
Update Orchestra to the latest version.\n\n\
Equivalent to: npm install -g orchestra-pi@latest";

/// Sessions subcommand help
const SESSIONS_HELP: &str = "\
Usage: orchestra sessions\n\n\
List all saved sessions for the current directory and interactively\n\
pick one to resume. Shows date, message count, and a preview of the\n\
first message for each session.\n\n\
Sessions are stored per-directory, so you only see sessions that were\n\
started from the current working directory.\n\n\
Compare with --continue (-c) which always resumes the most recent session.";

/// Worktree subcommand help
const WORKTREE_HELP: &str = "\
Usage: orchestra worktree <command> [args]\n\n\
Manage isolated git worktrees for parallel work streams.\n\n\
Commands:\n\
  list                 List worktrees with status (files changed, commits, dirty)\n\
  merge [name]         Squash-merge a worktree into main and clean up\n\
  clean                Remove all worktrees that have been merged or are empty\n\
  remove <name>        Remove a worktree (--force to remove with unmerged changes)\n\n\
The -w flag creates/resumes worktrees for interactive sessions:\n\
  orchestra -w               Auto-name a new worktree, or resume the only active one\n\
  orchestra -w my-feature    Create or resume a named worktree\n\n\
Lifecycle:\n\
  1. orchestra -w             Create worktree, start session inside it\n\
  2. (work normally)    All changes happen on the worktree branch\n\
  3. Ctrl+C             Exit — dirty work is auto-committed\n\
  4. orchestra -w             Resume where you left off\n\
  5. orchestra worktree merge Squash-merge into main when done\n\n\
Examples:\n\
  orchestra -w                              Start in a new auto-named worktree\n\
  orchestra -w auth-refactor                Create/resume \"auth-refactor\" worktree\n\
  orchestra worktree list                   See all worktrees and their status\n\
  orchestra worktree merge auth-refactor    Merge and clean up\n\
  orchestra worktree clean                  Remove all merged/empty worktrees\n\
  orchestra worktree remove old-branch      Remove a specific worktree\n\
  orchestra worktree remove old-branch --force  Remove even with unmerged changes";

/// Headless subcommand help
const HEADLESS_HELP: &str = "\
Usage: orchestra headless [flags] [command] [args...]\n\n\
Run /orchestra commands without the TUI. Default command: auto\n\n\
Flags:\n\
  --timeout N          Overall timeout in ms (default: 300000)\n\
  --json               JSONL event stream to stdout\n\
  --model ID           Override model\n\
  --supervised           Forward interactive UI requests to orchestrator via stdout/stdin\n\
  --response-timeout N   Timeout (ms) for orchestrator response (default: 30000)\n\
  --answers <path>       Pre-supply answers and secrets (JSON file)\n\
  --events <types>       Filter JSONL output to specific event types (comma-separated)\n\n\
Commands:\n\
  auto                 Run all queued units continuously (default)\n\
  next                 Run one unit\n\
  status               Show progress dashboard\n\
  new-milestone        Create a milestone from a specification document\n\
  query                JSON snapshot: state + next dispatch + costs (no LLM)\n\n\
new-milestone flags:\n\
  --context <path>     Path to spec/PRD file (use '-' for stdin)\n\
  --context-text <txt> Inline specification text\n\
  --auto               Start auto-mode after milestone creation\n\
  --verbose            Show tool calls in progress output\n\n\
Examples:\n\
  orchestra headless                                    Run /orchestra auto\n\
  orchestra headless next                               Run one unit\n\
  orchestra headless --json status                      Machine-readable status\n\
  orchestra headless --timeout 60000                    With 1-minute timeout\n\
  orchestra headless new-milestone --context spec.md    Create milestone from file\n\
  cat spec.md | orchestra headless new-milestone --context -   From stdin\n\
  orchestra headless new-milestone --context spec.md --auto    Create + auto-execute\n\
  orchestra headless --supervised auto                     Supervised orchestrator mode\n\
  orchestra headless --answers answers.json auto              With pre-supplied answers\n\
  orchestra headless --events agent_end,extension_ui_request auto   Filtered event stream\n\
  orchestra headless query                              Instant JSON state snapshot\n\n\
Exit codes: 0 = complete, 1 = error/timeout, 2 = blocked";

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_subcommand_help_config() {
        let help = get_subcommand_help("config");
        assert!(help.is_some());
        assert!(help.unwrap().contains("setup wizard"));
    }

    #[test]
    fn test_get_subcommand_help_update() {
        let help = get_subcommand_help("update");
        assert!(help.is_some());
        assert!(help.unwrap().contains("Update Orchestra"));
    }

    #[test]
    fn test_get_subcommand_help_sessions() {
        let help = get_subcommand_help("sessions");
        assert!(help.is_some());
        assert!(help.unwrap().contains("List all saved sessions"));
    }

    #[test]
    fn test_get_subcommand_help_worktree() {
        let help = get_subcommand_help("worktree");
        assert!(help.is_some());
        assert!(help.unwrap().contains("Manage isolated git worktrees"));
    }

    #[test]
    fn test_get_subcommand_help_headless() {
        let help = get_subcommand_help("headless");
        assert!(help.is_some());
        assert!(help
            .unwrap()
            .contains("Run /orchestra commands without the TUI"));
    }

    #[test]
    fn test_get_subcommand_help_wt_alias() {
        let help = get_subcommand_help("wt");
        assert!(help.is_some());
        // "wt" should map to same help as "worktree"
        let wt_help = get_subcommand_help("worktree").unwrap();
        assert_eq!(help.unwrap(), wt_help);
    }

    #[test]
    fn test_get_subcommand_help_unknown() {
        let help = get_subcommand_help("unknown");
        assert!(help.is_none());
    }

    #[test]
    fn test_get_subcommand_help_setup_alias() {
        let help = get_subcommand_help("setup");
        assert!(help.is_some());
        // "setup" should map to same help as "config"
        let config_help = get_subcommand_help("config").unwrap();
        assert_eq!(help.unwrap(), config_help);
    }

    #[test]
    fn test_format_main_help() {
        let help = format_main_help("1.0.0");
        assert!(help.contains("Orchestra v1.0.0"));
        assert!(help.contains("Get Shit Done"));
        assert!(help.contains("Usage: orchestra"));
        assert!(help.contains("--help"));
    }

    #[test]
    fn test_format_main_help_with_different_version() {
        let help = format_main_help("2.0.0-beta");
        assert!(help.contains("Orchestra v2.0.0-beta"));
    }

    #[test]
    fn test_format_subcommand_help() {
        let help = format_subcommand_help("config", "1.0.0");
        assert!(help.is_some());
        let help_text = help.unwrap();
        assert!(help_text.contains("Orchestra v1.0.0"));
        assert!(help_text.contains("setup wizard"));
    }

    #[test]
    fn test_format_subcommand_help_unknown() {
        let help = format_subcommand_help("unknown", "1.0.0");
        assert!(help.is_none());
    }

    #[test]
    fn test_config_help_content() {
        let help = CONFIG_HELP;
        assert!(help.contains("LLM provider"));
        assert!(help.contains("Web search provider"));
        assert!(help.contains("/login"));
    }

    #[test]
    fn test_update_help_content() {
        let help = UPDATE_HELP;
        assert!(help.contains("Update Orchestra"));
        assert!(help.contains("npm install"));
    }

    #[test]
    fn test_sessions_help_content() {
        let help = SESSIONS_HELP;
        assert!(help.contains("List all saved sessions"));
        assert!(help.contains("--continue"));
    }

    #[test]
    fn test_worktree_help_content() {
        let help = WORKTREE_HELP;
        assert!(help.contains("git worktrees"));
        assert!(help.contains("list"));
        assert!(help.contains("merge"));
        assert!(help.contains("clean"));
        assert!(help.contains("remove"));
    }

    #[test]
    fn test_headless_help_content() {
        let help = HEADLESS_HELP;
        assert!(help.contains("--timeout"));
        assert!(help.contains("--json"));
        assert!(help.contains("--model"));
        assert!(help.contains("auto"));
        assert!(help.contains("next"));
        assert!(help.contains("status"));
    }

    #[test]
    fn test_all_subcommands_have_help() {
        let subcommands = ["config", "update", "sessions", "worktree", "headless"];
        for &subcommand in &subcommands {
            let help = get_subcommand_help(subcommand);
            assert!(
                help.is_some(),
                "Subcommand '{}' should have help text",
                subcommand
            );
        }
    }

    #[test]
    fn test_help_text_is_static() {
        // Verify that all help text is &'static str (embedded in binary)
        let _ = CONFIG_HELP;
        let _ = UPDATE_HELP;
        let _ = SESSIONS_HELP;
        let _ = WORKTREE_HELP;
        let _ = HEADLESS_HELP;
    }

    #[test]
    fn test_worktree_help_contains_examples() {
        let help = WORKTREE_HELP;
        assert!(help.contains("Examples:"));
        assert!(help.contains("orchestra -w"));
        assert!(help.contains("orchestra worktree list"));
    }

    #[test]
    fn test_headless_help_contains_examples() {
        let help = HEADLESS_HELP;
        assert!(help.contains("Examples:"));
        assert!(help.contains("orchestra headless"));
        assert!(help.contains("orchestra headless next"));
    }

    #[test]
    fn test_help_text_multiline() {
        let help = CONFIG_HELP;
        // Verify help text contains newlines for formatting
        assert!(help.contains('\n'));
    }

    #[test]
    fn test_subcommand_aliases() {
        // Test that "wt" maps to "worktree"
        let wt_help = get_subcommand_help("wt");
        let worktree_help = get_subcommand_help("worktree");
        assert_eq!(wt_help, worktree_help);

        // Test that "setup" maps to "config"
        let setup_help = get_subcommand_help("setup");
        let config_help = get_subcommand_help("config");
        assert_eq!(setup_help, config_help);
    }

    #[test]
    fn test_main_help_structure() {
        let help = format_main_help("1.0.0");

        // Verify main help has required sections
        assert!(help.contains("Options:"));
        assert!(help.contains("Subcommands:"));
        assert!(help.contains("--version"));
        assert!(help.contains("--help"));
        assert!(help.contains("config"));
        assert!(help.contains("update"));
        assert!(help.contains("sessions"));
        assert!(help.contains("worktree"));
        assert!(help.contains("headless"));
    }

    #[test]
    fn test_format_main_help_includes_version() {
        let help_v1 = format_main_help("1.0.0");
        let help_v2 = format_main_help("2.0.0");

        assert!(help_v1.contains("Orchestra v1.0.0"));
        assert!(help_v2.contains("Orchestra v2.0.0"));
        assert!(!help_v1.contains("Orchestra v2.0.0"));
        assert!(!help_v2.contains("Orchestra v1.0.0"));
    }

    #[test]
    fn test_subcommand_help_includes_version() {
        let help = format_subcommand_help("config", "1.2.3").unwrap();

        assert!(help.contains("Orchestra v1.2.3"));
        assert!(help.contains("Usage: orchestra config"));
    }

    #[test]
    fn test_worktree_help_contains_lifecycle() {
        let help = WORKTREE_HELP;
        assert!(help.contains("Lifecycle:"));
        assert!(help.contains("1. orchestra -w"));
        assert!(help.contains("2. (work normally)"));
        assert!(help.contains("3. Ctrl+C"));
        assert!(help.contains("4. orchestra -w"));
        assert!(help.contains("5. orchestra worktree merge"));
    }

    #[test]
    fn test_headless_help_contains_exit_codes() {
        let help = HEADLESS_HELP;
        assert!(help.contains("Exit codes:"));
        assert!(help.contains("0 = complete"));
        assert!(help.contains("1 = error/timeout"));
        assert!(help.contains("2 = blocked"));
    }

    #[test]
    fn test_headless_help_contains_all_commands() {
        let help = HEADLESS_HELP;
        assert!(help.contains("Commands:"));
        assert!(help.contains("auto"));
        assert!(help.contains("next"));
        assert!(help.contains("status"));
        assert!(help.contains("new-milestone"));
        assert!(help.contains("query"));
    }

    #[test]
    fn test_headless_help_contains_new_milestone_flags() {
        let help = HEADLESS_HELP;
        assert!(help.contains("new-milestone flags:"));
        assert!(help.contains("--context"));
        assert!(help.contains("--context-text"));
        assert!(help.contains("--auto"));
        assert!(help.contains("--verbose"));
    }
}
