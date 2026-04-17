# Provider Guide

## Supported Providers

RustyCode supports multiple LLM providers through a unified interface, enabling you to:
- Switch between providers without code changes
- Use different providers for different tasks
- Track costs and token usage across providers
- Leverage provider-specific capabilities

### Provider List

| Provider | ID | Models | Streaming | Vision | Function Calling |
|----------|----|----|-----------|--------|------------------|
| Anthropic | `anthropic` | Claude 3.5 Sonnet, Opus 4.6, Haiku 4.5 | ✅ | ✅ | ✅ |
| OpenAI | `openai` | GPT-4o, GPT-4 Turbo, GPT-3.5 | ✅ | ✅ | ✅ |
| OpenRouter | `openrouter` | Multi-provider | ✅ | ❌ | ✅ |
| Google Gemini | `gemini` | Gemini Pro, Ultra | ✅ | ✅ | ✅ |
| Ollama | `ollama` | Local models (varies) | ✅ | Varies | Varies |

## Provider Configuration

### Anthropic (Claude)

**Environment Variable**: `ANTHROPIC_API_KEY`

**Configuration**:
```json
{
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",
      "models": [
        "claude-3-5-sonnet-latest",
        "claude-opus-4-6",
        "claude-haiku-4-5"
      ]
    }
  }
}
```

**Models**:
- `claude-3-5-sonnet-latest`: Best balance of capability and speed
- `claude-opus-4-6`: Maximum reasoning capability
- `claude-haiku-4-5`: Fastest, most cost-effective

**Pricing** (as of 2025):
- Sonnet: $0.003/1K input, $0.015/1K output
- Opus: $0.015/1K input, $0.075/1K output
- Haiku: $0.0008/1K input, $0.004/1K output

### OpenAI (GPT)

**Environment Variable**: `OPENAI_API_KEY`

**Configuration**:
```json
{
  "providers": {
    "openai": {
      "api_key": "{env:OPENAI_API_KEY}",
      "base_url": "https://api.openai.com/v1",
      "models": ["gpt-4o", "gpt-4-turbo", "gpt-3.5-turbo"]
    }
  }
}
```

**Models**:
- `gpt-4o`: Latest GPT-4 model with vision
- `gpt-4-turbo`: Fast GPT-4 variant
- `gpt-3.5-turbo`: Most cost-effective

**Pricing** (as of 2025):
- GPT-4o: $0.005/1K input, $0.015/1K output
- GPT-4 Turbo: $0.01/1K input, $0.03/1K output
- GPT-3.5 Turbo: $0.0005/1K input, $0.0015/1K output

### OpenRouter (Multi-Provider)

**Environment Variable**: `OPENROUTER_API_KEY`

**Configuration**:
```json
{
  "providers": {
    "openrouter": {
      "api_key": "{env:OPENROUTER_API_KEY}",
      "base_url": "https://openrouter.ai/api/v1"
    }
  }
}
```

**Benefits**:
- Access to 100+ models via single API
- Unified pricing and billing
- Model routing and load balancing
- Fallback and redundancy

**Note**: Pricing varies by model used. Check OpenRouter for current rates.

### Google Gemini

**Environment Variable**: `GEMINI_API_KEY`

**Configuration**:
```json
{
  "providers": {
    "gemini": {
      "api_key": "{env:GEMINI_API_KEY}",
      "base_url": "https://generativelanguage.googleapis.com/v1beta",
      "models": ["gemini-pro", "gemini-ultra"]
    }
  }
}
```

**Models**:
- `gemini-pro`: Balanced performance
- `gemini-ultra`: Maximum capability (1M context)

**Pricing** (as of 2025):
- Gemini Pro: $0.001/1K input, $0.002/1K output
- Gemini Ultra: $0.0035/1K input, $0.0105/1K output

### Ollama (Local Models)

**Environment Variable**: `OLLAMA_BASE_URL` (optional, defaults to `http://localhost:11434`)

