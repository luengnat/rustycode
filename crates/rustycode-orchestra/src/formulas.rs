// rustycode-orchestra/src/formulas.rs
//! Reusable workflow step templates inspired by Gastown's "Formula" concept.
//!
//! Formulas define reusable, ordered workflow steps inspired by the Gastown "Formula" concept.
//! Each formula has a name, description, and an ordered list of steps with optional
//! skip conditions and verification commands.

use crate::error::{OrchestraV2Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shell_words;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A single step within a formula workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaStep {
    /// The instruction or action prompt for this step.
    pub prompt: String,
    /// Optional condition — if this evaluates to true, skip this step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_condition: Option<String>,
    /// Optional command to run to verify this step completed successfully.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_command: Option<String>,
}

/// A reusable workflow formula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Formula {
    /// Human-readable formula name.
    pub name: String,
    /// Description of what this formula does.
    pub description: String,
    /// Ordered list of steps to execute.
    pub steps: Vec<FormulaStep>,
    /// When this formula was created.
    pub created_at: DateTime<Utc>,
    /// When this formula was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Formula {
    /// Create a new formula with the given name, description, and steps.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        steps: Vec<FormulaStep>,
    ) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            description: description.into(),
            steps,
            created_at: now,
            updated_at: now,
        }
    }

    /// Generate a filesystem-safe slug from the formula name.
    pub fn slug(&self) -> String {
        slugify_name(&self.name)
    }

    /// Number of steps in this formula.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Format a one-line summary.
    pub fn format_summary(&self) -> String {
        format!(
            "📋 {} — {} step{} | {}",
            self.name,
            self.steps.len(),
            if self.steps.len() == 1 { "" } else { "s" },
            self.description,
        )
    }
}

/// Validation result for a formula.
#[derive(Debug, Clone)]
pub struct FormulaValidation {
    /// Whether the formula passed validation.
    pub valid: bool,
    /// Validation errors (causes valid=false).
    pub errors: Vec<String>,
    /// Validation warnings (non-blocking).
    pub warnings: Vec<String>,
}

/// Context for formula step execution.
#[derive(Debug, Clone)]
pub struct FormulaExecutionContext {
    /// Whether to evaluate skip conditions.
    pub skip_checks_enabled: bool,
    /// Working directory for command execution.
    pub working_directory: PathBuf,
    /// Template variables for step prompts (`{{key}}` -> value).
    pub variables: HashMap<String, String>,
}

impl Default for FormulaExecutionContext {
    fn default() -> Self {
        Self {
            skip_checks_enabled: true,
            working_directory: PathBuf::from("."),
            variables: HashMap::new(),
        }
    }
}

/// Result of executing a single formula step.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Index of the step (0-based).
    pub step_index: usize,
    /// The resolved prompt text.
    pub prompt: String,
    /// Whether this step was skipped.
    pub skipped: bool,
    /// Verification result (`None` if no verification command).
    pub verified: Option<bool>,
    /// Output description.
    pub output: String,
}

impl std::fmt::Display for StepResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.skipped {
            write!(
                f,
                "⏭️ Step {}: SKIPPED — {}",
                self.step_index + 1,
                self.prompt,
            )
        } else {
            let verify_icon = match self.verified {
                Some(true) => " ✅",
                Some(false) => " ❌",
                None => "",
            };
            write!(
                f,
                "▶️ Step {}:{} — {}",
                self.step_index + 1,
                verify_icon,
                self.prompt,
            )
        }
    }
}

/// Manages formula YAML files on disk.
pub struct FormulaManager {
    formulas_dir: PathBuf,
}

impl FormulaManager {
    /// Create a new formula manager, ensuring the formulas directory exists.
    pub fn new(project_root: &Path) -> Result<Self> {
        let formulas_dir = project_root.join(".orchestra").join("formulas");
        if !formulas_dir.exists() {
            fs::create_dir_all(&formulas_dir).map_err(OrchestraV2Error::Io)?;
        }
        Ok(Self { formulas_dir })
    }

