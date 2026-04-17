// RustyCode Substitution Engine
//
// Handles {env:VAR} and {file:path} substitutions in configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub struct SubstitutionEngine {
    cache: HashMap<String, CachedValue>,
    recursion_limit: usize,
    /// Security: restrict file reads to this directory (None = default config dir)
    allowed_base: Option<PathBuf>,
}

#[derive(Clone)]
struct CachedValue {
    value: String,
    timestamp: SystemTime,
    ttl: Option<Duration>,
}

impl Default for SubstitutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstitutionEngine {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            recursion_limit: 10,
            allowed_base: None,
        }
    }

    /// Set the allowed base directory for file substitutions.
    /// If None, uses the default ~/.config/rustycode directory.
    /// This is useful for testing.
    pub fn with_allowed_base(mut self, base: PathBuf) -> Self {
        self.allowed_base = Some(base);
        self
    }

    pub fn process(&mut self, input: &str) -> Result<String, SubstitutionError> {
        self.process_with_depth(input, 0)
    }

    fn process_with_depth(
        &mut self,
        input: &str,
        depth: usize,
    ) -> Result<String, SubstitutionError> {
        if depth >= self.recursion_limit {
            return Err(SubstitutionError::RecursionLimitExceeded);
        }

        // Pre-allocate result string with estimated capacity
        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Check for escaped {{
                if chars.peek() == Some(&'{') {
                    chars.next();
                    result.push('{');
                    continue;
                }

                // Extract substitution
                let substitution = self.extract_substitution(&mut chars)?;

                // Resolve substitution recursively (support nested substitutions)
                let resolved = self.resolve_substitution(&substitution, depth)?;

                result.push_str(&resolved);
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    fn extract_substitution(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Result<String, SubstitutionError> {
        // Pre-allocate substitution string with reasonable capacity
        let mut substitution = String::with_capacity(64);
        let mut brace_depth = 1;

        for ch in chars.by_ref() {
            match ch {
                '{' => {
                    brace_depth += 1;
                    substitution.push(ch);
                }
                '}' => {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        return Ok(substitution);
                    }
                    substitution.push(ch);
                }
                _ => substitution.push(ch),
            }
        }

        Err(SubstitutionError::UnterminatedSubstitution)
    }

    fn resolve_substitution(
        &mut self,
        substitution: &str,
        depth: usize,
    ) -> Result<String, SubstitutionError> {
        // Parse substitution: allow either `{kind:value}` or shorthand `{kind}`
        let (raw_kind, raw_value) = if let Some(colon_pos) = substitution.find(':') {
            (&substitution[..colon_pos], &substitution[colon_pos + 1..])
        } else {
            // No colon => treat the whole token as kind with empty value
            (substitution, "")
        };

        // Trim whitespace around kind/value to be permissive (e.g., `{ current_model }`)
        let kind = raw_kind.trim();
        let value = raw_value.trim();

        // Normalize kind: strip surrounding quotes, replace '-' with '_' and lowercase
        let norm_kind = kind
            .trim_matches(|c| c == '"' || c == '\'')
            .replace('-', "_")
            .to_lowercase();

        // If the value contains nested substitutions, process them first
        let processed_value = if value.contains('{') {
            // Increase depth to avoid infinite recursion
            self.process_with_depth(value, depth + 1)?
        } else {
            value.to_string()
        };

        match norm_kind.as_str() {
            "env" => self.resolve_env(&processed_value),
            "file" => self.resolve_file(&processed_value),
            "default" => Ok(processed_value),
            // Backward-compatible helpers used in config templates/tests
            "current_model" => {
                // Prefer explicit env override, fall back to a reasonable default
                if let Ok(val) = std::env::var("CURRENT_MODEL") {
                    Ok(val)
                } else {
                    Ok("claude-sonnet-4-6".to_string())
                }
            }
            // Allow shorthand: if someone writes `{kind}` treat as kind with empty value
            _ => Err(SubstitutionError::UnknownKind(norm_kind.to_string())),
        }
    }

    fn resolve_env(&self, var_name: &str) -> Result<String, SubstitutionError> {
        std::env::var(var_name).map_err(|_| SubstitutionError::EnvVarNotFound(var_name.to_string()))
    }

    fn resolve_file(&mut self, path: &str) -> Result<String, SubstitutionError> {
        // Expand ~
        let expanded = self.expand_tilde(path)?;

        // SECURITY: Restrict file reads to designated config directory
        // This prevents reading arbitrary files like /etc/passwd, ~/.ssh/id_rsa, etc.
        let allowed_base = if let Some(custom_base) = &self.allowed_base {
            custom_base.clone()
        } else {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("rustycode")
        };

        // Canonicalize both paths to resolve symlinks and prevent TOCTOU attacks
        let canonical_path = std::fs::canonicalize(&expanded)
            .map_err(|e| SubstitutionError::FileReadError(expanded.clone(), e.to_string()))?;

        let canonical_allowed =
            std::fs::canonicalize(&allowed_base).unwrap_or_else(|_| allowed_base.clone());

        // Verify the file is within the allowed directory
        if !canonical_path.starts_with(&canonical_allowed) {
            return Err(SubstitutionError::SecurityError(format!(
                "File references outside the rustycode config directory are not allowed: {}",
                path
            )));
        }

        // Check cache
        if let Some(cached) = self.cache.get(path) {
            if let Some(ttl) = cached.ttl {
                if cached
                    .timestamp
                    .elapsed()
                    .unwrap_or(std::time::Duration::MAX)
                    < ttl
                {
                    return Ok(cached.value.clone());
                }
            } else {
                return Ok(cached.value.clone());
            }
        }

        // Read file (using the canonical path)
        let content = std::fs::read_to_string(&canonical_path)
            .map_err(|e| SubstitutionError::FileReadError(canonical_path.clone(), e.to_string()))?;

        let trimmed = content.trim().to_string();

        // Cache result (5 minute TTL)
        self.cache.insert(
            path.to_string(),
            CachedValue {
                value: trimmed.clone(),
                timestamp: SystemTime::now(),
                ttl: Some(Duration::from_secs(300)),
            },
        );

        Ok(trimmed)
    }

    fn expand_tilde(&self, path: &str) -> Result<PathBuf, SubstitutionError> {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return Ok(home.join(stripped));
            }
        }

        Ok(PathBuf::from(path))
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SubstitutionError {
    #[error("Recursion limit exceeded")]
    RecursionLimitExceeded,

    #[error("Unterminated substitution")]
    UnterminatedSubstitution,

    #[error("Invalid substitution format: {0}")]
    InvalidFormat(String),

    #[error("Unknown substitution kind: {0}")]
    UnknownKind(String),

    #[error("Environment variable not found: {0}")]
    EnvVarNotFound(String),

    #[error("Failed to read file {0}: {1}")]
    FileReadError(PathBuf, String),

    #[error("Security error: {0}")]
    SecurityError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_substitution() {
        std::env::set_var("TEST_VAR", "test_value");

        let mut engine = SubstitutionEngine::new();
        let result = engine.process("{env:TEST_VAR}");

        assert_eq!(result.unwrap(), "test_value");
    }

    #[test]
    fn test_file_substitution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "file content").unwrap();

        let mut engine = SubstitutionEngine::new().with_allowed_base(temp_dir.path().to_path_buf());
        let result = engine.process(&format!("{{file:{}}}", file_path.display()));

        assert_eq!(result.unwrap(), "file content");
    }

    #[test]
    fn test_nested_substitutions() {
        // Use a temporary file so tests don't depend on absolute paths
        let temp_dir = tempfile::tempdir().unwrap();
        let key_path = temp_dir.path().join("key.txt");
        std::fs::write(&key_path, "secret-key").unwrap();
        std::env::set_var("KEY_FILE", key_path.to_string_lossy().to_string());

        let mut engine = SubstitutionEngine::new().with_allowed_base(temp_dir.path().to_path_buf());
        let result = engine.process("{file:{env:KEY_FILE}}");

        assert_eq!(result.unwrap(), "secret-key");
    }

    #[test]
    fn test_default_substitution() {
        let mut engine = SubstitutionEngine::new();
        let result = engine.process("{default:default_value}");

        assert_eq!(result.unwrap(), "default_value");
    }
}
