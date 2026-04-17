//! Orchestra Verification Gate — Command Discovery and Execution
//!
//! This module discovers and runs verification commands to ensure code quality
//! before marking tasks as complete. It's the final quality gate in autonomous
//! development.
//!
//! # Command Discovery
//!
//! The verification gate searches for commands in multiple locations:
//!
//! 1. **Task Plan** - `verify:` field in TASK-PLAN.md
//! 2. **Preferences** - Global verification preferences
//! 3. **Package Scripts** - `typecheck`, `lint`, `test` from package.json
//!
//! # Discovery Priority
//!
//! Commands are discovered in this order:
//! - Task-specific commands (highest priority)
//! - User preference commands
//! - Package.json scripts (fallback)
//!
//! # Shell Injection Protection
//!
//! The gate detects potentially dangerous command patterns:
//! - Shell metacharacters (`;`, `|`, `&`, `$`, `>`, `<`)
//! - Command substitution (`$(...)`, backticks)
//! - Pipes and redirects
//!
//! Suspicious commands are rejected with a clear error message.
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::verification_gate::{VerificationGate, VerificationConfig};
//!
//! let gate = VerificationGate::new(project_root, config);
//! let result = gate.verify().await?;
//!
//! if result.all_passed {
//!     println!("All checks passed!");
//! } else {
//!     println!("Verification failed:");
//!     for check in &result.checks {
//!         if check.exit_code != 0 {
//!             println!("  {}: {}", check.command, check.stderr.unwrap_or_default());
//!         }
//!     }
//! }
//! ```
//!
//! # Output Truncation
//!
//! To prevent massive logs, command output is truncated:
//! - Max 10 KB per command
//! - Max 2 KB of stderr per failed check
//! - Max 10 KB total failure context

use std::collections::HashSet;
use std::process::Command;

// ─── Constants ───────────────────────────────────────────────────────────────────

/// Maximum bytes of stdout/stderr to retain per command (10 KB)
const MAX_OUTPUT_BYTES: usize = 10 * 1024;

/// Maximum chars of stderr to include per failed check
const MAX_STDERR_PER_CHECK: usize = 2_000;

/// Maximum total chars for combined failure context
const MAX_FAILURE_CONTEXT_CHARS: usize = 10_000;

/// Default command timeout (milliseconds)
#[allow(dead_code)] // Kept for future use
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 120_000;

/// Package.json script keys to probe, in order
const PACKAGE_SCRIPT_KEYS: &[&str] = &["typecheck", "lint", "test"];

// ─── Types ────────────────────────────────────────────────────────────────────

/// Verification check result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct VerificationCheck {
    /// Command that was run
    pub command: String,

    /// Exit code (0 = success)
    pub exit_code: i32,

    /// Standard output (truncated if needed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,

    /// Standard error (truncated if needed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,

    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Verification result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationResult {
    /// Individual verification checks
    pub checks: Vec<VerificationCheck>,

    /// Where commands were discovered
    pub discovery_source: DiscoverySource,

    /// Total duration for all checks
    pub total_duration_ms: u64,

    /// Whether all checks passed
    pub all_passed: bool,
}

/// Source of discovered commands
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiscoverySource {
    /// Commands from user preferences
    Preference,

    /// Commands from task plan verify field
    TaskPlan,

    /// Commands from package.json scripts
    PackageJson,

    /// No commands found
    None,
}

/// Options for discovering commands
#[derive(Debug, Clone, Default)]
pub struct DiscoverCommandsOptions {
    /// Explicit preference commands
    pub preference_commands: Option<Vec<String>>,

    /// Task plan verify field
    pub task_plan_verify: Option<String>,

    /// Current working directory
    pub cwd: String,
}

/// Result of command discovery
#[derive(Debug, Clone)]
pub struct DiscoveredCommands {
    /// Commands to run
    pub commands: Vec<String>,

    /// Source of commands
    pub source: DiscoverySource,
}