**Configuration**:
```json
{
  "providers": {
    "ollama": {
      "base_url": "http://localhost:11434",
      "models": ["llama2", "codellama", "mistral"]
    }
  }
}
```

**Models**: Varies based on what you have installed locally

**Benefits**:
- Free (runs locally)
- Privacy (data never leaves your machine)
- Custom models
- Offline operation

**Note**: Capabilities vary by model. Check individual model documentation.

## Usage Examples

### Auto-Discovery from Environment

```rust
use rustycode_providers::bootstrap_from_env;

#[tokio::main]
async fn main() {
    // Automatically discover providers from environment variables
    let registry = bootstrap_from_env().await;

    // List available providers
    for provider_id in registry.list_providers() {
        if let Some(provider) = registry.get_provider(&provider_id) {
            println!("{}: {}", provider.name, provider.description);
        }
    }

    // Get cost tracking
    let costs = registry.get_cost_summary();
    println!("Total cost: ${:.2}", costs.total_cost);
}
```

### Manual Provider Registration

```rust
use rustycode_providers::{ModelRegistry, ProviderMetadata, PricingInfo, Currency, ProviderCapabilities};
use rustycode_providers::pricing::PricingInfo;

#[tokio::main]
async fn main() {
    let registry = ModelRegistry::new();

    // Register custom provider
    let custom_provider = ProviderMetadata {
        id: "my-provider".to_string(),
        name: "My Custom Provider".to_string(),
        base_url: "https://api.myprovider.com".to_string(),
        api_key_env: "MY_PROVIDER_API_KEY".to_string(),
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: false,
            max_tokens: 4096,
            max_context_window: 128_000,
        },
        pricing: PricingInfo {
            input_cost_per_1k: 0.002,
            output_cost_per_1k: 0.008,
            currency: Currency::Usd,
        },
    };

    registry.register_provider(custom_provider).await;
}
```

### Using a Specific Provider

```rust
use rustycode_llm::{AnthropicProvider, ProviderConfig};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create provider config
    let config = ProviderConfig {
        api_key: Some(SecretString::new("sk-ant-...".to_string())),
        base_url: None,
        timeout_seconds: None,
        extra_headers: None,
    };

    // Create provider
    let provider = AnthropicProvider::new(
        config,
        "claude-3-5-sonnet-latest".to_string()
    )?;

    // Use provider for completion
    // ... (see rustycode-llm documentation)

    Ok(())
}
```

### Getting Provider Metadata

```rust
use rustycode_llm::get_metadata;

fn main() {
    // Get metadata for a provider
    if let Some(metadata) = get_metadata("anthropic") {
        println!("Provider: {}", metadata.name);
        println!("Base URL: {}", metadata.base_url);
        println!("Max Context: {}", metadata.max_context_window);
        println!("Streaming: {}", metadata.supports_streaming);
    }
}
```

## Cost Tracking

### Automatic Cost Calculation

RustyCode automatically tracks costs for all requests:

```rust
use rustycode_providers::{CostTracker, CostAccumulator};

#[tokio::main]
async fn main() {
    let mut tracker = CostTracker::new();

    // Simulate API calls
    tracker.track_request("anthropic", 1000, 500);  // 1K input, 500 output
    tracker.track_request("openai", 2000, 1000);     // 2K input, 1K output

    // Get cost summary
    let summary = tracker.get_summary();
    println!("Total cost: ${:.4}", summary.total_cost);
    println!("Anthropic: ${:.4}", summary.by_provider["anthropic"]);
    println!("OpenAI: ${:.4}", summary.by_provider["openai"]);
}
```

### Per-Request Cost Tracking

```rust
use rustycode_providers::ModelRegistry;

#[tokio::main]
async fn main() {
    let registry = bootstrap_from_env().await;

    // Make a request
    let (input_tokens, output_tokens) = (1000, 500);

    // Calculate cost
    if let Some(provider) = registry.get_provider("anthropic") {
        let cost = provider.pricing.calculate_cost(input_tokens, output_tokens);
        println!("Request cost: ${:.4}", cost);
    }
}
```