    /// Create, validate, and save a new formula.
    pub fn create(
        &self,
        name: &str,
        description: &str,
        steps: Vec<FormulaStep>,
    ) -> Result<Formula> {
        let formula = Formula::new(name, description, steps);
        let validation = self.validate(&formula)?;
        if !validation.valid {
            return Err(OrchestraV2Error::InvalidState(format!(
                "Formula validation failed: {}",
                validation.errors.join("; ")
            )));
        }
        self.save(&formula)?;
        Ok(formula)
    }

    /// Load a formula by name or slug.
    pub fn load(&self, name: &str) -> Result<Formula> {
        let slug = slugify_name(name);
        let path = self.formulas_dir.join(format!("{}.yaml", slug));
        let content = fs::read_to_string(&path).map_err(OrchestraV2Error::Io)?;
        serde_yaml::from_str(&content).map_err(|e| {
            OrchestraV2Error::Parse(format!("Invalid formula YAML for '{}': {}", name, e))
        })
    }

    /// Save a formula to disk as YAML.
    pub fn save(&self, formula: &Formula) -> Result<()> {
        let slug = formula.slug();
        let path = self.formulas_dir.join(format!("{}.yaml", slug));
        let content = serde_yaml::to_string(formula).map_err(|e| {
            OrchestraV2Error::Serialization(format!("Failed to serialize formula: {}", e))
        })?;
        fs::write(&path, content).map_err(OrchestraV2Error::Io)
    }

    /// List all formulas, sorted newest first.
    pub fn list(&self) -> Result<Vec<Formula>> {
        let mut formulas = Vec::new();
        if !self.formulas_dir.exists() {
            return Ok(formulas);
        }

        for entry in fs::read_dir(&self.formulas_dir).map_err(OrchestraV2Error::Io)? {
            let entry = entry.map_err(OrchestraV2Error::Io)?;
            if entry.path().extension().is_some_and(|ext| ext == "yaml") {
                let content = fs::read_to_string(entry.path()).map_err(OrchestraV2Error::Io)?;
                if let Ok(formula) = serde_yaml::from_str::<Formula>(&content) {
                    formulas.push(formula);
                }
            }
        }

        formulas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(formulas)
    }

    /// Get a formula by exact name match.
    pub fn get_by_name(&self, name: &str) -> Result<Option<Formula>> {
        let formulas = self.list()?;
        Ok(formulas.into_iter().find(|f| f.name == name))
    }

    /// Validate a formula's structure without saving.
    pub fn validate(&self, formula: &Formula) -> Result<FormulaValidation> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if formula.name.trim().is_empty() {
            errors.push("Formula name must not be empty".to_string());
        }

        if formula.description.trim().is_empty() {
            warnings.push("Formula description is empty".to_string());
        }

        if formula.steps.is_empty() {
            errors.push("Formula must have at least one step".to_string());
        }

        for (i, step) in formula.steps.iter().enumerate() {
            if step.prompt.trim().is_empty() {
                errors.push(format!("Step {}: prompt must not be empty", i + 1));
            }
        }