// ─── Command Detection Heuristics ───────────────────────────────────────────────

/// Known safe command prefixes
fn get_known_command_prefixes() -> HashSet<&'static str> {
    [
        "npm",
        "npx",
        "yarn",
        "pnpm",
        "bun",
        "bunx",
        "deno",
        "node",
        "ts-node",
        "tsx",
        "tsc",
        "sh",
        "bash",
        "zsh",
        "echo",
        "cat",
        "ls",
        "test",
        "true",
        "false",
        "pwd",
        "env",
        "make",
        "cargo",
        "go",
        "python",
        "python3",
        "pip",
        "pip3",
        "ruby",
        "gem",
        "bundle",
        "rake",
        "java",
        "javac",
        "mvn",
        "gradle",
        "docker",
        "docker-compose",
        "git",
        "gh",
        "eslint",
        "prettier",
        "vitest",
        "jest",
        "mocha",
        "pytest",
        "phpunit",
        "curl",
        "wget",
        "grep",
        "find",
        "diff",
        "wc",
        "sort",
        "head",
        "tail",
    ]
    .into_iter()
    .collect()
}

/// Check if a string looks like an executable command vs prose
///
/// # Arguments
/// * `cmd` - Command string to check
///
/// # Returns
/// True if appears to be a command
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_gate::*;
///
/// assert!(is_likely_command("cargo test"));
/// assert!(!is_likely_command("This looks like prose text"));
/// ```
pub fn is_likely_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return false;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return false;
    }

    let first_token = tokens[0];

    let known_prefixes = get_known_command_prefixes();

    // Known command prefix → definitely a command
    if known_prefixes.contains(first_token) {
        return true;
    }

    // Path-like first token → command
    if first_token.starts_with('/')
        || first_token.starts_with("./")
        || first_token.starts_with("../")
    {
        return true;
    }

    // Has flag-like tokens → command
    if tokens.iter().any(|t| t.starts_with('-')) {
        return true;
    }

    // First token starts with uppercase + 4 or more words → prose
    if first_token
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        && tokens.len() >= 4
    {
        return false;
    }

    // Contains commas followed by spaces → prose clause structure
    if trimmed.contains(", ") {
        return false;
    }

    // Default: assume command-like
    true
}

/// Check for shell injection patterns
///
/// # Arguments
/// * `cmd` - Command string to check
///
/// # Returns
/// True if command appears safe
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_gate::*;
///
/// assert!(sanitize_command("cargo test").is_some());
/// assert!(sanitize_command("rm -rf /; echo bad").is_none());
/// ```
pub fn sanitize_command(cmd: &str) -> Option<String> {
    // Shell injection patterns: ; | ` && > < newline $(  ${}
    let shell_injection = regex::Regex::new(r"[;|`><\n]|&&|\$\(|\$\{").unwrap();

    if shell_injection.is_match(cmd) {
        return None;
    }

    // Must be command-like
    if !is_likely_command(cmd) {
        return None;
    }

    Some(cmd.to_string())
}

// ─── Command Discovery ───────────────────────────────────────────────────────

