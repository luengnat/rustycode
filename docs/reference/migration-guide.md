# Migration Guide: Legacy → Provider v2 API

## Overview

This guide helps you migrate from the legacy `provider` API to the modern `provider_v2` API.

### What Changed?

| Aspect | Legacy (v1) | Modern (v2) |
|--------|-------------|--------------|
| API Key Type | `Option<String>` | `Option<SecretString>` |
| Config Creation | `from_legacy_config()` | Direct struct construction |
| Provider Trait | `LegacyLLMProvider` | `LLMProvider` |
| Error Type | `anyhow::Error` | `ProviderError` |
| Secret Handling | Plain strings | `SecretString` (zeroized) |

---

## Quick Migration

### Before (Legacy)
```rust
use rustycode_llm::{OpenAiProvider, ProviderConfigLegacy};

let legacy_config = ProviderConfigLegacy {
    provider_type: "openai".to_string(),
    model: "gpt-4".to_string(),
    api_key: Some("sk-...".to_string()),
    endpoint: Some("https://api.openai.com/v1".to_string()),
    // ...
};

let provider = OpenAiProvider::from_legacy_config(legacy_config)?;
```

### After (Modern)
```rust
use rustycode_llm::{OpenAiProvider, ProviderConfig};
use secrecy::SecretString;

let config = ProviderConfig {
    api_key: Some(SecretString::new("sk-...".to_string())),
    base_url: Some("https://api.openai.com/v1".to_string()),
    timeout_seconds: Some(120),
    extra_headers: None,
};

let provider = OpenAiProvider::new(config);
```

---

## Step-by-Step Migration

### Step 1: Update Imports

**Before:**
```rust
use rustycode_llm::{
    OpenAiProvider,
    provider::ProviderConfig as LegacyProviderConfig
};
```

**After:**
```rust
use rustycode_llm::{
    OpenAiProvider,
    ProviderConfig  // Now from provider_v2
};
use secrecy::SecretString;  // Add this
```

### Step 2: Convert API Keys

**Before:**
```rust
api_key: Some("my-key".to_string())
```

**After:**
```rust
api_key: Some(SecretString::new("my-key".to_string()))
```

### Step 3: Update Configuration Structure

**Before:**
```rust
ProviderConfigLegacy {
    provider_type: "openai".to_string(),
    model: "gpt-4".to_string(),
    api_key: Some(key),
    endpoint: Some(url),
    custom_headers: Some(headers),
}
```

**After:**
```rust
ProviderConfig {
    api_key: Some(SecretString::new(key)),
    base_url: Some(url),
    timeout_seconds: Some(120),
    extra_headers: Some(headers),
}
// model is now passed per-request
```

### Step 4: Update Provider Construction

**Before:**
```rust
let provider = OpenAiProvider::from_legacy_config(config)?;
```

**After:**
```rust
let provider = OpenAiProvider::new(config);
```

**Note**: Some providers return `Result`, others don't. Check signatures.

### Step 5: Update Request Creation

**Before:**
```rust
let request = CompletionRequest {
    model: "gpt-4".to_string(),
    messages: vec![/* ... */],
    max_tokens: Some(1000),
    temperature: Some(0.7),
};
```

**After:**
```rust
let request = CompletionRequest::new(
    "gpt-4".to_string(),
    vec![/* ... */],
)
.with_max_tokens(1000)
.with_temperature(0.7);
```

### Step 6: Update Error Handling

**Before:**
```rust
match provider.complete(request).await {
    Ok(response) => println!("{}", response.content),
    Err(e) => eprintln!("Error: {}", e),
}
```

**After:**
```rust
match provider.complete(request).await {
    Ok(response) => println!("{}", response.content),
    Err(ProviderError::Auth(msg)) => {
        eprintln!("Auth failed: {}", msg);
    }
    Err(ProviderError::RateLimited) => {
        eprintln!("Rate limited - retry later");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Common Migration Patterns

### Pattern 1: Environment Variable API Keys

**Before:**
```rust
let api_key = std::env::var("OPENAI_API_KEY")
    .unwrap_or_default();
```

**After:**
```rust
let api_key = std::env::var("OPENAI_API_KEY")
    .ok()
    .map(|k| SecretString::new(k));
```

### Pattern 2: Dynamic Provider Selection

**Before:**
```rust
let provider = match provider_type {
    "openai" => OpenAiProvider::from_legacy_config(config)?,
    "anthropic" => AnthropicProvider::from_legacy_config(config)?,
    // ...
};
```

**After:**
```rust
use rustycode_llm::create_provider_v2;

let provider = create_provider_v2("openai", "gpt-4")?;
```

### Pattern 3: SecretString Conversion

**When you need the actual string:**
```rust
// Option<SecretString> → Option<String>
let key_opt: Option<String> = config.api_key
    .as_ref()
    .map(|k| k.expose_secret().to_string());