        Ok(FormulaValidation {
            valid: errors.is_empty(),
            errors,
            warnings,
        })
    }

    /// Delete a formula by name.
    pub fn delete(&self, name: &str) -> Result<()> {
        let slug = slugify_name(name);
        let path = self.formulas_dir.join(format!("{}.yaml", slug));
        if path.exists() {
            fs::remove_file(path).map_err(OrchestraV2Error::Io)?;
        }
        Ok(())
    }

    /// Execute a single step of a formula.
    pub fn execute_step(
        &self,
        formula: &Formula,
        step_index: usize,
        context: &FormulaExecutionContext,
    ) -> Result<StepResult> {
        if step_index >= formula.steps.len() {
            return Err(OrchestraV2Error::InvalidState(format!(
                "Step index {} out of range (formula '{}' has {} steps)",
                step_index,
                formula.name,
                formula.steps.len()
            )));
        }

        let step = &formula.steps[step_index];
        let resolved_prompt = resolve_template_variables(&step.prompt, &context.variables);
        let skipped = if context.skip_checks_enabled {
            step.skip_condition
                .as_ref()
                .map(|c| evaluate_skip_condition(c))
                .unwrap_or(false)
        } else {
            false
        };

        if skipped {
            return Ok(StepResult {
                step_index,
                prompt: resolved_prompt,
                skipped: true,
                verified: None,
                output: "Skipped due to condition".to_string(),
            });
        }

        let verified = step.verification_command.as_ref().map(|cmd| {
            let resolved = resolve_template_variables(cmd, &context.variables);
            run_verification_command(&resolved, &context.working_directory)
        });

        let prompt_display = resolved_prompt.clone();
        Ok(StepResult {
            step_index,
            prompt: resolved_prompt,
            skipped: false,
            verified,
            output: format!("Executed step {}: {}", step_index + 1, prompt_display),
        })
    }

    /// Execute all steps of a formula sequentially.
    pub fn execute_formula(
        &self,
        formula: &Formula,
        context: &FormulaExecutionContext,
    ) -> Result<Vec<StepResult>> {
        let mut results = Vec::with_capacity(formula.steps.len());
        for i in 0..formula.steps.len() {
            let result = self.execute_step(formula, i, context)?;
            results.push(result);
        }
        Ok(results)
    }
}

