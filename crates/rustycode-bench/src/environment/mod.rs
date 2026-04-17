//! Environment abstraction for benchmark containers.

pub mod docker;

use std::path::Path;

pub use docker::{container_paths, DockerEnvironment, EnvironmentConfig, TrialPaths};

/// Result of executing a command inside a container.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ExecResult {
    #[must_use]
    pub const fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Container environment for running benchmark tasks.
///
/// Implementations manage container lifecycle (docker/podman compose)
/// and provide command execution, file upload/download capabilities.
#[async_trait::async_trait]
pub trait BenchEnvironment: Send + Sync {
    /// Start the container. If `force_build`, rebuild the image from Dockerfile
    /// instead of pulling a prebuilt image.
    async fn start(&mut self, force_build: bool) -> anyhow::Result<()>;

    /// Stop the container. If `delete`, remove images and volumes.
    async fn stop(&mut self, delete: bool) -> anyhow::Result<()>;

    /// Execute a command inside the container.
    async fn exec(&self, command: &str) -> anyhow::Result<ExecResult>;

    /// Execute a command with a timeout in seconds.
    async fn exec_with_timeout(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult>;

    /// Upload a file from host into the container.
    async fn upload_file(&self, src: &Path, dest: &str) -> anyhow::Result<()>;

    /// Download a file from the container to the host.
    async fn download_file(&self, src: &str, dest: &Path) -> anyhow::Result<()>;
}
