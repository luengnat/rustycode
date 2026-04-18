//! # rustycode-llm
//!
//! Multi-provider LLM client library supporting 13+ AI providers.
//!
//! ## Overview
//!
//! This library provides a unified interface for interacting with multiple
//! LLM providers including Anthropic, OpenAI, Azure OpenAI, Google Gemini,
//! and more. It handles authentication, streaming responses, error recovery,
//! rate limiting, and retry logic automatically.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use rustycode_llm::{OpenAiProvider, ProviderConfig, CompletionRequest, ChatMessage, LLMProvider};
//! use secrecy::SecretString;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = ProviderConfig {
//!         api_key: Some(SecretString::new(std::env::var("OPENAI_API_KEY")?.into())),
//!         base_url: None,
//!         timeout_seconds: Some(120),
//!         extra_headers: None,
//!         retry_config: None,
//!     };
//!
//!     let provider = OpenAiProvider::new(config, "gpt-4".to_string())?;
//!     let request = CompletionRequest::new(
//!         "gpt-4".to_string(),
//!         vec![ChatMessage::user("Hello, world!".to_string())],
//!     );
//!
//!     let response = LLMProvider::complete(&provider, request).await?;
//!     println!("{}", response.content);
//!     Ok(())
//! }
//! ```
//!
//! ## Supported Providers
//!
//! - **OpenAI** - GPT-3.5, GPT-4, GPT-4o
//! - **Anthropic** - Claude 3 Opus, Sonnet, Haiku
//! - **Azure OpenAI** - Hosted GPT models on Azure
//! - **Google Gemini** - Gemini 2.5 Pro, 2.0 Flash
//! - **Together AI** - Open-source models (Llama, Mixtral, etc.)
//! - **Cohere** - Command R, Command R+
//! - **Mistral AI** - Mistral Large, Mixtral 8x7B
//! - **Perplexity** - PPLX models with search
//! - **HuggingFace** - Inference API
//! - **Ollama** - Local models
//! - **GitHub Copilot** - Copilot-specific models
//! - **AWS Bedrock** - Amazon Bedrock models
//!
//! ## Streaming
//!
//! All providers support streaming responses:
//!
//! ```rust
//! # use rustycode_llm::{LLMProvider, OpenAiProvider, ProviderConfig, CompletionRequest, ChatMessage};
//! # use secrecy::SecretString;
//! # use futures::StreamExt;
//! # async fn example() -> anyhow::Result<()> {
//! # let config = ProviderConfig {
//! #     api_key: Some(SecretString::new("test-key".to_string().into())),
//! #     base_url: None,
//! #     timeout_seconds: Some(120),
//! #     extra_headers: None,
//! #     retry_config: None,
//! # };
//! # let provider = OpenAiProvider::new(config, "gpt-4".to_string()).unwrap();
//! # let request = CompletionRequest::new(
//! #     "gpt-4".to_string(),
//! #     vec![ChatMessage::user("Hello, world!".to_string())],
//! # );
//! let mut stream = LLMProvider::complete_stream(&provider, request).await?;
//! while let Some(chunk) = stream.next().await {
//!     print!("{:?}", chunk?);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Error Handling
//!
//! The library provides comprehensive error handling:
//!
//! ```rust
//! # use rustycode_llm::{LLMProvider, OpenAiProvider, ProviderConfig, CompletionRequest, ChatMessage, ProviderError};
//! # use secrecy::SecretString;
//! # async fn example() {
//! # let config = ProviderConfig {
//! #     api_key: Some(SecretString::new("test-key".to_string().into())),
//! #     base_url: None,
//! #     timeout_seconds: Some(120),
//! #     extra_headers: None,
//! #     retry_config: None,
//! # };
//! # let provider = OpenAiProvider::new(config, "gpt-4".to_string()).unwrap();
//! # let request = CompletionRequest::new(
//! #     "gpt-4".to_string(),
//! #     vec![ChatMessage::user("Hello, world!".to_string())],
//! # );
//! match LLMProvider::complete(&provider, request).await {
//!     Ok(response) => println!("{}", response.content),
//!     Err(ProviderError::Auth(msg)) => eprintln!("Authentication failed: {}", msg),
//!     Err(ProviderError::RateLimited { retry_delay: None }) => eprintln!("Rate limited - retry later"),
//!     Err(ProviderError::Network(msg)) => eprintln!("Network error: {}", msg),
//!     Err(e) => eprintln!("Other error: {}", e),
//! }
//! # }
//! ```
//!
//! ## Configuration
//!
//! ### ProviderConfig
//!
//! Common configuration for all providers:
//!
//! - `api_key` - API key (use `SecretString` for security)
//! - `base_url` - Custom API endpoint (optional)
//! - `timeout_seconds` - Request timeout (default: 120s)
//! - `extra_headers` - Additional HTTP headers (optional)
//!
//! ### Environment Variables
//!
//! Each provider respects its own environment variable:
//! - `OPENAI_API_KEY`
//! - `ANTHROPIC_API_KEY`
//! - `AZURE_OPENAI_API_KEY`
//! - `GOOGLE_API_KEY`
//! - `TOGETHER_API_KEY`
//! - `COHERE_API_KEY`
//! - `MISTRAL_API_KEY`
//! - `PERPLEXITY_API_KEY`
//! - `HF_TOKEN` / `HUGGINGFACE_API_KEY`
//! - `GITHUB_TOKEN` (for Copilot)
//!
//! ## Features
//!
//! - **Unified API** - Single interface for all providers
//! - **Streaming** - Real-time text generation
//! - **Auto-retry** - Configurable retry logic
//! - **Rate limiting** - Built-in rate limit handling
//! - **Error recovery** - Automatic error classification
//! - **Token tracking** - Usage statistics
//! - **Tool calling** - Function calling support (select providers)
//!
//! ## Architecture
//!
//! The library is organized into:
//!
//! - **Providers** - Individual provider implementations
//! - **provider_v2** - Modern provider trait and types
//! - **retry** - Exponential backoff retry logic
//! - **rate_limiter** - Token bucket rate limiting
//! - **error_recovery** - Error classification and recovery
//! - **tools** - Tool/function calling support
//! - **client_pool** - HTTP connection pooling
//!
//! ## Migration Notes
//!
//! This library has completed migration to the `provider_v2` API.
//! Legacy `ProviderConfig` has been replaced with `provider_v2::ProviderConfig`
//! which uses `SecretString` for secure API key handling.
//!
//! ### Breaking Changes from v1
//!
//! - `api_key` is now `Option<SecretString>` instead of `Option<String>`
//! - Removed `from_legacy_config()` methods
//! - Removed `create_provider_from_config_struct()`
//!
//! ## Examples
//!
//! See the `examples/` directory for:
//! - Basic usage
//! - Streaming responses
//! - Error handling
//! - Custom configuration
//! - Multi-provider setups
//!
//! ## License
//!
//! MIT License - See LICENSE file for details.

