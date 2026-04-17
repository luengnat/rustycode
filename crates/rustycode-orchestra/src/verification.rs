//! Verification Gates
//!
//! Run tests and checks after each unit to ensure it actually worked.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use tracing::{info, warn};

/// Verification result
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub passed: bool,
    pub failures: Vec<VerificationFailure>,
}

/// Verification failure details
#[derive(Debug, Clone)]
pub struct VerificationFailure {
    pub check: String,
    pub message: String,
    pub context: String,
}

/// Verification gate configuration
#[derive(Debug, Clone)]
pub struct VerificationConfig {
    /// Run tests after unit completion
    pub run_tests: bool,
    /// Check required files exist
    pub check_files: bool,
    /// Validate build succeeds
    pub check_build: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            run_tests: true,
            check_files: true,
            check_build: true,
        }
    }
}

/// Verification gate executor
pub struct VerificationGate {
    project_root: PathBuf,
    config: VerificationConfig,
}

impl VerificationGate {
    pub fn new(project_root: PathBuf, config: VerificationConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }

    /// Run all verification checks
    pub fn verify(
        &self,
        unit_id: &str,
        milestone_id: Option<&str>,
        slice_id: Option<&str>,
    ) -> Result<VerificationResult> {
        info!("🔍 Running verification gate for unit: {}", unit_id);

        let mut failures = Vec::new();

        // Check build
        if self.config.check_build {
            if let Err(e) = self.check_build() {
                failures.push(VerificationFailure {
                    check: "build".to_string(),
                    message: "Build failed".to_string(),
                    context: e.to_string(),
                });
            }
        }

        // Run tests
        if self.config.run_tests {
            if let Err(e) = self.run_tests() {
                failures.push(VerificationFailure {
                    check: "tests".to_string(),
                    message: "Tests failed".to_string(),
                    context: e.to_string(),
                });
            }
        }

        // Check required files
        if self.config.check_files {
            if let Err(e) = self.check_required_files(unit_id, milestone_id, slice_id) {
                failures.push(VerificationFailure {
                    check: "files".to_string(),
                    message: "Required files missing".to_string(),
                    context: e.to_string(),
                });
            }
        }

        let passed = failures.is_empty();
        if passed {
            info!("✅ Verification passed for unit: {}", unit_id);
        } else {
            warn!(
                "❌ Verification failed for unit: {} ({} failures)",
                unit_id,
                failures.len()
            );
        }

        Ok(VerificationResult { passed, failures })
    }

