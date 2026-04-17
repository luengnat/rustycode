# Provider Registry & Bootstrap System - Detailed Design

## Overview

This document provides detailed implementation specifications for the Provider Registry & Bootstrap System, incorporating patterns from opencoderust (bootstrap, metadata), kilocode (gateway routing), and gemini-cli (dynamic discovery).

## Architecture

```
Provider System Architecture:
┌─────────────────────────────────────────────────────────────┐
│ Bootstrap Configuration                                     │
│ ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  │
│ │ Providers     │  │ Custom        │  │ Models        │  │
│ │ Configuration │  │ Loaders       │  │ Metadata      │  │
│ └───────────────┘  └───────────────┘  └───────────────┘  │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ Provider Bootstrap                                          │
│ ├─ Load Provider Metadata                                   │
│ ├─ Initialize Providers                                     │
│ ├─ Apply Custom Loaders                                     │
│ └─ Register Models                                          │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ Provider Registry                                           │
│ ├─ Model Registry (25+ models)                              │
│ ├─ Provider Registry (10+ providers)                        │
│ ├─ Cost Tracking                                             │
│ └─ Metadata Query                                            │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ Dynamic Discovery Service                                    │
│ ├─ Model Discovery from Endpoints                           │
│ ├─ Capabilities Detection                                    │
│ └─ Cost Information Fetching                                 │
└─────────────────────────────────────────────────────────────┘
```

## Data Structures

### Provider Metadata

```rust
// crates/rustycode-llm/src/models/metadata.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Comprehensive model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    // Identification
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub provider: String,
    pub version: Option<String>,

    // Pricing (in USD per million tokens)
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cached_cost_per_million: Option<f64>,

    // Capabilities
    pub context_limit: usize,
    pub max_output_tokens: usize,
    pub supports_streaming: bool,
    pub supports_function_calling: bool,
    pub supports_vision: bool,
    pub supports_audio_input: bool,
    pub supports_audio_output: bool,

    // Generation parameters
    pub temperature_range: Option<(f32, f32)>,
    pub top_p_range: Option<(f32, f32)>,
    pub top_k_range: Option<(u32, u32)>,
    pub default_temperature: Option<f32>,
    pub default_max_tokens: Option<usize>,

    // Performance characteristics
    pub quality_score: f64,        // 0.0 - 1.0
    pub speed_score: f64,          // 0.0 - 1.0
    pub latency_ms: Option<f64>,    // Average latency
    pub tokens_per_second: Option<f64>,

    // Provider-specific
    pub provider_metadata: HashMap<String, serde_json::Value>,

    // Classification
    pub model_type: ModelType,
    pub tier: ModelTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelType {
    Chat,
    Completion,
    Embedding,
    ImageGeneration,
    AudioTranscription,
    AudioGeneration,
    FunctionCalling,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelTier {
    Enterprise,
    Production,
    Beta,
    Experimental,
}

impl ModelMetadata {
    /// Calculate cost for a request
    pub fn estimate_cost(
        &self,
        input_tokens: usize,
        output_tokens: usize,
        use_cache: bool,
    ) -> CostEstimate {
        let input_cost = (input_tokens as f64 / 1_000_000.0)
            * self.input_cost_per_million;

        let output_cost = (output_tokens as f64 / 1_000_000.0)
            * self.output_cost_per_million;

        let cached_cost = if use_cache && self.cached_cost_per_million.is_some() {
            (input_tokens as f64 / 1_000_000.0)
                * self.cached_cost_per_million.unwrap()
        } else {
            0.0
        };

        CostEstimate {
            input_cost,
            output_cost,
            cached_cost,
            total_cost: input_cost + output_cost,
        }
    }

    /// Check if model supports a capability
    pub fn supports(&self, capability: Capability) -> bool {
        match capability {
            Capability::Streaming => self.supports_streaming,
            Capability::FunctionCalling => self.supports_function_calling,
            Capability::Vision => self.supports_vision,
            Capability::AudioInput => self.supports_audio_input,
            Capability::AudioOutput => self.supports_audio_output,
        }
    }

    /// Get optimal parameters for this model
    pub fn get_optimal_parameters(&self) -> ModelParameters {
        ModelParameters {
            temperature: self.default_temperature.unwrap_or(0.1),
            max_tokens: self.default_max_tokens.unwrap_or(4096),
            top_p: self.top_p_range
                .map(|(min, _)| min)
                .unwrap_or(0.9),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cached_cost: f64,
    pub total_cost: f64,
}

#[derive(Debug, Clone)]
pub struct ModelParameters {
    pub temperature: f32,
    pub max_tokens: usize,
    pub top_p: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Streaming,
    FunctionCalling,
    Vision,
    AudioInput,
    AudioOutput,
}

/// Provider metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    // Identification
    pub id: String,
    pub name: String,
    pub display_name: String,

    // Authentication
    pub requires_auth: bool,
    pub auth_type: AuthType,
    pub auth_docs_url: Option<String>,

    // API Configuration
    pub base_url: Option<String>,
    pub api_version: Option<String>,
    pub headers: HashMap<String, String>,

    // Capabilities
    pub capabilities: ProviderCapabilities,

    // Rate Limits
    pub rate_limits: RateLimits,

    // Status
    pub status: ProviderStatus,

    // Documentation
    pub docs_url: Option<String>,
    pub models_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub function_calling: bool,
    pub vision: bool,
    pub audio_input: bool,
    pub audio_output: bool,
    pub max_concurrent_requests: usize,
    pub supports_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub requests_per_minute: Option<usize>,
    pub tokens_per_minute: Option<usize>,
    pub concurrent_requests: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthType {
    None,
    ApiKey,
    BearerToken,
    OAuth2,
    BasicAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderStatus {
    Available,
    Degraded,
    Unavailable,
}
```