pub mod advisor;
pub mod anthropic;
pub mod anthropic_streaming;
pub mod azure;
pub mod bedrock;
pub mod caching;
pub mod circuit_breaker;
pub mod client_pool;
pub mod cohere;
pub mod compaction;
pub mod conversation;
pub mod copilot;
pub mod cost_tracker;
pub mod degradation_status;
pub mod download_manager;
pub mod error_recovery;
pub mod gemini;
pub mod graceful_degradation;
pub mod huggingface;
#[cfg(feature = "litert")]
pub mod litert_lm;
pub mod mistral;
pub mod model_info;
pub mod model_router;
pub mod offline_mode;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod perplexity;
pub mod provider;
pub mod provider_error_policy;
pub mod provider_fallback;
pub mod provider_helpers;
pub mod provider_metadata;
pub mod provider_registry_v2;
pub mod provider_router;
pub mod provider_v2;
pub mod rate_limiter;
pub mod registry;
pub mod replay_provider;
pub mod retry;
pub mod singleton_provider;
pub mod timeout_handler;
pub mod together;
pub mod token_tracker;
pub mod tool_executor;
pub mod tool_selection_helper;
pub mod tools;
pub mod usage_estimator;
pub mod zhipu;

use anyhow::{Context, Result};
use secrecy::SecretString;

