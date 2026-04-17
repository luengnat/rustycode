//! Oracle agent — runs the pre-written solution script.

use std::path::PathBuf;

use anyhow::{bail, Context};

use super::BenchAgent;
use crate::environment::BenchEnvironment;

/// Agent that runs the oracle solution (solve.sh) from the task.
///
/// This is used for infrastructure validation: if the oracle solution
/// passes verification, the environment and verifier are working correctly.
pub struct OracleAgent {
    /// Path to the solution directory (contains solve.sh).
    solution_dir: PathBuf,
}

impl OracleAgent {
    #[must_use]
    pub const fn new(solution_dir: PathBuf) -> Self {
        Self { solution_dir }
    }
}

#[async_trait::async_trait]
impl BenchAgent for OracleAgent {
    fn name(&self) -> &'static str {
        "oracle"
    }

    async fn setup(&mut self, env: &mut dyn BenchEnvironment) -> anyhow::Result<()> {
        // Upload the solution script to the container
        let solve_script = self.solution_dir.join("solve.sh");
        if solve_script.exists() {
            env.upload_file(&solve_script, "/tmp/solve.sh").await?;
            env.exec("chmod +x /tmp/solve.sh").await?;
            tracing::info!("Uploaded oracle solution script");
        }
        Ok(())
    }

    async fn run(
        &mut self,
        _instruction: &str,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()> {
        let solve_script = self.solution_dir.join("solve.sh");
        if !solve_script.exists() {
            bail!("Oracle solution not found at {}", solve_script.display());
        }

        tracing::info!("Running oracle solution...");
        let result = env
            .exec_with_timeout("bash /tmp/solve.sh", 600)
            .await
            .context("oracle solution execution failed")?;

        if !result.success() {
            tracing::warn!(
                "Oracle solution exited with code {}: {}",
                result.exit_code,
                result.stderr
            );
        }

        tracing::info!("Oracle solution completed (exit {})", result.exit_code);
        Ok(())
    }
}
