// rustycode-orchestra/src/orchestra_tools.rs
//! Orchestra Tool Integration
//!
//! Integrates rustycode-tools with Orchestra methodology for verification,
//! state tracking, and workflow operations.

use std::path::Path;
use std::process::Command;

/// Orchestra tool execution result
#[derive(Debug, Clone, PartialEq)]
pub struct OrchestraToolResult {
    pub passed: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Orchestra tool execution configuration
#[derive(Debug, Clone)]
pub struct ToolConfig {
    pub command: String,
    pub args: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub working_dir: Option<String>,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            timeout_ms: Some(30_000),
            working_dir: None,
        }
    }
}

/// Execute a verification command with timeout
pub fn run_verification(config: &ToolConfig) -> OrchestraToolResult {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args);

    if let Some(dir) = &config.working_dir {
        cmd.current_dir(dir);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            OrchestraToolResult {
                passed: exit_code == 0,
                exit_code,
                stdout,
                stderr,
            }
        }
        Err(e) => OrchestraToolResult {
            passed: false,
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("Failed to execute command: {}", e),
        },
    }
}

/// Detect verification commands from package manager
pub fn detect_verification_commands(project_dir: &Path) -> Vec<String> {
    let mut commands = Vec::new();

    // Check for Node.js projects
    if project_dir.join("package.json").exists() {
        if has_npm_script(project_dir, "test") {
            commands.push("npm test".to_string());
        }
        if has_npm_script(project_dir, "lint") {
            commands.push("npm run lint".to_string());
        }
        if has_npm_script(project_dir, "build") {
            commands.push("npm run build".to_string());
        }
    }

    // Check for Rust projects
    if project_dir.join("Cargo.toml").exists() {
        commands.push("cargo test".to_string());
        commands.push("cargo clippy".to_string());
        commands.push("cargo build --release".to_string());
    }

    // Check for Python projects
    if project_dir.join("pyproject.toml").exists()
        || project_dir.join("setup.py").exists()
        || project_dir.join("requirements.txt").exists()
    {
        commands.push("pytest".to_string());
        if project_dir.join("pyproject.toml").exists() {
            commands.push("ruff check".to_string());
        }
    }

    commands
}

/// Check if package.json has a specific script
fn has_npm_script(project_dir: &Path, script_name: &str) -> bool {
    let package_json = project_dir.join("package.json");
    if !package_json.exists() {
        return false;
    }

    match std::fs::read_to_string(package_json) {
        Ok(content) => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
                    return scripts.contains_key(script_name);
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Format verification result for display
pub fn format_verification_result(result: &OrchestraToolResult) -> String {
    if result.passed {
        format!("✓ PASSED (exit code: {})", result.exit_code)
    } else {
        format!(
            "✗ FAILED (exit code: {})\nstderr: {}",
            result.exit_code,
            result.stderr.lines().take(5).collect::<Vec<_>>().join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_config_default() {
        let config = ToolConfig::default();
        assert_eq!(config.command, "");
        assert!(config.args.is_empty());
        assert_eq!(config.timeout_ms, Some(30_000));
        assert!(config.working_dir.is_none());
    }

    #[test]
    fn test_verification_result_passed() {
        let result = OrchestraToolResult {
            passed: true,
            exit_code: 0,
            stdout: "All tests passed".to_string(),
            stderr: String::new(),
        };
        assert!(result.passed);
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_verification_result_failed() {
        let result = OrchestraToolResult {
            passed: false,
            exit_code: 1,
            stdout: String::new(),
            stderr: "Test failed".to_string(),
        };
        assert!(!result.passed);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_format_verification_result_passed() {
        let result = OrchestraToolResult {
            passed: true,
            exit_code: 0,
            stdout: "OK".to_string(),
            stderr: String::new(),
        };
        let formatted = format_verification_result(&result);
        assert!(formatted.contains("PASSED"));
        assert!(formatted.contains("exit code: 0"));
    }

    #[test]
    fn test_format_verification_result_failed() {
        let result = OrchestraToolResult {
            passed: false,
            exit_code: 1,
            stdout: String::new(),
            stderr: "Error: something failed".to_string(),
        };
        let formatted = format_verification_result(&result);
        assert!(formatted.contains("FAILED"));
        assert!(formatted.contains("stderr:"));
    }
}
