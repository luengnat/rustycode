//! Runs RustyCode on a SWE-bench instance and collects the prediction

use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use crate::error::{OrchestraV2Error, Result};
use crate::swebench::instance::{load_instances, SweBenchInstance};
use crate::swebench::report::{write_predictions, write_predictions_jsonl, Prediction};

/// Default model name for predictions
const MODEL_NAME: &str = "rustycode-orchestra2";

/// Runner that orchestrates SWE-bench evaluation
pub struct SweBenchRunner {
    /// Path to SWE-bench instances JSON/JSONL file
    pub instances_path: PathBuf,
    /// Path to write predictions output
    pub output_path: PathBuf,
    /// Cost budget per instance (USD)
    pub budget: f64,
    /// Number of instances to run in parallel
    pub parallel: usize,
    /// Specific instance IDs to run (None = all)
    pub instance_ids: Option<Vec<String>>,
    /// Output format: "json" or "jsonl"
    pub format: String,
}

impl SweBenchRunner {
    /// Create a new runner with the given configuration
    pub fn new(
        instances_path: PathBuf,
        output_path: PathBuf,
        budget: f64,
        parallel: usize,
        instance_ids: Option<Vec<String>>,
    ) -> Self {
        Self {
            instances_path,
            output_path,
            budget,
            parallel,
            instance_ids,
            format: "json".to_string(),
        }
    }

    /// Run all selected instances and return predictions
    pub async fn run_all(&self) -> Result<Vec<Prediction>> {
        info!(
            "SWE-bench runner starting: instances={}, output={}, budget=${:.2}, parallel={}",
            self.instances_path.display(),
            self.output_path.display(),
            self.budget,
            self.parallel,
        );

        let all_instances = load_instances(&self.instances_path)?;
        let instances = self.filter_instances(all_instances);

        if instances.is_empty() {
            warn!("No instances to run");
            return Ok(Vec::new());
        }

        info!("Running {} instance(s)", instances.len());

        let mut predictions = Vec::with_capacity(instances.len());

        for instance in &instances {
            info!(">>> Instance: {}", instance.instance_id);
            match self.run_instance(instance).await {
                Ok(prediction) => {
                    info!(
                        "<<< Instance {} completed successfully",
                        instance.instance_id
                    );
                    predictions.push(prediction);
                }
                Err(e) => {
                    error!("<<< Instance {} FAILED: {}", instance.instance_id, e);
                    // Record empty prediction so evaluation can count it as unresolved
                    predictions.push(Prediction {
                        instance_id: instance.instance_id.clone(),
                        model_patch: String::new(),
                        model_name_or_path: MODEL_NAME.to_string(),
                    });
                }
            }
        }

        // Write predictions to file
        if self.format == "jsonl" {
            write_predictions_jsonl(&predictions, &self.output_path)?;
        } else {
            write_predictions(&predictions, &self.output_path)?;
        }

        info!(
            "Wrote {} prediction(s) to {}",
            predictions.len(),
            self.output_path.display()
        );

        let succeeded = predictions
            .iter()
            .filter(|p| !p.model_patch.is_empty())
            .count();
        let failed = predictions.len() - succeeded;
        info!("Results: {} succeeded, {} failed", succeeded, failed);

        Ok(predictions)
    }

    /// Run a single instance: clone repo, checkout commit, run Autonomous Mode, collect diff.
    ///
    /// The actual Autonomous Mode execution is currently a stub that collects a placeholder
    /// prediction. Integration with the full Autonomous Mode executor will be wired once
    /// the headless execution path is production-ready.
    pub async fn run_instance(&self, instance: &SweBenchInstance) -> Result<Prediction> {
        info!(
            "Processing instance: {} ({} @ {})",
            instance.instance_id, instance.repo, instance.base_commit
        );

        // TODO: Wire Autonomous Mode headless execution here.
        //
        // The full pipeline will be:
        // 1. Clone the repo to a temp directory
        // 2. Checkout the base_commit
        // 3. Apply the test_patch
        // 4. Build the problem statement as a Autonomous Mode task
        // 5. Run Orchestra2Executor::run() within the budget
        // 6. Collect the git diff as the model_patch
        //
        // For now, produce a stub prediction so the CLI wiring and report
        // format can be validated end-to-end.

        let model_patch = collect_prediction_patch_stub(instance)?;

        Ok(Prediction {
            instance_id: instance.instance_id.clone(),
            model_patch,
            model_name_or_path: MODEL_NAME.to_string(),
        })
    }

    /// Filter instances to only those matching the requested IDs
    fn filter_instances(&self, instances: Vec<SweBenchInstance>) -> Vec<SweBenchInstance> {
        if let Some(ref ids) = self.instance_ids {
            let id_set: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
            instances
                .into_iter()
                .filter(|i| id_set.contains(i.instance_id.as_str()))
                .collect()
        } else {
            instances
        }
    }
}