### Bootstrap Configuration

```rust
// crates/rustycode-llm/src/bootstrap/config.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    pub providers: Vec<ProviderBootstrapConfig>,
    pub models: Vec<ModelBootstrapConfig>,
    pub custom_loaders: Vec<CustomLoaderConfig>,
    pub discovery: DiscoveryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBootstrapConfig {
    pub id: String,
    pub provider_type: String,
    pub enabled: bool,
    pub priority: i32,
    pub config: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBootstrapConfig {
    pub id: String,
    pub provider_id: String,
    pub aliases: Vec<String>,
    pub capabilities: Vec<String>,
    pub metadata: Option<ModelMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomLoaderConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_secs: u64,
    pub retry_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub endpoints: Vec<DiscoveryEndpoint>,
    pub cache_ttl_secs: u64,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryEndpoint {
    pub id: String,
    pub url: String,
    pub provider: String,
    pub auth_env_var: String,
}
```

## Implementation Details

### 1. Model Registry

```rust
// crates/rustycode-llm/src/models/registry.rs

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct ModelRegistry {
    models: Arc<RwLock<HashMap<String, ModelMetadata>>>,
    providers: Arc<RwLock<HashMap<String, ProviderMetadata>>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            providers: Arc::new(RwLock::new(HashMap::new())),
        };

        registry.load_builtin_metadata();

        registry
    }

    fn load_builtin_metadata(&self) {
        let mut models = self.models.write().unwrap();
        let mut providers = self.providers.write().unwrap();

        // Anthropic
        providers.insert("anthropic".into(), ProviderMetadata {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            display_name: "Anthropic".into(),
            requires_auth: true,
            auth_type: AuthType::ApiKey,
            auth_docs_url: Some("https://docs.anthropic.com/claude/reference/getting-started-with-the-api".into()),
            base_url: Some("https://api.anthropic.com".into()),
            api_version: Some("2023-06-01".into()),
            headers: {
                let mut map = HashMap::new();
                map.insert("anthropic-version".into(), "2023-06-01".into());
                map
            },
            capabilities: ProviderCapabilities {
                streaming: true,
                function_calling: true,
                vision: true,
                audio_input: false,
                audio_output: false,
                max_concurrent_requests: 10,
                supports_cache: true,
            },
            rate_limits: RateLimits {
                requests_per_minute: Some(50),
                tokens_per_minute: Some(40_000),
                concurrent_requests: 10,
            },
            status: ProviderStatus::Available,
            docs_url: Some("https://docs.anthropic.com".into()),
            models_url: Some("https://docs.anthropic.com/claude/docs/models-overview".into()),
        });

        models.insert("claude-3-5-sonnet-20250514".into(), ModelMetadata {
            id: "claude-3-5-sonnet-20250514".into(),
            name: "claude-3-5-sonnet-20250514".into(),
            display_name: "Claude 3.5 Sonnet".into(),
            provider: "anthropic".into(),
            version: Some("20250514".into()),
            input_cost_per_million: 3.0,
            output_cost_per_million: 15.0,
            cached_cost_per_million: Some(0.30),
            context_limit: 200_000,
            max_output_tokens: 8192,
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: true,
            supports_audio_input: false,
            supports_audio_output: false,
            temperature_range: Some((0.0, 1.0)),
            top_p_range: Some((0.0, 1.0)),
            top_k_range: None,
            default_temperature: Some(0.1),
            default_max_tokens: Some(4096),
            quality_score: 0.95,
            speed_score: 0.85,
            latency_ms: Some(800.0),
            tokens_per_second: Some(65.0),
            provider_metadata: {
                let mut map = HashMap::new();
                map.insert("claude_2_compatible".into(), serde_json::json!(false));
                map
            },
            model_type: ModelType::Chat,
            tier: ModelTier::Production,
        });

        models.insert("claude-3-5-haiku-20241022".into(), ModelMetadata {
            id: "claude-3-5-haiku-20241022".into(),
            name: "claude-3-5-haiku-20241022".into(),
            display_name: "Claude 3.5 Haiku".into(),
            provider: "anthropic".into(),
            version: Some("20241022".into()),
            input_cost_per_million: 0.80,
            output_cost_per_million: 4.0,
            cached_cost_per_million: Some(0.08),
            context_limit: 200_000,
            max_output_tokens: 8192,
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: true,
            supports_audio_input: false,
            supports_audio_output: false,
            temperature_range: Some((0.0, 1.0)),
            top_p_range: Some((0.0, 1.0)),
            top_k_range: None,
            default_temperature: Some(0.1),
            default_max_tokens: Some(4096),
            quality_score: 0.80,
            speed_score: 0.95,
            latency_ms: Some(300.0),
            tokens_per_second: Some(150.0),
            provider_metadata: HashMap::new(),
            model_type: ModelType::Chat,
            tier: ModelTier::Production,
        });

        // OpenAI
        providers.insert("openai".into(), ProviderMetadata {
            id: "openai".into(),
            name: "OpenAI".into(),
            display_name: "OpenAI".into(),
            requires_auth: true,
            auth_type: AuthType::ApiKey,
            auth_docs_url: Some("https://platform.openai.com/docs/quickstart".into()),
            base_url: Some("https://api.openai.com/v1".into()),
            api_version: None,
            headers: HashMap::new(),
            capabilities: ProviderCapabilities {
                streaming: true,
                function_calling: true,
                vision: true,
                audio_input: true,
                audio_output: true,
                max_concurrent_requests: 100,
                supports_cache: true,
            },
            rate_limits: RateLimits {
                requests_per_minute: Some(500),
                tokens_per_minute: Some(150_000),
                concurrent_requests: 100,
            },
            status: ProviderStatus::Available,
            docs_url: Some("https://platform.openai.com/docs".into()),
            models_url: Some("https://platform.openai.com/docs/models".into()),
        });

        models.insert("gpt-4o".into(), ModelMetadata {
            id: "gpt-4o".into(),
            name: "gpt-4o".into(),
            display_name: "GPT-4o".into(),
            provider: "openai".into(),
            version: Some("2024-05-13".into()),
            input_cost_per_million: 2.50,
            output_cost_per_million: 10.0,
            cached_cost_per_million: Some(1.25),
            context_limit: 128_000,
            max_output_tokens: 4096,
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: true,
            supports_audio_input: true,
            supports_audio_output: true,
            temperature_range: Some((0.0, 2.0)),
            top_p_range: Some((0.0, 1.0)),
            top_k_range: None,
            default_temperature: Some(0.1),
            default_max_tokens: Some(4096),
            quality_score: 0.92,
            speed_score: 0.90,
            latency_ms: Some(400.0),
            tokens_per_second: Some(85.0),
            provider_metadata: HashMap::new(),
            model_type: ModelType::Chat,
            tier: ModelTier::Production,
        });

        // OpenRouter
        providers.insert("openrouter".into(), ProviderMetadata {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            display_name: "OpenRouter".into(),
            requires_auth: true,
            auth_type: AuthType::ApiKey,
            auth_docs_url: Some("https://openrouter.ai/docs/quick-start".into()),
            base_url: Some("https://openrouter.ai/api/v1".into()),
            api_version: None,
            headers: {
                let mut map = HashMap::new();
                map.insert("HTTP-Referer".into(), "https://github.com/nat/rustycode".into());
                map.insert("X-Title".into(), "RustyCode".into());
                map
            },
            capabilities: ProviderCapabilities {
                streaming: true,
                function_calling: true,
                vision: true,
                audio_input: false,
                audio_output: false,
                max_concurrent_requests: 20,
                supports_cache: false,
            },
            rate_limits: RateLimits {
                requests_per_minute: None,
                tokens_per_minute: None,
                concurrent_requests: 20,
            },
            status: ProviderStatus::Available,
            docs_url: Some("https://openrouter.ai/docs".into()),
            models_url: Some("https://openrouter.ai/models".into()),
        });
    }

    /// Register a model
    pub fn register_model(&self, metadata: ModelMetadata) {
        let mut models = self.models.write().unwrap();
        models.insert(metadata.id.clone(), metadata);
    }

    /// Get model by ID
    pub fn get_model(&self, id: &str) -> Option<ModelMetadata> {
        let models = self.models.read().unwrap();
        models.get(id).cloned()
    }

    /// List all models
    pub fn list_models(&self) -> Vec<ModelMetadata> {
        let models = self.models.read().unwrap();
        models.values().cloned().collect()
    }

    /// List models for a provider
    pub fn list_models_for_provider(&self, provider_id: &str) -> Vec<ModelMetadata> {
        let models = self.models.read().unwrap();
        models
            .values()
            .filter(|m| m.provider == provider_id)
            .cloned()
            .collect()
    }

    /// Find best model matching criteria
    pub fn find_best_model(&self, criteria: &ModelSelectionCriteria) -> Option<ModelMetadata> {
        let models = self.models.read().unwrap();

        let mut matching: Vec<_> = models
            .values()
            .filter(|m| {
                // Filter by provider
                if let Some(provider) = &criteria.provider {
                    if &m.provider != provider {
                        return false;
                    }
                }

                // Filter by capabilities
                if let Some(required_capabilities) = &criteria.required_capabilities {
                    if !required_capabilities.iter().all(|cap| m.supports(*cap)) {
                        return false;
                    }
                }

                // Filter by cost
                if let Some(max_cost) = criteria.max_cost_per_million {
                    if m.input_cost_per_million > max_cost {
                        return false;
                    }
                }

                // Filter by context
                if let Some(min_context) = criteria.min_context_limit {
                    if m.context_limit < min_context {
                        return false;
                    }
                }

                // Filter by model type
                if let Some(model_type) = &criteria.model_type {
                    if &m.model_type != model_type {
                        return false;
                    }
                }

                // Filter by tier
                if let Some(tier) = &criteria.tier {
                    if &m.tier != tier {
                        return false;
                    }
                }

                true
            })
            .collect();

        // Sort by criteria
        match criteria.sort_by {
            SortBy::Quality => matching.sort_by(|a, b| {
                b.quality_score.partial_cmp(&a.quality_score).unwrap()
            }),
            SortBy::Speed => matching.sort_by(|a, b| {
                b.speed_score.partial_cmp(&a.speed_score).unwrap()
            }),
            SortBy::Cost => matching.sort_by(|a, b| {
                a.input_cost_per_million.partial_cmp(&b.input_cost_per_million).unwrap()
            }),
        };

        matching.first().cloned()
    }

    /// Get provider metadata
    pub fn get_provider(&self, id: &str) -> Option<ProviderMetadata> {
        let providers = self.providers.read().unwrap();
        providers.get(id).cloned()
    }

    /// List all providers
    pub fn list_providers(&self) -> Vec<ProviderMetadata> {
        let providers = self.providers.read().unwrap();
        providers.values().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct ModelSelectionCriteria {
    pub provider: Option<String>,
    pub required_capabilities: Option<Vec<Capability>>,
    pub max_cost_per_million: Option<f64>,
    pub min_context_limit: Option<usize>,
    pub model_type: Option<ModelType>,
    pub tier: Option<ModelTier>,
    pub sort_by: SortBy,
}

#[derive(Debug, Clone, Copy)]
pub enum SortBy {
    Quality,
    Speed,
    Cost,
}
```