// SecretString → String
let key: String = secret_key.expose_secret().to_string();
```

**Note**: Use `.to_string()`, not `.clone()`!
- `.expose_secret()` returns `&str`
- `.clone()` on `&str` returns `&str` again (type error!)
- `.to_string()` creates owned `String`

---

## Breaking Changes

### 1. Removed Methods

```rust
// ❌ No longer exists
from_legacy_config()
create_provider_from_config_struct()

// ✅ Use instead
Provider::new(ProviderConfig)
create_provider_v2()
```

### 2. Type Changes

```rust
// ❌ Old
api_key: Option<String>

// ✅ New
api_key: Option<SecretString>
```

### 3. Required Imports

```rust
// ✅ Add this
use secrecy::SecretString;
```

---

## Testing Your Migration

### 1. Compilation Check
```bash
cargo check --package rustycode-llm
```

### 2. Run Examples
```bash
cargo run --example basic_usage
cargo run --example streaming
cargo run --example error_handling
```

### 3. Test Your Code
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_works() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new("test-key".to_string())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
        };

        let provider = YourProvider::new(config);
        assert_eq!(provider.name(), "provider-name");
    }
}
```

---

## Troubleshooting

### Error: `expected Box<str>, found String`

**Problem**: Using `SecretString::new(s)` instead of `SecretString::new(s.into())`

**Solution**:
```rust
// ❌ Wrong
SecretString::new(my_string)

// ✅ Right
SecretString::new(my_string.into())
```

### Error: `expected Option<&str>, found Option<String>`

**Problem**: Calling `.as_deref()` incorrectly

**Solution**:
```rust
// ❌ Wrong
config_key.or(env_key.as_deref())

// ✅ Right
let config_key = /* Option<String> */;
let env_key = /* Option<String> */;
config_key.or(env_key)
```

### Error: `mismatched types: ()`

**Problem**: Some constructors now return `Result`, not direct instance

**Solution**:
```rust
// Check the signature
let provider = AnthropicProvider::new(config)?;  // Returns Result
let provider = OpenAiProvider::new(config);     // Returns instance directly
```

---

## Feature Parity

| Feature | Legacy | Modern | Notes |
|---------|--------|--------|-------|
| Basic completions | ✅ | ✅ | Same functionality |
| Streaming | ✅ | ✅ | Same functionality |
| Multiple providers | ✅ | ✅ | Same providers |
| Error recovery | ✅ | ✅ | Better error types |
| Rate limiting | ✅ | ✅ | Same behavior |
| Token tracking | ✅ | ✅ | Same behavior |
| Tool calling | ✅ | ✅ | Same behavior |
| Security | ⚠️ | ✅ | Better with SecretString |

---

## Advanced Topics

### Custom Headers

```rust
let mut headers = std::collections::HashMap::new();
headers.insert("X-Custom-Header".to_string(), "value".to_string());

let config = ProviderConfig {
    api_key: Some(SecretString::new(key)),
    base_url: Some(url.to_string()),
    timeout_seconds: Some(120),
    extra_headers: Some(headers),
};
```

### Timeout Configuration

```rust
let config = ProviderConfig {
    api_key: Some(SecretString::new(key)),
    base_url: None,
    timeout_seconds: Some(180),  // 3 minutes
    extra_headers: None,
};
```

### Base URL Override

```rust
let config = ProviderConfig {
    api_key: Some(SecretString::new(key)),
    base_url: Some("https://custom-proxy.com".to_string()),
    timeout_seconds: Some(120),
    extra_headers: None,
};
```

---

## Getting Help

### Documentation
- [Module Documentation](../src/lib.rs)
- [Provider Examples](../examples/)
- [API Reference](https://docs.rs/rustycode-llm)

### ADRs
- [Provider v2 API Design](./adr/001-provider-v2-api.md)
- [SecretString Integration](./adr/002-secretstring-integration.md)
- [Macro-Based Providers](./adr/003-macro-based-providers.md)

### Troubleshooting
- Check compilation errors
- Review examples
- Run tests: `cargo test --package rustycode-llm`

---

## Checklist

Use this checklist to ensure complete migration:

- [ ] Update imports (add `secrecy::SecretString`)
- [ ] Convert `api_key` to `SecretString`
- [ ] Update `ProviderConfig` structure
- [ ] Replace `from_legacy_config()` with `new()`
- [ ] Update request construction
- [ ] Update error handling
- [ ] Test compilation
- [ ] Run examples
- [ ] Verify functionality

---

## Need More Help?

- Open an issue on GitHub
- Check the examples directory
- Review provider source code for patterns
- Consult the ADRs for design rationale

---

**Last Updated**: 2024-03-14
**API Version**: v2 (provider_v2)
**Status**: Stable ✅