/// Collect the git diff as a prediction patch (stub).
///
/// In production, this will run `git diff` in the repo directory after
/// Autonomous Mode has made its changes. For now, returns an empty patch.
fn collect_prediction_patch_stub(instance: &SweBenchInstance) -> Result<String> {
    // TODO: Replace with actual `git diff` collection after Autonomous Mode execution.
    // The real implementation will:
    //   let output = tokio::process::Command::new("git")
    //       .args(["diff"])
    //       .current_dir(&repo_path)
    //       .output()
    //       .await?;
    //   String::from_utf8_lossy(&output.stdout).to_string()

    info!(
        "Collecting prediction patch for {} (stub)",
        instance.instance_id
    );
    Ok(String::new())
}

/// Run `git diff` in a repository directory and return the output.
///
/// This is the production path for collecting prediction patches after
/// Autonomous Mode has modified files. Used by the full execution pipeline.
pub async fn collect_prediction_patch(repo_path: &Path) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(["diff"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| OrchestraV2Error::Git(format!("Failed to run git diff: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrchestraV2Error::Git(format!(
            "git diff failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Clone a repo and checkout a specific commit.
///
/// Returns the path to the cloned repository directory.
pub async fn clone_and_checkout(repo: &str, base_commit: &str, work_dir: &Path) -> Result<PathBuf> {
    let repo_url = format!("https://github.com/{}.git", repo);
    let repo_name = repo.replace('/', "_");
    let clone_path = work_dir.join(&repo_name);

    if clone_path.exists() {
        info!("Repository already cloned at {}", clone_path.display());
    } else {
        info!("Cloning {} into {}", repo_url, clone_path.display());
        let output = tokio::process::Command::new("git")
            .args(["clone", &repo_url, clone_path.to_str().unwrap_or(".")])
            .output()
            .await
            .map_err(|e| OrchestraV2Error::Git(format!("Failed to clone {}: {}", repo_url, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestraV2Error::Git(format!(
                "git clone failed for {}: {}",
                repo_url,
                stderr.trim()
            )));
        }
    }

    info!("Checking out {}", base_commit);
    let output = tokio::process::Command::new("git")
        .args(["checkout", base_commit])
        .current_dir(&clone_path)
        .output()
        .await
        .map_err(|e| OrchestraV2Error::Git(format!("Failed to checkout {}: {}", base_commit, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrchestraV2Error::Git(format!(
            "git checkout {} failed: {}",
            base_commit,
            stderr.trim()
        )));
    }

    Ok(clone_path)
}

/// Apply a patch using `git apply`
pub async fn apply_patch(repo_path: &Path, _patch: &str) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args(["apply"])
        .stdin(std::process::Stdio::piped())
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| OrchestraV2Error::Git(format!("Failed to run git apply: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrchestraV2Error::Git(format!(
            "git apply failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CRATE_TEST_LOCK;

    fn make_test_instance(id: &str) -> SweBenchInstance {
        SweBenchInstance {
            instance_id: id.to_string(),
            repo: "test/repo".to_string(),
            version: "1.0".to_string(),
            base_commit: "abc123".to_string(),
            problem_statement: "Fix the bug".to_string(),
            hints_text: None,
            created_at: "2024-01-01T00:00:00".to_string(),
            test_patch: "".to_string(),
            patch: "".to_string(),
            fail_to_pass: vec![],
            pass_to_pass: vec![],
        }
    }

    #[test]
    fn runner_filters_by_instance_ids() {
        let _guard = CRATE_TEST_LOCK.lock();
        let runner = SweBenchRunner::new(
            PathBuf::from("/dev/null"),
            PathBuf::from("/dev/null"),
            1.0,
            1,
            Some(vec!["a".to_string(), "c".to_string()]),
        );

        let instances = vec![
            make_test_instance("a"),
            make_test_instance("b"),
            make_test_instance("c"),
        ];

        let filtered = runner.filter_instances(instances);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].instance_id, "a");
        assert_eq!(filtered[1].instance_id, "c");
    }

    #[test]
    fn runner_no_filter_returns_all() {
        let _guard = CRATE_TEST_LOCK.lock();
        let runner = SweBenchRunner::new(
            PathBuf::from("/dev/null"),
            PathBuf::from("/dev/null"),
            1.0,
            1,
            None,
        );

        let instances = vec![make_test_instance("a"), make_test_instance("b")];

        let filtered = runner.filter_instances(instances);
        assert_eq!(filtered.len(), 2);
    }

    #[tokio::test]
    async fn run_instance_stub_returns_empty_patch() {
        let dir = tempfile::tempdir().unwrap();
        let runner = SweBenchRunner::new(
            dir.path().join("instances.json"),
            dir.path().join("pred.json"),
            0.50,
            1,
            None,
        );

        let instance = make_test_instance("test__001");
        let pred = runner.run_instance(&instance).await.unwrap();
        assert_eq!(pred.instance_id, "test__001");
        assert_eq!(pred.model_name_or_path, "rustycode-orchestra2");
        assert!(pred.model_patch.is_empty()); // stub returns empty
    }

    #[test]
    fn collect_prediction_patch_stub_returns_empty() {
        let _guard = CRATE_TEST_LOCK.lock();
        let instance = make_test_instance("test__001");
        let patch = collect_prediction_patch_stub(&instance).unwrap();
        assert!(patch.is_empty());
    }
}