### Cost Summary by Provider

```rust
use rustycode_providers::{CostSummary, Currency};

fn print_cost_summary(summary: CostSummary) {
    println!("=== Cost Summary ===");
    println!("Total: ${:.2} {}", summary.total_cost, summary.currency);
    println!("\nBy Provider:");

    for (provider_id, cost) in summary.by_provider {
        println!("  {}: ${:.4}", provider_id, cost);
    }

    println!("\nBy Model:");
    for (model, cost) in summary.by_model {
        println!("  {}: ${:.4}", model, cost);
    }
}
```

## Extending Providers

### Adding a New Provider

To add support for a new LLM provider:

1. **Implement the LLMProvider trait**:
```rust
use rustycode_llm::LLMProvider;
use async_trait::async_trait;

pub struct MyProvider {
    config: ProviderConfig,
    model: String,
}

#[async_trait]
impl LLMProvider for MyProvider {
    fn name(&self) -> &str {
        "my-provider"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        // Implement completion logic
    }

    async fn complete_stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = Result<String, ProviderError>> + Send>>, ProviderError> {
        // Implement streaming logic
    }
}
```

2. **Register in provider factory**:
```rust
// In rustycode-runtime/src/agent.rs
ProviderType::MyProvider => Ok(Box::new(
    MyProvider::new(config).map_err(|e| ProviderError::Configuration(e.to_string()))?
)),
```

3. **Add provider metadata**:
```rust
// In rustycode-providers/src/lib.rs
pub fn my_provider() -> ProviderMetadata {
    ProviderMetadata {
        id: "my-provider".to_string(),
        name: "My Provider".to_string(),
        base_url: "https://api.myprovider.com".to_string(),
        api_key_env: "MY_PROVIDER_API_KEY".to_string(),
        capabilities: ProviderCapabilities {
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: false,
            max_tokens: 4096,
            max_context_window: 128_000,
        },
        pricing: PricingInfo {
            input_cost_per_1k: 0.002,
            output_cost_per_1k: 0.008,
            currency: Currency::Usd,
        },
    }
}
```

### Custom Models

Add custom models to existing providers:

```rust
use rustycode_providers::ModelRegistry;

#[tokio::main]
async fn main() {
    let registry = ModelRegistry::new();

    // Register custom model
    let provider = registry.get_provider("anthropic").unwrap();
    provider.add_model("claude-3-custom", 200_000, 8192);

    // Use custom model
    let model = provider.get_model("claude-3-custom").unwrap();
    println!("Model: {}, Context: {}", model.id, model.max_context_window);
}
```

### Provider Capabilities

Define what your provider supports:

```rust
use rustycode_providers::ProviderCapabilities;

let capabilities = ProviderCapabilities {
    supports_streaming: true,        // Does it support streaming?
    supports_function_calling: true, // Does it support tool calling?
    supports_vision: true,           // Does it support images?
    max_tokens: 8192,                // Max tokens per response
    max_context_window: 200_000,     // Max total context
};
```

## Best Practices

### Choosing the Right Provider

| Use Case | Recommended Provider | Model |
|----------|---------------------|-------|
| General coding | Anthropic | claude-3-5-sonnet-latest |
| Complex reasoning | Anthropic | claude-opus-4-6 |
| Fast responses | Anthropic | claude-haiku-4-5 |
| Vision tasks | OpenAI | gpt-4o |
| Large context | Google Gemini | gemini-ultra |
| Cost-sensitive | Anthropic | claude-haiku-4-5 |
| Privacy/local | Ollama | llama2 |
| Multi-model | OpenRouter | varies |

### API Key Security

1. **Use environment variables**:
   ```json
   {
     "api_key": "{env:ANTHROPIC_API_KEY}"
   }
   ```