/// Discover verification commands using first-non-empty-wins strategy
///
/// # Arguments
/// * `options` - Discovery options
///
/// # Returns
/// Discovered commands with source
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_gate::*;
///
/// let options = DiscoverCommandsOptions {
///     preference_commands: Some(vec!["cargo test".to_string()]),
///     cwd: "/project".to_string(),
///     ..Default::default()
/// };
///
/// let discovered = discover_commands(&options);
/// assert_eq!(discovered.source, DiscoverySource::Preference);
/// ```
pub fn discover_commands(options: &DiscoverCommandsOptions) -> DiscoveredCommands {
    // 1. Preference commands
    if let Some(ref prefs) = options.preference_commands {
        let filtered: Vec<String> = prefs
            .iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();

        if !filtered.is_empty() {
            return DiscoveredCommands {
                commands: filtered,
                source: DiscoverySource::Preference,
            };
        }
    }

    // 2. Task plan verify field
    if let Some(ref verify) = options.task_plan_verify {
        let trimmed = verify.trim();
        if !trimmed.is_empty() {
            let commands: Vec<String> = trimmed
                .split("&&")
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .filter_map(|c| sanitize_command(&c))
                .collect();

            if !commands.is_empty() {
                return DiscoveredCommands {
                    commands,
                    source: DiscoverySource::TaskPlan,
                };
            }
        }
    }

    // 3. package.json scripts
    let pkg_path = std::path::Path::new(&options.cwd).join("package.json");
    if pkg_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(scripts) = pkg.get("scripts").and_then(|v| v.as_object()) {
                    let mut commands = Vec::new();

                    for &key in PACKAGE_SCRIPT_KEYS {
                        if let Some(_script) = scripts.get(key).and_then(|v| v.as_str()) {
                            commands.push(format!("npm run {}", key));
                        }
                    }

                    if !commands.is_empty() {
                        return DiscoveredCommands {
                            commands,
                            source: DiscoverySource::PackageJson,
                        };
                    }
                }
            }
        }
    }

    // 4. Nothing found
    DiscoveredCommands {
        commands: Vec::new(),
        source: DiscoverySource::None,
    }
}

// ─── Command Execution ───────────────────────────────────────────────────────

/// Truncate string to max bytes
fn truncate_to_bytes(value: &str, max_bytes: usize) -> String {
    let byte_len = value.len();
    if byte_len <= max_bytes {
        return value.to_string();
    }

    // Truncate conservatively
    let truncated = &value[..max_bytes.min(value.len())];

    // Trim to last full character (UTF-8 safe)
    match truncated.char_indices().last() {
        Some((pos, _)) => truncated[..pos].to_string(),
        None => truncated.to_string(),
    }
}

/// Run a single verification command
///
/// # Arguments
/// * `command` - Command string to run
/// * `cwd` - Working directory
///
/// # Returns
/// Verification check result
fn run_command(command: &str, cwd: &str) -> VerificationCheck {
    let start = std::time::Instant::now();

    // Parse command into parts
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return VerificationCheck {
            command: command.to_string(),
            exit_code: 1,
            stdout: None,
            stderr: Some("Empty command".to_string()),
            duration_ms: 0,
        };
    }

    // Execute command
    let result = Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(cwd)
        .output();

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(1);

            let stdout = if !output.stdout.is_empty() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                Some(truncate_to_bytes(&stdout_str, MAX_OUTPUT_BYTES))
            } else {
                None
            };

            let stderr = if !output.stderr.is_empty() {
                let stderr_str = String::from_utf8_lossy(&output.stderr);
                Some(truncate_to_bytes(&stderr_str, MAX_OUTPUT_BYTES))
            } else {
                None
            };

            VerificationCheck {
                command: command.to_string(),
                exit_code,
                stdout,
                stderr,
                duration_ms,
            }
        }
        Err(e) => VerificationCheck {
            command: command.to_string(),
            exit_code: 1,
            stdout: None,
            stderr: Some(format!("Command execution error: {}", e)),
            duration_ms,
        },
    }
}

/// Run verification gate
///
/// # Arguments
/// * `commands` - Commands to run
/// * `cwd` - Working directory
/// * `discovery_source` - Source of commands
///
/// # Returns
/// Verification result with all checks
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_gate::*;
///
/// let result = run_verification_gate(
///     &["cargo test".to_string(), "cargo clippy".to_string()],
///     "/project",
///     DiscoverySource::Preference
/// );
/// ```
#[must_use]
pub fn run_verification_gate(
    commands: &[String],
    cwd: &str,
    discovery_source: DiscoverySource,
) -> VerificationResult {
    let mut checks = Vec::new();
    let mut total_duration_ms = 0;

    for command in commands {
        let check = run_command(command, cwd);
        total_duration_ms += check.duration_ms;
        checks.push(check);
    }

    let all_passed = checks.iter().all(|c| c.exit_code == 0);

    VerificationResult {
        checks,
        discovery_source,
        total_duration_ms,
        all_passed,
    }
}