### 2. Bootstrap System

```rust
// crates/rustycode-llm/src/bootstrap/system.rs

use crate::models::{ModelMetadata, ProviderMetadata};
use crate::provider_v2::{LLMProvider, ProviderConfig};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ProviderBootstrap {
    registry: ModelRegistry,
    providers: HashMap<String, Arc<dyn LLMProvider>>,
    custom_loaders: Vec<Box<dyn CustomLoader>>,
}

impl ProviderBootstrap {
    pub fn new() -> Self {
        Self {
            registry: ModelRegistry::new(),
            providers: HashMap::new(),
            custom_loaders: Vec::new(),
        }
    }

    /// Bootstrap from configuration
    pub async fn bootstrap(&mut self, config: &BootstrapConfig) -> Result<BootstrapError> {
        // Phase 1: Load provider metadata
        self.load_provider_metadata(config).await?;

        // Phase 2: Initialize providers
        self.initialize_providers(config).await?;

        // Phase 3: Apply custom loaders
        self.apply_custom_loaders(&config.custom_loaders).await?;

        // Phase 4: Discover models dynamically if enabled
        if config.discovery.enabled {
            self.discover_models(&config.discovery).await?;
        }

        Ok(())
    }

    async fn load_provider_metadata(&mut self, config: &BootstrapConfig) -> Result<BootstrapError> {
        for model_config in &config.models {
            // Get existing metadata or use provided metadata
            let metadata = if let Some(metadata) = &model_config.metadata {
                metadata.clone()
            } else {
                // Look up in registry
                self.registry.get_model(&model_config.id)
                    .ok_or(BootstrapError::ModelNotFound(model_config.id.clone()))?
            };

            // Register with aliases
            for alias in &model_config.aliases {
                let mut aliased_metadata = metadata.clone();
                aliased_metadata.id = alias.clone();
                aliased_metadata.name = alias.clone();
                self.registry.register_model(aliased_metadata);
            }
        }

        Ok(())
    }

    async fn initialize_providers(
        &mut self,
        config: &BootstrapConfig,
    ) -> Result<BootstrapError> {
        // Sort by priority
        let mut provider_configs = config.providers.clone();
        provider_configs.sort_by_key(|p| p.priority);

        for provider_config in &provider_configs {
            if !provider_config.enabled {
                continue;
            }

            let provider = self.create_provider(provider_config).await?;

            self.providers.insert(
                provider_config.id.clone(),
                provider,
            );
        }

        Ok(())
    }

    async fn create_provider(
        &self,
        config: &ProviderBootstrapConfig,
    ) -> Result<Arc<dyn LLMProvider>, BootstrapError> {
        match config.provider_type.as_str() {
            "anthropic" => {
                Ok(Arc::new(AnthropicProvider::new(
                    config.config.clone(),
                    "claude-3-5-sonnet-20250514".into(),
                )?))
            }
            "openai" => {
                Ok(Arc::new(OpenAiProvider::new(
                    config.config.clone(),
                    "gpt-4o".into(),
                )?))
            }
            "openrouter" => {
                Ok(Arc::new(OpenRouterProvider::new(
                    config.config.clone(),
                    "anthropic/claude-3.5-sonnet".into(),
                )?))
            }
            _ => Err(BootstrapError::UnknownProvider(
                config.provider_type.clone(),
            )),
        }
    }

    async fn apply_custom_loaders(
        &mut self,
        loader_configs: &[CustomLoaderConfig],
    ) -> Result<BootstrapError> {
        for loader_config in loader_configs {
            self.run_custom_loader(loader_config).await?;
        }

        Ok(())
    }

    async fn run_custom_loader(
        &mut self,
        config: &CustomLoaderConfig,
    ) -> Result<BootstrapError> {
        let timeout = Duration::from_secs(config.timeout_secs);

        let output = tokio::time::timeout(
            timeout,
            tokio::process::Command::new(&config.command)
                .args(&config.args)
                .output(),
        )
        .await
        .map_err(|_| BootstrapError::LoaderTimeout(config.name.clone()))?
        .map_err(|e| BootstrapError::LoaderError(config.name.clone(), e.to_string()))?;

        if !output.status.success() {
            return Err(BootstrapError::LoaderFailed(
                config.name.clone(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Parse output
        let loader_output: LoaderOutput = serde_json::from_slice(&output.stdout)
            .map_err(|e| BootstrapError::InvalidLoaderOutput(config.name.clone(), e.to_string()))?;

        // Register models from loader
        for model in loader_output.models {
            self.registry.register_model(model);
        }

        Ok(())
    }

    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    pub fn get_provider(&self, id: &str) -> Option<Arc<dyn LLMProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoaderOutput {
    models: Vec<ModelMetadata>,
    providers: Vec<ProviderMetadata>,
}

#[async_trait]
pub trait CustomLoader: Send + Sync {
    async fn load(&self) -> Result<LoaderOutput, BootstrapError>;
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Unknown provider: {0}")]
    UnknownProvider(String),

    #[error("Loader {0} timed out")]
    LoaderTimeout(String),

    #[error("Loader {0} error: {1}")]
    LoaderError(String, String),

    #[error("Loader {0} failed: {1}")]
    LoaderFailed(String, String),

    #[error("Invalid loader output from {0}: {1}")]
    InvalidLoaderOutput(String, String),
}
```