// Use shared config parsing utilities from rustycode-config
use rustycode_config::{api_key_env_name, default_model_for_provider};

pub use advisor::{AdvisorConfig, AdvisorResponse, AdvisorTool};
pub use anthropic::AnthropicProvider;
pub use azure::AzureProvider;
pub use bedrock::BedrockProvider;
pub use client_pool::{global_client, global_pool, ClientPool, ClientPoolConfig, PoolStats};
pub use cohere::CohereProvider;
pub use conversation::ConversationManager;
pub use copilot::CopilotProvider;
pub use degradation_status::{
    DegradationReport, OperationStatus, RecoveryGuidance, StatusIndicator,
};
pub use error_recovery::{
    classify_error, default_strategy, with_recovery, ErrorKind, RecoveryStrategy,
};
pub use gemini::GeminiProvider;
pub use graceful_degradation::{
    DegradationHandler, DegradationMetadata, DegradationMetadataBuilder, ErrorClassifier,
    ErrorKind as DegradationErrorKind, ErrorSeverity, PartialResult,
    RetryConfig as DegradationRetryConfig,
};
pub use huggingface::HuggingFaceProvider;
#[cfg(feature = "litert")]
pub use litert_lm::LiteRtLmProvider;
pub use mistral::MistralProvider;
pub use model_router::{
    ModelChoice, ModelRouter, Request, RouterConfig, SimpleRouter, TaskComplexity,
};
pub use offline_mode::{
    CodeMetadata, LocalCodeAnalysisResult, LocalCodeAnalyzer, LocalSearchEngine, OfflineMode,
    OfflineModeConfig, SearchResult, SearchStats, StaticToolDescriptions, SyntaxValidationResult,
};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use openrouter::OpenRouterProvider;
pub use perplexity::PerplexityProvider;
// Export provider_v2 types without V2 suffix (migration complete)
pub use provider_v2::{
    ChatMessage, CompletionRequest, CompletionResponse, ContentBlockType, ContentDelta,
    LLMProvider, MessageRole, ProviderConfig, ProviderError, SSEEvent, StreamChunk, Usage,
};

// Re-export provider_v2 macros (available as: shared_client!, build_request!, etc.)

// Legacy types for backward compatibility (deprecated - migrate to provider_v2 API)
pub use provider::{
    CompletionResponse as CompletionResponseLegacy, LLMProvider as LLMProviderLegacy,
    ProviderConfig as ProviderConfigLegacy, ProviderType,
};
pub use provider_error_policy::{retry_plan_for_error, user_facing_error_for, RetryPlan};
pub use rate_limiter::{RateLimitConfig, RateLimitType, RateLimiter, RateLimiterBuilder};
pub use registry::ProviderRegistryBuilder;
pub use retry::{is_retryable_error, retry_with_backoff, RetryConfig};
pub use together::TogetherProvider;
pub use token_tracker::{
    cost_per_million_tokens, cost_per_million_tokens_io, estimate_cost, ModelUsage, TokenTracker,
    TrackedRequest, UsageSummary,
};
pub use zhipu::ZhipuProvider;

// Tool execution integration
pub use singleton_provider::{initialize_provider, is_initialized, reset, SharedLLMProvider};
pub use tool_executor::{LLMToolExecutor, ParsedToolCall, ToolExecutionResult};
pub mod utils;

#[cfg(test)]
mod cross_provider_tests;
pub use utils::{
    chunk_text, estimate_tokens, extract_reasoning_effort, extract_xml, extract_xml_all,
    extract_xml_all_multiline, extract_xml_multiline, has_tag, is_reasoning_model, llm_call,
    parse_summary, strip_xml_tags, ReasoningEffort, Summary,
};