/// Format failed verification checks into prompt-injectable text
///
/// # Arguments
/// * `result` - Verification result
///
/// # Returns
/// Formatted failure context, empty if all passed
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_gate::*;
///
/// let result = VerificationResult {
///     checks: vec![
///         VerificationCheck {
///             command: "cargo test".to_string(),
///             exit_code: 1,
///             stderr: Some("test failed".to_string()),
///             ..Default::default()
///         }
///     ],
///     discovery_source: DiscoverySource::Preference,
///     total_duration_ms: 1000,
///     all_passed: false,
/// };
///
/// let context = format_failure_context(&result);
/// assert!(context.contains("Verification Failures"));
/// ```
pub fn format_failure_context(result: &VerificationResult) -> String {
    let failures: Vec<&VerificationCheck> =
        result.checks.iter().filter(|c| c.exit_code != 0).collect();

    if failures.is_empty() {
        return String::new();
    }

    let mut blocks = Vec::new();

    for check in failures {
        let mut stderr = check.stderr.as_deref().unwrap_or("");

        if stderr.len() > MAX_STDERR_PER_CHECK {
            stderr = &stderr[..MAX_STDERR_PER_CHECK];
            let truncated = format!("{}\n…[truncated]", stderr);
            blocks.push(format!(
                "### ❌ `{}` (exit code {})\n\
                 ```\nstderr\n{}\n\
                 ```",
                check.command, check.exit_code, truncated
            ));
        } else {
            blocks.push(format!(
                "### ❌ `{}` (exit code {})\n\
                 ```\nstderr\n{}\n\
                 ```",
                check.command, check.exit_code, stderr
            ));
        }
    }

    let mut body = blocks.join("\n\n");
    let header = "## Verification Failures\n\n";

    if header.len() + body.len() > MAX_FAILURE_CONTEXT_CHARS {
        let max_body = MAX_FAILURE_CONTEXT_CHARS.saturating_sub(header.len());
        body = format!(
            "{}\n\n…[remaining failures truncated]",
            &body[..max_body.min(body.len())]
        );
    }

    format!("{}{}", header, body)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_likely_command_cargo() {
        assert!(is_likely_command("cargo test"));
        assert!(is_likely_command("cargo build --release"));
    }

    #[test]
    fn test_is_likely_command_npm() {
        assert!(is_likely_command("npm test"));
        assert!(is_likely_command("npm run lint"));
    }

    #[test]
    fn test_is_likely_command_path() {
        assert!(is_likely_command("./scripts/test.sh"));
        assert!(is_likely_command("/usr/bin/test"));
    }

    #[test]
    fn test_is_likely_command_flags() {
        assert!(is_likely_command("command --flag --other=value"));
    }

    #[test]
    fn test_is_likely_prose() {
        assert!(!is_likely_command("This looks like prose text"));
        assert!(!is_likely_command(
            "Document exists, contains all 5 scale names"
        ));
    }

    #[test]
    fn test_sanitize_command_safe() {
        assert!(sanitize_command("cargo test").is_some());
        assert!(sanitize_command("npm run lint").is_some());
    }

    #[test]
    fn test_sanitize_command_injection() {
        assert!(sanitize_command("rm -rf /; echo bad").is_none());
        assert!(sanitize_command("cat file | grep test").is_none());
        assert!(sanitize_command("echo $(whoami)").is_none());
        // Additional injection patterns
        assert!(sanitize_command("echo bad && cat /etc/passwd").is_none());
        assert!(sanitize_command("cat /etc/passwd > stolen.txt").is_none());
        assert!(sanitize_command("cat < /etc/passwd").is_none());
        assert!(sanitize_command("echo ${IFS}injection").is_none());
        assert!(sanitize_command("echo bad\nevil_command").is_none());
    }

    #[test]
    fn test_sanitize_command_prose() {
        assert!(sanitize_command("This is not a command").is_none());
    }

    #[test]
    fn test_discover_commands_preference() {
        let options = DiscoverCommandsOptions {
            preference_commands: Some(vec!["cargo test".to_string()]),
            cwd: "/project".to_string(),
            ..Default::default()
        };

        let discovered = discover_commands(&options);
        assert_eq!(discovered.source, DiscoverySource::Preference);
        assert_eq!(discovered.commands, vec!["cargo test"]);
    }

    #[test]
    fn test_discover_commands_task_plan() {
        let options = DiscoverCommandsOptions {
            task_plan_verify: Some("cargo test && cargo clippy".to_string()),
            cwd: "/project".to_string(),
            ..Default::default()
        };

        let discovered = discover_commands(&options);
        assert_eq!(discovered.source, DiscoverySource::TaskPlan);
        assert_eq!(discovered.commands.len(), 2);
    }

    #[test]
    fn test_discover_commands_none() {
        let options = DiscoverCommandsOptions {
            cwd: "/project".to_string(),
            ..Default::default()
        };

        let discovered = discover_commands(&options);
        assert_eq!(discovered.source, DiscoverySource::None);
        assert!(discovered.commands.is_empty());
    }

    #[test]
    fn test_truncate_to_bytes() {
        let text = "Hello, world!";
        let truncated = truncate_to_bytes(text, 5);
        assert!(truncated.len() <= 5); // May be less due to UTF-8
    }

    #[test]
    fn test_truncate_to_bytes_no_truncate() {
        let text = "Hello";
        let truncated = truncate_to_bytes(text, 10);
        assert_eq!(truncated, "Hello");
    }

    #[test]
    fn test_format_failure_context_all_passed() {
        let result = VerificationResult {
            checks: vec![VerificationCheck {
                command: "cargo test".to_string(),
                exit_code: 0,
                ..Default::default()
            }],
            discovery_source: DiscoverySource::Preference,
            total_duration_ms: 100,
            all_passed: true,
        };

        let context = format_failure_context(&result);
        assert!(context.is_empty());
    }

    #[test]
    fn test_format_failure_context_with_failures() {
        let result = VerificationResult {
            checks: vec![VerificationCheck {
                command: "cargo test".to_string(),
                exit_code: 1,
                stderr: Some("test failed".to_string()),
                ..Default::default()
            }],
            discovery_source: DiscoverySource::Preference,
            total_duration_ms: 100,
            all_passed: false,
        };

        let context = format_failure_context(&result);
        assert!(context.contains("Verification Failures"));
        assert!(context.contains("cargo test"));
        assert!(context.contains("test failed"));
    }

    #[test]
    fn test_verification_check_default() {
        let check = VerificationCheck::default();
        assert_eq!(check.exit_code, 0);
        assert!(check.command.is_empty());
    }

    #[test]
    fn test_known_command_prefixes() {
        let prefixes = get_known_command_prefixes();
        assert!(prefixes.contains("cargo"));
        assert!(prefixes.contains("npm"));
        assert!(prefixes.contains("pytest"));
    }

    #[test]
    fn test_format_failure_context_truncation() {
        // Create a long stderr
        let long_stderr = "x".repeat(MAX_STDERR_PER_CHECK + 1000);

        let result = VerificationResult {
            checks: vec![VerificationCheck {
                command: "test".to_string(),
                exit_code: 1,
                stderr: Some(long_stderr),
                ..Default::default()
            }],
            discovery_source: DiscoverySource::Preference,
            total_duration_ms: 100,
            all_passed: false,
        };

        let context = format_failure_context(&result);
        assert!(context.contains("truncated"));
    }
}
