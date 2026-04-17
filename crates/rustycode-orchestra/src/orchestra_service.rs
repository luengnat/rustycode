//! Shared Orchestra control-plane service.
//!
//! This module centralizes the Orchestra entry points used by CLI and TUI:
//! bootstrap/init, quick-task seeding, and canonical Orchestra2Executor creation.

use anyhow::{Context, Result};
use rustycode_llm::{
    create_provider_v2, create_provider_with_config, load_model_from_config,
    load_provider_config_from_env, load_provider_type_from_config, LLMProvider,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::project_bootstrap::{
    bootstrap_default_project, bootstrap_project, bootstrap_quick_task_project, BootstrapInfo,
};
use crate::Orchestra2Executor;
use crate::OrchestraProjectConfig;

#[derive(Clone)]
pub struct ProviderBundle {
    pub provider_type: String,
    pub model: String,
    pub provider: Arc<dyn LLMProvider>,
}

impl std::fmt::Debug for ProviderBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderBundle")
            .field("provider_type", &self.provider_type)
            .field("model", &self.model)
            .field("provider", &"<LLMProvider>")
            .finish()
    }
}

impl ProviderBundle {
    pub fn describe(&self) -> String {
        format!("{} ({})", self.model, self.provider_type)
    }
}

pub struct OrchestraService;

impl OrchestraService {
    pub async fn init_project(
        project_root: &Path,
        name: &str,
        description: &str,
        vision: &str,
    ) -> Result<BootstrapInfo> {
        bootstrap_project(
            project_root,
            name,
            description,
            vision,
            "Implement the first meaningful improvement for this project",
            "Initial improvement",
        )
        .await
    }

    pub async fn quick_project(
        project_root: &Path,
        task_description: &str,
    ) -> Result<BootstrapInfo> {
        bootstrap_quick_task_project(project_root, task_description).await
    }

    pub async fn bootstrap_default_if_needed(project_root: &Path) -> Result<Option<BootstrapInfo>> {
        if project_root.join(".orchestra/milestones").exists() {
            Ok(None)
        } else {
            Ok(Some(bootstrap_default_project(project_root).await?))
        }
    }

    pub async fn bootstrap_quick_if_needed(
        project_root: &Path,
        task_description: &str,
    ) -> Result<Option<BootstrapInfo>> {
        if project_root.join(".orchestra/milestones").exists() {
            Ok(None)
        } else {
            Ok(Some(
                bootstrap_quick_task_project(project_root, task_description).await?,
            ))
        }
    }

