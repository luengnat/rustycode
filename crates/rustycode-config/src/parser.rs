/// Get the environment variable name for a provider's API key
pub fn api_key_env_name(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "openai" => "OPENAI_API_KEY".to_string(),
        "anthropic" => "ANTHROPIC_API_KEY".to_string(),
        "openrouter" => "OPENROUTER_API_KEY".to_string(),
        _ => format!("{}_API_KEY", provider.to_uppercase().replace('-', "_")),
    }
}

/// Get the default model for a provider
pub fn default_model_for_provider(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "openai" => "gpt-4o".to_string(),
        // Keep legacy mappings expected by backward-compat tests
        "anthropic" => "claude-sonnet-4-6".to_string(),
        "google" => "gemini-pro".to_string(),
        "github" => "gpt-4o-copilot".to_string(),
        "ollama" => "llama3".to_string(),
        // OpenRouter default - use the generic free tier
        "openrouter" => "openrouter/free".to_string(),
        // Default to gpt-4o for unknown providers for backward compatibility
        _ => "gpt-4o".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_env_name_known_providers() {
        assert_eq!(api_key_env_name("openai"), "OPENAI_API_KEY");
        assert_eq!(api_key_env_name("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(api_key_env_name("openrouter"), "OPENROUTER_API_KEY");
    }

    #[test]
    fn test_api_key_env_name_custom_provider() {
        assert_eq!(api_key_env_name("custom"), "CUSTOM_API_KEY");
        assert_eq!(api_key_env_name("my-provider"), "MY_PROVIDER_API_KEY");
        assert_eq!(api_key_env_name("DeepSeek"), "DEEPSEEK_API_KEY");
    }

    #[test]
    fn test_api_key_env_name_case_insensitive() {
        assert_eq!(api_key_env_name("OPENAI"), "OPENAI_API_KEY");
        assert_eq!(api_key_env_name("Anthropic"), "ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_default_model_for_provider_known() {
        assert_eq!(default_model_for_provider("openai"), "gpt-4o");
        assert_eq!(default_model_for_provider("google"), "gemini-pro");
        assert_eq!(default_model_for_provider("github"), "gpt-4o-copilot");
        assert_eq!(default_model_for_provider("ollama"), "llama3");
        assert_eq!(default_model_for_provider("openrouter"), "openrouter/free");
    }

    #[test]
    fn test_default_model_for_provider_unknown() {
        assert_eq!(default_model_for_provider("unknown"), "gpt-4o");
        assert_eq!(default_model_for_provider("custom"), "gpt-4o");
    }

    #[test]
    fn test_default_model_for_provider_case_insensitive() {
        assert_eq!(default_model_for_provider("OPENAI"), "gpt-4o");
        assert_eq!(default_model_for_provider("OpenAI"), "gpt-4o");
    }
}