// Provider metadata system for dynamic configuration and prompt optimization
pub use provider_metadata::{
    get_metadata, ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat, ToolSchema,
};

// Provider registry for centralized provider/model management
pub use provider_registry_v2::{
    ModelInfo as ModelSpec, ModelTier, ProviderMetadata as ProviderMeta, ProviderRegistryV2,
    TaskModelConfig, TaskType,
};

// Model info and capability metadata exports
pub use model_info::{
    is_reasoning_model_name, KnownModels, ModelCapabilities, DEFAULT_CONTEXT_LIMIT,
};

// Provider helpers for convenient access to registry functions
pub use provider_helpers::{
    find_model_provider, find_provider, get_cheapest_model, get_context_window, get_default_model,
    get_model_cost, get_models_by_tier, get_provider_info_json, get_registry, is_model_available,
    is_provider_available, list_models, list_provider_models, list_providers, select_model,
    select_model_with_config,
};

// Circuit breaker for managing endpoint health and cascading failure prevention
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerRegistry, CircuitBreakerStatus,
    CircuitState,
};

// Timeout handling for LLM operations and tool execution
pub use timeout_handler::{
    ModelTimeoutPreset, TimeoutConfig, TimeoutEvent, TimeoutHandler, TimeoutStats, TimeoutTracker,
};

// Multi-provider router for intelligent provider selection and failover
pub use provider_router::{
    default_router, ProviderConfig as RouterProviderConfig, ProviderRouter, RoutingDecision,
    RoutingRequirements, RoutingStrategy,
};

pub mod mock;
pub use mock::MockProvider;

const DEFAULT_CONTEXT_SIZE: usize = 8192;
const DEFAULT_MAX_TOKENS: u32 = 8192;
const DEFAULT_TEMPERATURE: f32 = 0.1;

fn apply_preset_or_default(mut config: ProviderConfigLegacy) -> ProviderConfigLegacy {
    if let Ok(preset) = std::env::var("RUSTYCODE_PROVIDER_PRESET") {
        return match preset.to_lowercase().as_str() {
            "precision" => config.precision_preset(),
            "creative" => config.creative_preset(),
            _ => config,
        };
    }

    config.temperature = DEFAULT_TEMPERATURE;
    config
}

fn apply_model_policy(mut config: ProviderConfigLegacy) -> ProviderConfigLegacy {
    let model = config.model.to_lowercase();

    // Reasoning models use deterministic decoding
    if model.contains("o1") || model.contains("o3") || model.contains("reasoning") {
        config.temperature = 0.0;
    }

    // Provider-specific tuning based on model characteristics
    if model.contains("claude") {
        config.temperature = config.temperature.min(0.2);
        config.max_tokens = Some(config.max_tokens.unwrap_or(4096).max(4096));
    } else if model.contains("gemini") {
        config.temperature = config.temperature.clamp(0.1, 0.3);
        if model.contains("flash") {
            config.max_tokens = Some(config.max_tokens.unwrap_or(4096).min(4096));
        }
    } else if model.contains("gpt") || model.contains("o4") {
        config.temperature = config.temperature.min(0.2);
    } else if model.contains("qwen") {
        config.temperature = config.temperature.clamp(0.2, 0.6);
    } else if model.contains("llama") || model.contains("mistral") {
        config.temperature = config.temperature.min(0.25);
    }

    config
}