    pub fn resolve_provider() -> Result<ProviderBundle> {
        match load_provider_config_from_env() {
            Ok((provider_type, model, config)) => {
                // Validate: non-local providers require an API key
                let needs_api_key = !matches!(
                    provider_type.to_lowercase().as_str(),
                    "ollama" | "local" | "lmstudio"
                );
                if needs_api_key && config.api_key.is_none() {
                    let env_var = Self::api_key_env_for_provider(&provider_type);
                    anyhow::bail!(
                        "No API key configured for provider '{}'. \
                         Set the {} environment variable or run `rustycode config` \
                         to configure a provider with an API key.",
                        provider_type,
                        env_var
                    );
                }

                let provider = create_provider_with_config(&provider_type, &model, config)
                    .context("Failed to create provider from environment config")?;
                Ok(ProviderBundle {
                    provider_type,
                    model,
                    provider,
                })
            }
            Err(env_err) => {
                let provider_type = load_provider_type_from_config()
                    .context("Failed to load provider type from config")?;
                let model = load_model_from_config().context("Failed to load model from config")?;

                // Validate: non-local providers require an API key in fallback path too
                let needs_api_key = !matches!(
                    provider_type.to_lowercase().as_str(),
                    "ollama" | "local" | "lmstudio"
                );
                if needs_api_key {
                    let env_var = Self::api_key_env_for_provider(&provider_type);
                    let has_key = std::env::var(&env_var)
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false);
                    if !has_key {
                        anyhow::bail!(
                            "No API key configured for provider '{}'. \
                             Set the {} environment variable or run `rustycode config` \
                             to configure a provider with an API key. \
                             (Environment config loader also failed: {})",
                            provider_type,
                            env_var,
                            env_err
                        );
                    }
                }

                let provider = create_provider_v2(&provider_type, &model).with_context(|| {
                    format!(
                        "Failed to create provider from config (env loader also failed: {})",
                        env_err
                    )
                })?;

                Ok(ProviderBundle {
                    provider_type,
                    model,
                    provider,
                })
            }
        }
    }

    /// Return the standard environment variable name for a provider's API key.
    fn api_key_env_for_provider(provider_type: &str) -> String {
        match provider_type.to_lowercase().as_str() {
            "openai" | "open_ai" => "OPENAI_API_KEY".to_string(),
            "anthropic" | "claude" => "ANTHROPIC_API_KEY".to_string(),
            "openrouter" => "OPENROUTER_API_KEY".to_string(),
            "gemini" | "google" => "GOOGLE_API_KEY".to_string(),
            "copilot" | "github" => "GITHUB_TOKEN".to_string(),
            "bedrock" | "aws" => "AWS_ACCESS_KEY_ID".to_string(),
            "azure" | "azure_openai" | "microsoft" => "AZURE_OPENAI_API_KEY".to_string(),
            "cohere" => "COHERE_API_KEY".to_string(),
            "mistral" | "mistral_ai" => "MISTRAL_API_KEY".to_string(),
            "together" | "together_ai" => "TOGETHER_API_KEY".to_string(),
            "perplexity" | "pplx" => "PERPLEXITY_API_KEY".to_string(),
            "huggingface" | "hf" => "HF_API_KEY".to_string(),
            _ => format!("{}_API_KEY", provider_type.to_uppercase().replace('-', "_")),
        }
    }

    pub fn create_executor(project_root: PathBuf, budget: f64) -> Result<Orchestra2Executor> {
        let provider = Self::resolve_provider()?;
        let executor = Orchestra2Executor::new(
            project_root.clone(),
            provider.provider,
            provider.model,
            budget,
        );

        // Load and apply task model overrides if they exist
        let orchestra_config = OrchestraProjectConfig::load(&project_root)?;
        Ok(match orchestra_config {
            Some(cfg) => executor.with_task_model_overrides(cfg),
            None => executor,
        })
    }

    pub async fn run_auto(project_root: PathBuf, budget: f64) -> Result<Option<BootstrapInfo>> {
        let bootstrap = Self::bootstrap_default_if_needed(&project_root).await?;
        let executor = Self::create_executor(project_root, budget)?;
        executor.run().await?;
        Ok(bootstrap)
    }

    pub async fn run_quick_task(
        project_root: PathBuf,
        task_description: String,
        budget: f64,
    ) -> Result<Option<BootstrapInfo>> {
        let bootstrap = Self::bootstrap_quick_if_needed(&project_root, &task_description).await?;
        let executor = Self::create_executor(project_root, budget)?;
        executor.run().await?;
        Ok(bootstrap)
    }
}

#[cfg(test)]
mod tests {
    use super::OrchestraService;
    use tempfile::TempDir;

    #[tokio::test]
    async fn bootstrap_default_if_needed_creates_fresh_project() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let info = OrchestraService::bootstrap_default_if_needed(&root)
            .await
            .unwrap();
        let info = info.expect("expected bootstrap info for fresh project");
        let expected_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap()
            .to_string();

        assert_eq!(info.project_name, expected_name);
        assert_eq!(info.task_id, "T01");
        assert!(root.join(".orchestra/STATE.md").exists());
    }

    #[tokio::test]
    async fn bootstrap_default_if_needed_skips_existing_project() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let first = OrchestraService::bootstrap_default_if_needed(&root)
            .await
            .unwrap();
        assert!(first.is_some());

        let second = OrchestraService::bootstrap_default_if_needed(&root)
            .await
            .unwrap();
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn bootstrap_quick_if_needed_creates_fresh_project() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let info = OrchestraService::bootstrap_quick_if_needed(&root, "Fix the login flow")
            .await
            .unwrap();
        let info = info.expect("expected bootstrap info for fresh quick task");

        assert_eq!(info.task_title, "Fix the login flow");
        assert_eq!(info.task_goal, "Initial quick improvement");
        assert!(root
            .join(".orchestra/milestones/M01/slices/S01/tasks/T01/T01-PLAN.md")
            .exists());
    }
}