### 3. Dynamic Discovery Service

```rust
// crates/rustycode-llm/src/discovery/service.rs

use reqwest::Client;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

pub struct ModelDiscoveryService {
    client: Client,
    cache: HashMap<String, CachedModels>,
    cache_ttl: Duration,
}

#[derive(Clone)]
struct CachedModels {
    models: Vec<ModelMetadata>,
    timestamp: SystemTime,
}

impl ModelDiscoveryService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            cache: HashMap::new(),
            cache_ttl: Duration::from_secs(3600), // 1 hour
        }
    }

    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Discover models from an endpoint
    pub async fn discover_models(
        &mut self,
        endpoint: &DiscoveryEndpoint,
    ) -> Result<Vec<ModelMetadata>, DiscoveryError> {
        // Check cache
        if let Some(cached) = self.cache.get(&endpoint.id) {
            if cached.timestamp.elapsed().unwrap() < self.cache_ttl {
                return Ok(cached.models.clone());
            }
        }

        // Fetch models
        let models = self.fetch_models(endpoint).await?;

        // Cache results
        self.cache.insert(
            endpoint.id.clone(),
            CachedModels {
                models: models.clone(),
                timestamp: SystemTime::now(),
            },
        );

        Ok(models)
    }

    async fn fetch_models(
        &self,
        endpoint: &DiscoveryEndpoint,
    ) -> Result<Vec<ModelMetadata>, DiscoveryError> {
        let url = format!("{}/models", endpoint.url);

        let api_key = std::env::var(&endpoint.auth_env_var)
            .map_err(|_| DiscoveryError::MissingApiKey(endpoint.auth_env_var.clone()))?;

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| DiscoveryError::RequestError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DiscoveryError::HttpError(
                response.status().as_u16(),
                response.text().await.unwrap_or_default(),
            ));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| DiscoveryError::ParseError(e.to_string()))?;

        self.parse_models_response(json, &endpoint.provider)
    }

    fn parse_models_response(
        &self,
        json: serde_json::Value,
        provider: &str,
    ) -> Result<Vec<ModelMetadata>, DiscoveryError> {
        let mut models = Vec::new();

        let data = json
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DiscoveryError::InvalidResponse("Missing 'data' field".into()))?;

        for model_json in data {
            if let Ok(metadata) = self.parse_model_metadata(model_json, provider) {
                models.push(metadata);
            }
        }

        Ok(models)
    }

    fn parse_model_metadata(
        &self,
        json: &serde_json::Value,
        provider: &str,
    ) -> Result<ModelMetadata, DiscoveryError> {
        let id = json
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DiscoveryError::InvalidResponse("Missing 'id' field".into()))?;

        let name = json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(id);

        let pricing = json.get("pricing").ok_or_else(|| {
            DiscoveryError::InvalidResponse("Missing 'pricing' field".into())
        })?;

        let input_cost = pricing
            .get("prompt")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let output_cost = pricing
            .get("completion")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let context_length = json
            .get("context_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(4096) as usize;

        Ok(ModelMetadata {
            id: id.to_string(),
            name: id.to_string(),
            display_name: name.to_string(),
            provider: provider.to_string(),
            version: None,
            input_cost_per_million: input_cost,
            output_cost_per_million: output_cost,
            cached_cost_per_million: None,
            context_limit: context_length,
            max_output_tokens: json
                .get("max_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize),
            supports_streaming: json
                .get("supports_streaming")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            supports_function_calling: json
                .get("supports_function_calling")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            supports_vision: json
                .get("supports_vision")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            supports_audio_input: json
                .get("supports_audio_input")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            supports_audio_output: json
                .get("supports_audio_output")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            temperature_range: None,
            top_p_range: None,
            top_k_range: None,
            default_temperature: None,
            default_max_tokens: None,
            quality_score: json
                .get("quality_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5),
            speed_score: json
                .get("speed_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5),
            latency_ms: None,
            tokens_per_second: None,
            provider_metadata: HashMap::new(),
            model_type: ModelType::Chat,
            tier: ModelTier::Production,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("Request error: {0}")]
    RequestError(String),

    #[error("HTTP {0}: {1}")]
    HttpError(u16, String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}
```