/// Convert a formula name to a filesystem-safe slug.
fn slugify_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    slug.split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Resolve `{{variable}}` placeholders in a template string.
fn resolve_template_variables(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Evaluate a skip condition using simple truthy matching.
fn evaluate_skip_condition(condition: &str) -> bool {
    let trimmed = condition.trim().to_lowercase();
    matches!(trimmed.as_str(), "true" | "yes" | "1" | "pass")
}

/// Run a verification command and return whether it succeeded.
fn run_verification_command(command: &str, working_directory: &Path) -> bool {
    // SECURE: Parse command using shell_words to prevent shell injection
    let parts = match shell_words::split(command) {
        Ok(p) => p,
        Err(_) => return false,
    };

    if parts.is_empty() {
        return false;
    }

    let binary = &parts[0];
    let args: Vec<&String> = parts.iter().skip(1).collect();

    std::process::Command::new(binary)
        .args(args)
        .current_dir(working_directory)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula_new() {
        let steps = vec![
            FormulaStep {
                prompt: "Run tests".to_string(),
                skip_condition: None,
                verification_command: Some("cargo test".to_string()),
            },
            FormulaStep {
                prompt: "Fix failures".to_string(),
                skip_condition: Some("tests pass".to_string()),
                verification_command: None,
            },
        ];
        let formula = Formula::new("Quick Fix", "Fix issues quickly", steps);
        assert_eq!(formula.name, "Quick Fix");
        assert_eq!(formula.description, "Fix issues quickly");
        assert_eq!(formula.steps.len(), 2);
        assert_eq!(formula.slug(), "quick-fix");
    }

    #[test]
    fn test_formula_slug() {
        let formula = Formula::new(
            "My Cool Formula!",
            "test",
            vec![FormulaStep {
                prompt: "step".to_string(),
                skip_condition: None,
                verification_command: None,
            }],
        );
        assert_eq!(formula.slug(), "my-cool-formula");
    }

    #[test]
    fn test_slugify_name() {
        assert_eq!(slugify_name("Hello World"), "hello-world");
        assert_eq!(slugify_name("  My   Cool  Formula!  "), "my-cool-formula");
        assert_eq!(slugify_name("test123"), "test123");
        assert_eq!(slugify_name("A & B + C"), "a-b-c");
    }

    #[test]
    fn test_formula_step_count() {
        let formula = Formula::new(
            "test",
            "test",
            vec![
                FormulaStep {
                    prompt: "a".into(),
                    skip_condition: None,
                    verification_command: None,
                },
                FormulaStep {
                    prompt: "b".into(),
                    skip_condition: None,
                    verification_command: None,
                },
                FormulaStep {
                    prompt: "c".into(),
                    skip_condition: None,
                    verification_command: None,
                },
            ],
        );
        assert_eq!(formula.step_count(), 3);
    }

    #[test]
    fn test_formula_format_summary() {
        let formula = Formula::new(
            "Deploy",
            "Deploy to production",
            vec![
                FormulaStep {
                    prompt: "step".into(),
                    skip_condition: None,
                    verification_command: None,
                },
                FormulaStep {
                    prompt: "step".into(),
                    skip_condition: None,
                    verification_command: None,
                },
            ],
        );
        let summary = formula.format_summary();
        assert!(summary.contains("Deploy"));
        assert!(summary.contains("2 steps"));
    }

    #[test]
    fn test_manager_create_and_load() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Bug Fix Workflow",
                "Standard bug fixing workflow",
                vec![
                    FormulaStep {
                        prompt: "Read error output".to_string(),
                        skip_condition: None,
                        verification_command: Some("echo ok".to_string()),
                    },
                    FormulaStep {
                        prompt: "Identify root cause".to_string(),
                        skip_condition: Some("tests pass".to_string()),
                        verification_command: None,
                    },
                ],
            )
            .unwrap();

        assert_eq!(formula.name, "Bug Fix Workflow");
        assert_eq!(formula.steps.len(), 2);

        // Verify file was written
        let file_path = temp
            .path()
            .join(".orchestra")
            .join("formulas")
            .join("bug-fix-workflow.yaml");
        assert!(file_path.exists());

        // Load by name
        let loaded = manager.load("Bug Fix Workflow").unwrap();
        assert_eq!(loaded.name, "Bug Fix Workflow");
        assert_eq!(loaded.steps.len(), 2);

        // Load by slug
        let by_slug = manager.load("bug-fix-workflow").unwrap();
        assert_eq!(by_slug.name, "Bug Fix Workflow");
    }

    #[test]
    fn test_manager_list() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();
        assert!(manager.list().unwrap().is_empty());

        manager
            .create(
                "Formula A",
                "First",
                vec![FormulaStep {
                    prompt: "step".into(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        manager
            .create(
                "Formula B",
                "Second",
                vec![FormulaStep {
                    prompt: "step".into(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        let list = manager.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_manager_get_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        manager
            .create(
                "Exact Match",
                "desc",
                vec![FormulaStep {
                    prompt: "step".into(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        let found = manager.get_by_name("Exact Match").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Exact Match");

        let not_found = manager.get_by_name("Does Not Exist").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_manager_delete() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        manager
            .create(
                "Delete Me",
                "desc",
                vec![FormulaStep {
                    prompt: "step".into(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        assert_eq!(manager.list().unwrap().len(), 1);
        manager.delete("Delete Me").unwrap();
        assert!(manager.list().unwrap().is_empty());
    }

    #[test]
    fn test_validate_valid() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = Formula::new(
            "Valid",
            "desc",
            vec![FormulaStep {
                prompt: "step".into(),
                skip_condition: None,
                verification_command: None,
            }],
        );
        let v = manager.validate(&formula).unwrap();
        assert!(v.valid);
        assert!(v.errors.is_empty());
    }

    #[test]
    fn test_validate_empty_name() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = Formula::new(
            "",
            "desc",
            vec![FormulaStep {
                prompt: "step".into(),
                skip_condition: None,
                verification_command: None,
            }],
        );
        let v = manager.validate(&formula).unwrap();
        assert!(!v.valid);
        assert!(v
            .errors
            .iter()
            .any(|e| e.contains("name must not be empty")));
    }

    #[test]
    fn test_validate_no_steps() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = Formula::new("Test", "desc", vec![]);
        let v = manager.validate(&formula).unwrap();
        assert!(!v.valid);
        assert!(v.errors.iter().any(|e| e.contains("at least one step")));
    }

    #[test]
    fn test_validate_empty_step_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = Formula::new(
            "Test",
            "desc",
            vec![FormulaStep {
                prompt: "  ".to_string(),
                skip_condition: None,
                verification_command: None,
            }],
        );
        let v = manager.validate(&formula).unwrap();
        assert!(!v.valid);
        assert!(v
            .errors
            .iter()
            .any(|e| e.contains("prompt must not be empty")));
    }

    #[test]
    fn test_validate_warning_empty_description() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = Formula::new(
            "Test",
            "",
            vec![FormulaStep {
                prompt: "step".into(),
                skip_condition: None,
                verification_command: None,
            }],
        );
        let v = manager.validate(&formula).unwrap();
        assert!(v.valid);
        assert!(v
            .warnings
            .iter()
            .any(|w| w.contains("description is empty")));
    }

    #[test]
    fn test_create_invalid_fails() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();
        let result = manager.create("", "desc", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_step_basic() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Exec Test",
                "test",
                vec![FormulaStep {
                    prompt: "Do something".to_string(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        let ctx = FormulaExecutionContext {
            skip_checks_enabled: true,
            working_directory: temp.path().to_path_buf(),
            variables: HashMap::new(),
        };
        let result = manager.execute_step(&formula, 0, &ctx).unwrap();
        assert_eq!(result.step_index, 0);
        assert!(!result.skipped);
        assert!(result.verified.is_none());
    }

    #[test]
    fn test_execute_step_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Skip Test",
                "test",
                vec![
                    FormulaStep {
                        prompt: "Step one".into(),
                        skip_condition: None,
                        verification_command: None,
                    },
                    FormulaStep {
                        prompt: "Step two".into(),
                        skip_condition: Some("true".into()),
                        verification_command: None,
                    },
                ],
            )
            .unwrap();

        let ctx = FormulaExecutionContext {
            skip_checks_enabled: true,
            working_directory: temp.path().to_path_buf(),
            variables: HashMap::new(),
        };
        let result = manager.execute_step(&formula, 1, &ctx).unwrap();
        assert!(result.skipped);
        assert!(result.verified.is_none());
    }

    #[test]
    fn test_execute_step_skip_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "No Skip",
                "test",
                vec![FormulaStep {
                    prompt: "Step".into(),
                    skip_condition: Some("true".into()),
                    verification_command: None,
                }],
            )
            .unwrap();

        let ctx = FormulaExecutionContext {
            skip_checks_enabled: false,
            working_directory: temp.path().to_path_buf(),
            variables: HashMap::new(),
        };
        let result = manager.execute_step(&formula, 0, &ctx).unwrap();
        assert!(!result.skipped);
    }

    #[test]
    fn test_execute_step_out_of_range() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Range Test",
                "test",
                vec![FormulaStep {
                    prompt: "only step".into(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        let ctx = FormulaExecutionContext::default();
        let result = manager.execute_step(&formula, 5, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_with_verification() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Verify Test",
                "test",
                vec![FormulaStep {
                    prompt: "Create file".into(),
                    skip_condition: None,
                    verification_command: Some("echo ok".into()),
                }],
            )
            .unwrap();

        let ctx = FormulaExecutionContext {
            skip_checks_enabled: true,
            working_directory: temp.path().to_path_buf(),
            variables: HashMap::new(),
        };
        let result = manager.execute_step(&formula, 0, &ctx).unwrap();
        assert_eq!(result.verified, Some(true));
    }

    #[test]
    fn test_execute_with_template_variables() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Template Test",
                "test",
                vec![FormulaStep {
                    prompt: "Deploy to {{environment}} with version {{version}}".into(),
                    skip_condition: None,
                    verification_command: None,
                }],
            )
            .unwrap();

        let mut vars = HashMap::new();
        vars.insert("environment".into(), "production".into());
        vars.insert("version".into(), "1.2.3".into());

        let ctx = FormulaExecutionContext {
            skip_checks_enabled: true,
            working_directory: temp.path().to_path_buf(),
            variables: vars,
        };
        let result = manager.execute_step(&formula, 0, &ctx).unwrap();
        assert!(result.prompt.contains("production"));
        assert!(result.prompt.contains("1.2.3"));
        assert!(!result.prompt.contains("{{"));
    }

    #[test]
    fn test_execute_formula_full_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        let formula = manager
            .create(
                "Full Workflow",
                "complete workflow test",
                vec![
                    FormulaStep {
                        prompt: "Initialize".into(),
                        skip_condition: None,
                        verification_command: None,
                    },
                    FormulaStep {
                        prompt: "Skip me".into(),
                        skip_condition: Some("true".into()),
                        verification_command: None,
                    },
                    FormulaStep {
                        prompt: "Final step".into(),
                        skip_condition: None,
                        verification_command: Some("echo done".into()),
                    },
                ],
            )
            .unwrap();

        let ctx = FormulaExecutionContext {
            skip_checks_enabled: true,
            working_directory: temp.path().to_path_buf(),
            variables: HashMap::new(),
        };
        let results = manager.execute_formula(&formula, &ctx).unwrap();
        assert_eq!(results.len(), 3);
        assert!(!results[0].skipped);
        assert!(results[1].skipped);
        assert!(!results[2].skipped);
        assert_eq!(results[2].verified, Some(true));
    }

    #[test]
    fn test_yaml_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let manager = FormulaManager::new(temp.path()).unwrap();

        manager
            .create(
                "YAML Test",
                "roundtrip test",
                vec![
                    FormulaStep {
                        prompt: "Full step".into(),
                        skip_condition: Some("skip me".into()),
                        verification_command: Some("verify me".into()),
                    },
                    FormulaStep {
                        prompt: "Minimal step".into(),
                        skip_condition: None,
                        verification_command: None,
                    },
                ],
            )
            .unwrap();

        let loaded = manager.load("YAML Test").unwrap();
        assert_eq!(loaded.name, "YAML Test");
        assert_eq!(loaded.description, "roundtrip test");
        assert_eq!(loaded.steps.len(), 2);
        assert_eq!(loaded.steps[0].skip_condition, Some("skip me".to_string()));
        assert_eq!(
            loaded.steps[0].verification_command,
            Some("verify me".to_string())
        );
        assert!(loaded.steps[1].skip_condition.is_none());
        assert!(loaded.steps[1].verification_command.is_none());
    }

    #[test]
    fn test_evaluate_skip_condition() {
        assert!(evaluate_skip_condition("true"));
        assert!(evaluate_skip_condition("True"));
        assert!(evaluate_skip_condition("TRUE"));
        assert!(evaluate_skip_condition("yes"));
        assert!(evaluate_skip_condition("YES"));
        assert!(evaluate_skip_condition("1"));
        assert!(evaluate_skip_condition("pass"));
        assert!(evaluate_skip_condition("PASS"));
        assert!(!evaluate_skip_condition("false"));
        assert!(!evaluate_skip_condition("tests pass"));
        assert!(!evaluate_skip_condition(""));
        assert!(!evaluate_skip_condition("random text"));
    }

    #[test]
    fn test_resolve_template_variables() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "world".into());
        vars.insert("version".into(), "2.0".into());

        let result = resolve_template_variables("Hello {{name}}, version {{version}}", &vars);
        assert_eq!(result, "Hello world, version 2.0");

        let unresolved = resolve_template_variables("No vars here {{unknown}}", &vars);
        assert_eq!(unresolved, "No vars here {{unknown}}");
    }

    #[test]
    fn test_step_result_display() {
        let skipped = StepResult {
            step_index: 1,
            prompt: "test".into(),
            skipped: true,
            verified: None,
            output: "".into(),
        };
        assert!(format!("{}", skipped).contains("SKIPPED"));

        let verified_ok = StepResult {
            step_index: 0,
            prompt: "test".into(),
            skipped: false,
            verified: Some(true),
            output: "".into(),
        };
        let display = format!("{}", verified_ok);
        assert!(display.contains("✅"));

        let verified_fail = StepResult {
            step_index: 0,
            prompt: "test".into(),
            skipped: false,
            verified: Some(false),
            output: "".into(),
        };
        assert!(format!("{}", verified_fail).contains("❌"));
    }
}