    /// Check if build succeeds
    fn check_build(&self) -> Result<()> {
        // Detect project type and run appropriate build command
        if self.project_root.join("Cargo.toml").exists() {
            // Rust project
            let output = Command::new("cargo")
                .args(["build", "--quiet"])
                .current_dir(&self.project_root)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Cargo build failed: {}", stderr));
            }
        } else if self.project_root.join("package.json").exists() {
            // Node.js project
            let output = Command::new("npm")
                .args(["run", "build"])
                .current_dir(&self.project_root)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("npm build failed: {}", stderr));
            }
        }

        Ok(())
    }

    /// Run tests
    fn run_tests(&self) -> Result<()> {
        if self.project_root.join("Cargo.toml").exists() {
            // Rust project
            let output = Command::new("cargo")
                .args(["test", "--quiet"])
                .current_dir(&self.project_root)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Cargo tests failed: {}", stderr));
            }
        } else if self.project_root.join("package.json").exists() {
            // Node.js project
            let output = Command::new("npm")
                .args(["test"])
                .current_dir(&self.project_root)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("npm tests failed: {}", stderr));
            }
        }

        Ok(())
    }

    /// Check required files exist
    fn check_required_files(
        &self,
        unit_id: &str,
        milestone_id: Option<&str>,
        slice_id: Option<&str>,
    ) -> Result<()> {
        // For tasks, check if summary was written
        let milestone = milestone_id.unwrap_or("M01");
        let slice = slice_id.unwrap_or("S01");

        let summary_path = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone)
            .join("slices")
            .join(slice)
            .join("tasks")
            .join(format!("{}-SUMMARY.md", unit_id));

        if !summary_path.exists() {
            return Err(anyhow::anyhow!(
                "Summary file not found: {:?}",
                summary_path
            ));
        }

        Ok(())
    }

    /// Format failure context for retry prompt
    pub fn format_failure_context(failures: &[VerificationFailure]) -> String {
        let mut context = String::from("## Verification Failures\n\n");

        for failure in failures {
            context.push_str(&format!(
                "### {}: {}\n```\n{}\n```\n\n",
                failure.check, failure.message, failure.context
            ));
        }

        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- VerificationConfig ---

    #[test]
    fn config_default_all_true() {
        let cfg = VerificationConfig::default();
        assert!(cfg.run_tests);
        assert!(cfg.check_files);
        assert!(cfg.check_build);
    }

    #[test]
    fn config_custom_all_false() {
        let cfg = VerificationConfig {
            run_tests: false,
            check_files: false,
            check_build: false,
        };
        assert!(!cfg.run_tests);
        assert!(!cfg.check_files);
        assert!(!cfg.check_build);
    }

    // --- VerificationResult ---

    #[test]
    fn result_passed_no_failures() {
        let r = VerificationResult {
            passed: true,
            failures: vec![],
        };
        assert!(r.passed);
        assert!(r.failures.is_empty());
    }

    #[test]
    fn result_failed_with_failures() {
        let r = VerificationResult {
            passed: false,
            failures: vec![VerificationFailure {
                check: "build".into(),
                message: "Build failed".into(),
                context: "error".into(),
            }],
        };
        assert!(!r.passed);
        assert_eq!(r.failures.len(), 1);
    }

    // --- VerificationFailure ---

    #[test]
    fn failure_fields() {
        let f = VerificationFailure {
            check: "tests".into(),
            message: "Tests failed".into(),
            context: "1 failed".into(),
        };
        assert_eq!(f.check, "tests");
        assert_eq!(f.message, "Tests failed");
        assert_eq!(f.context, "1 failed");
    }

    // --- format_failure_context ---

    #[test]
    fn format_empty_failures() {
        let ctx = VerificationGate::format_failure_context(&[]);
        assert!(ctx.contains("Verification Failures"));
    }

    #[test]
    fn format_single_failure() {
        let failures = vec![VerificationFailure {
            check: "build".into(),
            message: "Build failed".into(),
            context: "cargo error".into(),
        }];
        let ctx = VerificationGate::format_failure_context(&failures);
        assert!(ctx.contains("build"));
        assert!(ctx.contains("Build failed"));
        assert!(ctx.contains("cargo error"));
    }

    #[test]
    fn format_multiple_failures() {
        let failures = vec![
            VerificationFailure {
                check: "build".into(),
                message: "Build failed".into(),
                context: "err1".into(),
            },
            VerificationFailure {
                check: "tests".into(),
                message: "Tests failed".into(),
                context: "err2".into(),
            },
        ];
        let ctx = VerificationGate::format_failure_context(&failures);
        assert!(ctx.contains("build"));
        assert!(ctx.contains("tests"));
    }

    // --- VerificationGate ---

    #[test]
    fn gate_new() {
        let gate = VerificationGate::new(PathBuf::from("/tmp"), VerificationConfig::default());
        assert_eq!(gate.project_root, PathBuf::from("/tmp"));
        assert!(gate.config.run_tests);
    }

    #[test]
    fn verify_with_all_checks_disabled() {
        let gate = VerificationGate::new(
            PathBuf::from("/nonexistent"),
            VerificationConfig {
                run_tests: false,
                check_files: false,
                check_build: false,
            },
        );
        let result = gate.verify("unit-1", None, None).unwrap();
        assert!(result.passed);
        assert!(result.failures.is_empty());
    }
}