### 4. Cost Tracking System

```rust
// crates/rustycode-llm/src/cost_tracking/tracker.rs

use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cached_tokens: usize,
    pub total_tokens: usize,
}

impl TokenUsage {
    pub fn new(input: usize, output: usize, cached: usize) -> Self {
        let total = input + output + cached;
        Self {
            input_tokens: input,
            output_tokens: output,
            cached_tokens: cached,
            total_tokens: total,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestCost {
    pub model_id: String,
    pub usage: TokenUsage,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cached_cost: f64,
    pub total_cost: f64,
    pub timestamp: std::time::SystemTime,
    pub request_id: String,
}

pub struct CostTracker {
    requests: Arc<RwLock<Vec<RequestCost>>>,
    total_cost: Arc<RwLock<f64>>,
    by_model: Arc<RwLock<HashMap<String, f64>>>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(Vec::new())),
            total_cost: Arc::new(RwLock::new(0.0)),
            by_model: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn track_request(&self, cost: RequestCost) {
        let mut requests = self.requests.write().await;
        let mut total_cost = self.total_cost.write().await;
        let mut by_model = self.by_model.write().await;

        requests.push(cost.clone());
        *total_cost += cost.total_cost;

        *by_model
            .entry(cost.model_id.clone())
            .or_insert(0.0) += cost.total_cost;
    }

    pub async fn get_total_cost(&self) -> f64 {
        *self.total_cost.read().await
    }

    pub async fn get_cost_by_model(&self, model_id: &str) -> f64 {
        let by_model = self.by_model.read().await;
        *by_model.get(model_id).unwrap_or(&0.0)
    }

    pub async fn generate_cost_report(&self) -> CostReport {
        let requests = self.requests.read().await;
        let total_cost = *self.total_cost.read().await;
        let by_model = self.by_model.read().await;

        let mut by_model_sorted: Vec<_> = by_model
            .iter()
            .map(|(model, cost)| (model.clone(), *cost))
            .collect();

        by_model_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        CostReport {
            total_cost,
            by_model: by_model_sorted,
            request_count: requests.len(),
            requests: requests.clone(),
        }
    }

    pub async fn reset(&self) {
        self.requests.write().await.clear();
        *self.total_cost.write().await = 0.0;
        self.by_model.write().await.clear();
    }
}

#[derive(Debug)]
pub struct CostReport {
    pub total_cost: f64,
    pub by_model: Vec<(String, f64)>,
    pub request_count: usize,
    pub requests: Vec<RequestCost>,
}

impl CostReport {
    pub fn format_summary(&self) -> String {
        let mut summary = String::new();

        summary.push_str("## Cost Summary\n\n");
        summary.push_str(&format!("Total Cost: ${:.2}\n", self.total_cost));
        summary.push_str(&format!("Total Requests: {}\n\n", self.request_count));

        summary.push_str("### Cost by Model\n\n");
        for (model, cost) in &self.by_model {
            summary.push_str(&format!("- {}: ${:.2}\n", model, cost));
        }

        summary
    }
}
```