fn load_provider_config() -> Result<(String, ProviderConfigLegacy)> {
    // Check for runtime provider override first
    let provider_override = std::env::var("RUSTYCODE_PROVIDER_OVERRIDE")
        .ok()
        .filter(|s| !s.trim().is_empty());

    // Try JSON config first: ~/.rustycode/config.json
    let json_config_path = dirs::home_dir().map(|p| p.join(".rustycode").join("config.json"));

    if let Some(config_path) = json_config_path {
        if config_path.exists() {
            tracing::debug!("Loading config from: {}", config_path.display());
            let contents =
                std::fs::read_to_string(&config_path).context("failed to read config.json")?;

            // Parse as JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                // Get provider type: runtime override > config > default
                let provider_type = provider_override
                    .as_deref()
                    .unwrap_or_else(|| {
                        json.get("provider")
                            .and_then(|v| v.as_str())
                            .unwrap_or("anthropic")
                    })
                    .to_string();

                // Get model: env override > config > default
                let model = std::env::var("RUSTYCODE_MODEL_OVERRIDE")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| {
                        json.get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&default_model_for_provider(&provider_type))
                            .to_string()
                    });

                // Try API key from multiple locations in JSON:
                // 1. providers.PROVIDER.api_key (new format)
                // 2. api_key (old format at root)
                // 3. Environment variable (only if non-empty)
                let api_key = json
                    .get("providers")
                    .and_then(|p| p.get(&provider_type))
                    .and_then(|p| p.get("api_key"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .or_else(|| {
                        json.get("api_key")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                    })
                    .or_else(|| {
                        std::env::var(api_key_env_name(&provider_type))
                            .ok()
                            .filter(|s| !s.is_empty())
                    });

                // Try base_url from multiple locations in JSON:
                // 1. providers.PROVIDER.base_url (new format)
                // 2. base_url (old format at root)
                let base_url = json
                    .get("providers")
                    .and_then(|p| p.get(&provider_type))
                    .and_then(|p| p.get("base_url"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .or_else(|| {
                        json.get("base_url")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                    });

                let config = ProviderConfigLegacy {
                    provider_type: parse_provider_type(&provider_type)?,
                    model,
                    temperature: json
                        .get("temperature")
                        .and_then(|v| v.as_f64())
                        .map(|v| v as f32)
                        .unwrap_or(DEFAULT_TEMPERATURE),
                    context_size: DEFAULT_CONTEXT_SIZE,
                    max_tokens: json
                        .get("max_tokens")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .or(Some(DEFAULT_MAX_TOKENS)),
                    api_key,
                    endpoint: base_url,
                    system_prompt: json
                        .get("system_prompt")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    custom_headers: None,
                    models: None,
                };

                let config = apply_preset_or_default(config);
                return Ok((provider_type, apply_model_policy(config)));
            }
        }
    }

    // Fall back to environment variables
    let provider_type = provider_override.unwrap_or_else(|| {
        std::env::var("RUSTYCODE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string())
    });
    let api_key = std::env::var(api_key_env_name(&provider_type)).ok();
    let base_url = std::env::var(format!("{}_BASE_URL", provider_type.to_uppercase()))
        .ok()
        .filter(|s| !s.is_empty());

    let config = ProviderConfigLegacy {
        provider_type: parse_provider_type(&provider_type)?,
        model: std::env::var("RUSTYCODE_MODEL_OVERRIDE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_model_for_provider(&provider_type)),
        temperature: DEFAULT_TEMPERATURE,
        context_size: DEFAULT_CONTEXT_SIZE,
        max_tokens: Some(DEFAULT_MAX_TOKENS),
        api_key,
        endpoint: base_url,
        system_prompt: None,
        custom_headers: None,
        models: None,
    };

    let config = apply_preset_or_default(config);
    Ok((provider_type, apply_model_policy(config)))
}

/// Create an LLM provider using the provider_v2 API
/// This is the recommended factory function for new code
pub fn create_provider_v2(
    provider_type: &str,
    model: &str,
) -> Result<std::sync::Arc<dyn LLMProvider>> {
    create_provider_with_config(provider_type, model, ProviderConfig::default())
}

/// Create an LLM provider with a specific config (including API key)
pub fn create_provider_with_config(
    provider_type: &str,
    model: &str,
    config: ProviderConfig,
) -> Result<std::sync::Arc<dyn LLMProvider>> {
    // Use provided config, but fall back to env var for API key if not set
    let v2_config = if config.api_key.is_some() {
        config
    } else {
        ProviderConfig {
            api_key: std::env::var(api_key_env_name(provider_type))
                .ok()
                .map(|s| SecretString::new(s.into())),
            ..config
        }
    };

    // Try strict constructors first. If they fail with a configuration error and
    // an API key or base_url was supplied (commonly coming from legacy file configs),
    // try the non-strict `new_without_validation` constructor where available. This
    // helps migration from legacy provider configs where key formats may differ
    // or validation should be deferred.
    let has_credentials = v2_config.api_key.is_some() || v2_config.base_url.is_some();
    let provider: std::sync::Arc<dyn LLMProvider> = match provider_type.to_lowercase().as_str() {
        "openai" | "open_ai" => match OpenAiProvider::new(v2_config.clone(), model.to_string()) {
            Ok(p) => std::sync::Arc::new(p),
            Err(e) => {
                if has_credentials && matches!(e, ProviderError::Configuration(_)) {
                    std::sync::Arc::new(OpenAiProvider::new_without_validation(
                        v2_config.clone(),
                        model.to_string(),
                    )?)
                } else {
                    return Err(e.into());
                }
            }
        },
        "anthropic" => match AnthropicProvider::new(v2_config.clone(), model.to_string()) {
            Ok(p) => std::sync::Arc::new(p),
            Err(e) => {
                if has_credentials && matches!(e, ProviderError::Configuration(_)) {
                    std::sync::Arc::new(AnthropicProvider::new_without_validation(
                        v2_config.clone(),
                        model.to_string(),
                    )?)
                } else {
                    return Err(e.into());
                }
            }
        },
        "ollama" => std::sync::Arc::new(OllamaProvider::new(v2_config)?),
        "gemini" | "google" => std::sync::Arc::new(GeminiProvider::new(v2_config)?),
        "copilot" | "github" => std::sync::Arc::new(CopilotProvider::new(v2_config)?),
        "bedrock" | "aws" => {
            std::sync::Arc::new(BedrockProvider::new(v2_config, model.to_string())?)
        }
        "azure" | "azure_openai" | "microsoft" => {
            std::sync::Arc::new(AzureProvider::new(v2_config)?)
        }
        "cohere" => std::sync::Arc::new(CohereProvider::new(v2_config)?),
        "mistral" | "mistral_ai" => {
            std::sync::Arc::new(MistralProvider::new(v2_config, model.to_string())?)
        }
        "together" | "together_ai" => std::sync::Arc::new(TogetherProvider::new(v2_config)?),
        "perplexity" | "pplx" => {
            std::sync::Arc::new(PerplexityProvider::new(v2_config, model.to_string())?)
        }
        "huggingface" | "hf" => {
            std::sync::Arc::new(HuggingFaceProvider::new(v2_config, model.to_string())?)
        }
        "openrouter" => match OpenRouterProvider::new(v2_config.clone(), model.to_string()) {
            Ok(p) => std::sync::Arc::new(p),
            Err(e) => {
                if has_credentials && matches!(e, ProviderError::Configuration(_)) {
                    std::sync::Arc::new(OpenRouterProvider::new_without_validation(
                        v2_config.clone(),
                        model.to_string(),
                    )?)
                } else {
                    return Err(e.into());
                }
            }
        },
        // Litert Lightweight LiteRtLmProvider wiring
        #[cfg(feature = "litert")]
        "litert-lm" | "litert_lm" | "litert" => {
            let provider = LiteRtLmProvider::new(v2_config, model.to_string())?;
            let boxed: Box<dyn LLMProvider> = Box::new(provider);
            std::sync::Arc::<dyn LLMProvider>::from(boxed)
        }
        #[cfg(not(feature = "litert"))]
        "litert-lm" | "litert_lm" | "litert" => {
            return Err(ProviderError::Configuration(
                "LiteRT-LM provider requires the 'litert' feature flag (needs C++ toolchain)"
                    .into(),
            )
            .into());
        }
        _ => match AnthropicProvider::new(v2_config.clone(), model.to_string()) {
            Ok(p) => std::sync::Arc::new(p),
            Err(e) => {
                if has_credentials && matches!(e, ProviderError::Configuration(_)) {
                    std::sync::Arc::new(AnthropicProvider::new_without_validation(
                        v2_config.clone(),
                        model.to_string(),
                    )?)
                } else {
                    return Err(e.into());
                }
            }
        },
    };

    Ok(provider)
}

/// Load just the model name from config (for use in streaming)
///
/// Resolution order:
/// 1. `RUSTYCODE_MODEL_OVERRIDE` environment variable (runtime override)
/// 2. `model` field from config file / provider defaults
pub fn load_model_from_config() -> Result<String> {
    if let Ok(model) = std::env::var("RUSTYCODE_MODEL_OVERRIDE") {
        if !model.trim().is_empty() {
            return Ok(model);
        }
    }

    // Try multiple config file locations (JSON only)
    let config_paths = vec![
        // Standard config location
        dirs::home_dir().map(|p| p.join(".rustycode").join("config.json")),
        // Workspace config
        std::env::current_dir()
            .ok()
            .map(|d| d.join(".rustycode").join("config.json")),
        // Fallback
        Some(std::path::PathBuf::from(".rustycode/config.json")),
    ];

    for config_path in config_paths {
        let config_path = match config_path {
            Some(p) => p,
            None => continue,
        };

        if config_path.exists() {
            let contents =
                std::fs::read_to_string(&config_path).context("failed to read config file")?;

            // Parse JSON and extract model field
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(model) = json.get("model").and_then(|v| v.as_str()) {
                    return Ok(model.to_string());
                }
            }
        }
    }

    // Fallback to full config loading
    let (_provider_type, config) = load_provider_config()?;
    Ok(config.model)
}

/// Load provider type from config file
///
/// This is a public wrapper around the private load_provider_config function
pub fn load_provider_type_from_config() -> Result<String> {
    let (provider_type, _config) = load_provider_config()?;
    Ok(provider_type)
}

/// Load provider config from environment (public wrapper)
pub fn load_provider_config_from_env() -> Result<(String, String, provider_v2::ProviderConfig)> {
    let (provider_type, config) = load_provider_config()?;
    let model = config.model.clone();

    // Convert legacy config to v2 config
    let v2_config = provider_v2::ProviderConfig {
        api_key: config.api_key.map(|s| SecretString::new(s.into())),
        base_url: config.endpoint,
        timeout_seconds: Some(120),
        extra_headers: config.custom_headers,
        retry_config: None, // Use default if None
    };

    Ok((provider_type, model, v2_config))
}

fn parse_provider_type(s: &str) -> Result<provider::ProviderType> {
    match s.to_lowercase().as_str() {
        "openai" | "open_ai" => Ok(provider::ProviderType::OpenAI),
        "anthropic" => Ok(provider::ProviderType::Anthropic),
        "ollama" => Ok(provider::ProviderType::Ollama),
        "gemini" | "google" => Ok(provider::ProviderType::Gemini),
        "copilot" | "github" => Ok(provider::ProviderType::Copilot),
        "bedrock" | "aws" => Ok(provider::ProviderType::Bedrock),
        "azure" | "azure_openai" | "microsoft" => Ok(provider::ProviderType::Azure),
        "cohere" => Ok(provider::ProviderType::Cohere),
        "mistral" | "mistral_ai" => Ok(provider::ProviderType::Mistral),
        "together" | "together_ai" => Ok(provider::ProviderType::Together),
        "perplexity" | "pplx" => Ok(provider::ProviderType::Perplexity),
        "huggingface" | "hf" => Ok(provider::ProviderType::HuggingFace),
        "openrouter" => Ok(provider::ProviderType::OpenAI), // Treat as OpenAI-compatible
        _ => Ok(provider::ProviderType::Anthropic),
    }
}