2. **Never commit API keys**:
   ```bash
   # Add to .gitignore
   .env
   *.key
   ```

3. **Rotate keys regularly**:
   - Set expiration dates on keys
   - Use different keys for dev/prod
   - Monitor usage for anomalies

### Error Handling

```rust
use rustycode_llm::{ProviderError, LLMProvider};

async fn safe_completion(provider: &dyn LLMProvider, request: CompletionRequest) -> Result<String, String> {
    match provider.complete(request).await {
        Ok(response) => Ok(response.content),
        Err(ProviderError::RateLimited(_)) => {
            Err("Rate limited. Please wait.".to_string())
        }
        Err(ProviderError::InvalidAuth(_)) => {
            Err("Invalid API key. Check your credentials.".to_string())
        }
        Err(ProviderError::NetworkError(_)) => {
            Err("Network error. Please check your connection.".to_string())
        }
        Err(e) => {
            Err(format!("Provider error: {}", e))
        }
    }
}
```

### Performance Optimization

1. **Use streaming for long responses**:
```rust
let stream = provider.complete_stream(request).await?;
pin_mut!(stream);

while let Some(chunk) = stream.next().await {
    print!("{}", chunk?);
}
```

2. **Batch requests when possible**:
```rust
// Instead of multiple small requests
for item in items {
    let result = provider.complete(request).await?;
}

// Use one large request
let result = provider.complete(large_request).await?;
```

3. **Cache responses**:
```rust
use std::collections::HashMap;

let mut cache = HashMap::new();
let cache_key = format!("{:?}", request);

if !cache.contains_key(&cache_key) {
    let result = provider.complete(request).await?;
    cache.insert(cache_key, result);
}
```

## Troubleshooting

### Provider Not Found

**Problem**: Provider not available after bootstrap

**Solutions**:
1. Check environment variable:
   ```bash
   echo $ANTHROPIC_API_KEY
   ```

2. Verify API key format:
   ```bash
   # Should start with sk-ant-
   export ANTHROPIC_API_KEY=sk-ant-...
   ```

3. Check provider registration:
   ```rust
   let registry = bootstrap_from_env().await;
   println!("Providers: {:?}", registry.list_providers());
   ```

### Rate Limiting

**Problem**: Getting rate limit errors

**Solutions**:
1. Implement exponential backoff:
```rust
use tokio::time::{sleep, Duration};

async fn retry_with_backoff(provider: &dyn LLMProvider, request: CompletionRequest, max_retries: u32) -> Result<CompletionResponse, ProviderError> {
    let mut attempt = 0;
    let mut delay = Duration::from_millis(1000);

    loop {
        match provider.complete(request.clone()).await {
            Ok(response) => return Ok(response),
            Err(ProviderError::RateLimited(_)) if attempt < max_retries => {
                sleep(delay).await;
                delay *= 2;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

2. Use multiple providers:
```rust
let providers = vec!["anthropic", "openai", "openrouter"];

for provider_id in providers {
    if let Ok(response) = provider.complete(request.clone()).await {
        return Ok(response);
    }
}
```

### Incorrect Costs

**Problem**: Cost calculations don't match actual billing

**Solutions**:
1. Update pricing data:
```rust
let provider = registry.get_provider("anthropic").unwrap();
provider.pricing.input_cost_per_1k = 0.003;
provider.pricing.output_cost_per_1k = 0.015;
```

2. Verify token counting:
```rust
let actual_tokens = provider.count_tokens(&request.messages);
println!("Actual tokens: {}", actual_tokens);
```

3. Check provider documentation for current rates

## Conclusion

The RustyCode provider system is designed to be:
- **Flexible**: Support for multiple providers through unified interface
- **Transparent**: Clear cost tracking and usage monitoring
- **Extensible**: Easy to add new providers and models
- **Production-Ready**: Robust error handling and retry logic

For more information, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Configuration Guide](CONFIGURATION.md)
- [Agent System Guide](AGENTS.md)