## Usage Examples

### Basic Bootstrap

```rust
use rustycode_llm::bootstrap::ProviderBootstrap;
use rustycode_llm::models::ModelRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bootstrap = ProviderBootstrap::new();

    let config = BootstrapConfig {
        providers: vec![
            ProviderBootstrapConfig {
                id: "anthropic".into(),
                provider_type: "anthropic".into(),
                enabled: true,
                priority: 0,
                config: ProviderConfig {
                    api_key: Some(std::env::var("ANTHROPIC_API_KEY")?.into()),
                    ..Default::default()
                },
            }
        ],
        models: vec![],
        custom_loaders: vec![],
        discovery: DiscoveryConfig {
            enabled: false,
            endpoints: vec![],
            cache_ttl_secs: 3600,
            timeout_secs: 10,
        },
    };

    bootstrap.bootstrap(&config).await?;

    let registry = bootstrap.registry();

    // Find best model
    let criteria = ModelSelectionCriteria {
        provider: Some("anthropic".into()),
        required_capabilities: Some(vec![Capability::Streaming, Capability::Vision]),
        max_cost_per_million: Some(5.0),
        min_context_limit: Some(100_000),
        model_type: Some(ModelType::Chat),
        tier: Some(ModelTier::Production),
        sort_by: SortBy::Quality,
    };

    let model = registry.find_best_model(&criteria)
        .expect("No model found");

    println!("Selected model: {} (${}/1M tokens)",
        model.display_name,
        model.input_cost_per_million
    );

    Ok(())
}
```

### Dynamic Discovery

```rust
use rustycode_llm::discovery::ModelDiscoveryService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut discovery = ModelDiscoveryService::new();

    let endpoint = DiscoveryEndpoint {
        id: "openrouter".into(),
        url: "https://openrouter.ai/api/v1".into(),
        provider: "openrouter".into(),
        auth_env_var: "OPENROUTER_API_KEY".into(),
    };

    let models = discovery.discover_models(&endpoint).await?;

    println!("Discovered {} models:", models.len());
    for model in &models {
        println!("  - {} (${:.2}/1M input)",
            model.display_name,
            model.input_cost_per_million
        );
    }

    Ok(())
}
```

### Cost Tracking

```rust
use rustycode_llm::cost_tracking::{CostTracker, RequestCost, TokenUsage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracker = CostTracker::new();

    // Track a request
    tracker.track_request(RequestCost {
        model_id: "claude-3-5-sonnet-20250514".into(),
        usage: TokenUsage::new(1000, 500, 0),
        input_cost: 0.003,
        output_cost: 0.0075,
        cached_cost: 0.0,
        total_cost: 0.0105,
        timestamp: std::time::SystemTime::now(),
        request_id: "req-123".into(),
    }).await;

    // Generate report
    let report = tracker.generate_cost_report().await;

    println!("{}", report.format_summary());

    Ok(())
}
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_registry_initialization() {
        let registry = ModelRegistry::new();

        let claude = registry.get_model("claude-3-5-sonnet-20250514");
        assert!(claude.is_some());

        let model = claude.unwrap();
        assert_eq!(model.provider, "anthropic");
        assert_eq!(model.supports_streaming, true);
    }

    #[test]
    fn test_model_selection_criteria() {
        let registry = ModelRegistry::new();

        let criteria = ModelSelectionCriteria {
            provider: Some("anthropic".into()),
            required_capabilities: Some(vec![Capability::Streaming]),
            max_cost_per_million: Some(5.0),
            min_context_limit: Some(100_000),
            model_type: Some(ModelType::Chat),
            tier: Some(ModelTier::Production),
            sort_by: SortBy::Quality,
        };

        let model = registry.find_best_model(&criteria);
        assert!(model.is_some());
    }

    #[test]
    fn test_cost_estimation() {
        let registry = ModelRegistry::new();
        let model = registry.get_model("claude-3-5-sonnet-20250514").unwrap();

        let cost = model.estimate_cost(1000, 500, false);

        assert_eq!(cost.input_cost, 0.003); // 1000 / 1M * 3.0
        assert_eq!(cost.output_cost, 0.0075); // 500 / 1M * 15.0
        assert_eq!(cost.total_cost, 0.0105);
    }

    #[test]
    fn test_cost_tracking() {
        let tracker = CostTracker::new();

        tracker.track_request(RequestCost {
            model_id: "test-model".into(),
            usage: TokenUsage::new(100, 50, 0),
            input_cost: 0.001,
            output_cost: 0.002,
            cached_cost: 0.0,
            total_cost: 0.003,
            timestamp: std::time::SystemTime::now(),
            request_id: "req-1".into(),
        });

        // Block on async
        futures::executor::block_on(async {
            let total = tracker.get_total_cost().await;
            assert_eq!(total, 0.003);
        });
    }
}
```

## Performance Considerations

1. **Model Registry**: O(1) lookups, O(n) filtering
2. **Bootstrap**: O(p) where p = number of providers
3. **Discovery**: O(n) where n = number of models, cached for 1 hour
4. **Cost Tracking**: O(1) tracking, O(n) reporting

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.11", features = ["json"] }
thiserror = "2"
async-trait = "0.1"

[dev-dependencies]
tokio-test = "0.4"
```
