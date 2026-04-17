# Configuration System Detailed Design

## Overview

This document provides detailed implementation specifications for the upgraded configuration system, incorporating patterns from opencoderust (JSONC + substitutions), gemini-cli (hierarchical merging), and kilocode (multi-source loading).

## Architecture

```
Configuration Loading Pipeline:
┌─────────────────────────────────────────────────────────────┐
│ 1. Search Path Discovery                                    │
│    - Global: ~/.config/rustycode/config.jsonc               │
│    - Home: ~/.rustycode/config.jsonc                        │
│    - Workspace: .rustycode/config.jsonc                     │
│    - Project: .rustycode/project.jsonc                      │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ 2. JSON/JSONC Parsing                                        │
│    - Remove comments (// and /* */)                         │
│    - Remove trailing commas                                 │
│    - Parse as JSON                                           │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ 3. Hierarchical Merging                                     │
│    - Apply merge strategies (deep, override, concat)        │
│    - Priority: project > workspace > global > template      │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ 4. Substitution Processing                                  │
│    - {env:VAR_NAME} → environment variable                  │
│    - {file:path} → file contents                            │
│    - Recursive substitution resolution                       │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ 5. Schema Validation                                        │
│    - Validate against JSON Schema                           │
│    - Check required fields                                  │
│    - Type checking                                           │
└──────────────────┬──────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│ 6. Deserialization                                           │
│    - Convert to Config struct                               │
│    - Apply defaults                                          │
│    - Validate invariants                                     │
└─────────────────────────────────────────────────────────────┘
```

## Data Structures

### Config Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,

    // Core settings
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,

    // Provider configuration
    #[serde(default)]
    pub providers: ProvidersConfig,

    // Workspace settings
    pub workspace: Option<WorkspaceConfig>,

    // Features
    #[serde(default)]
    pub features: FeaturesConfig,

    // Advanced
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    pub anthropic: Option<ProviderConfig>,
    pub openai: Option<ProviderConfig>,
    pub openrouter: Option<ProviderConfig>,
    #[serde(flatten)]
    pub custom: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: Option<String>,
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeaturesConfig {
    #[serde(default)]
    pub git_integration: bool,

    #[serde(default)]
    pub file_watcher: bool,

    #[serde(default)]
    pub mcp_servers: Vec<String>,

    #[serde(default)]
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvancedConfig {
    #[serde(default)]
    pub log_level: String,

    #[serde(default)]
    pub cache_enabled: bool,

    #[serde(default)]
    pub telemetry_enabled: bool,

    #[serde(default)]
    pub experimental: HashMap<String, Value>,
}
```

### JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "RustyCode Configuration",
  "type": "object",
  "required": ["model"],
  "properties": {
    "model": {
      "type": "string",
      "description": "Default model to use"
    },
    "temperature": {
      "type": "number",
      "minimum": 0.0,
      "maximum": 1.0,
      "description": "Temperature for generation"
    },
    "max_tokens": {
      "type": "integer",
      "minimum": 1,
      "description": "Maximum tokens to generate"
    },
    "providers": {
      "type": "object",
      "description": "Provider configurations"
    },
    "workspace": {
      "type": "object",
      "properties": {
        "name": {"type": "string"},
        "root": {"type": "string"},
        "features": {
          "type": "array",
          "items": {"type": "string"}
        }
      }
    }
  }
}
```

## Implementation Details

### 1. JSONC Parser

```rust
// crates/rustycode-config/src/jsonc/parser.rs

use serde_json::Value;
use std::result::Result as StdResult;

pub struct JsoncParser {
    allow_comments: bool,
    allow_trailing_commas: bool,
}

impl JsoncParser {
    pub fn new() -> Self {
        Self {
            allow_comments: true,
            allow_trailing_commas: true,
        }
    }

    pub fn parse_str(&self, input: &str) -> StdResult<Value, ParseError> {
        let mut chars = CharIndices::new(input);
        let mut output = String::new();
        let mut depth = 0;

        while let Some((idx, ch)) = chars.next() {
            match ch {
                '/' => self.handle_slash(&mut chars, &mut output)?,
                '"' => self.handle_string(&mut chars, &mut output)?,
                _ => {
                    // Track depth for trailing comma detection
                    self.track_depth(ch, &mut depth);
                    output.push(ch);
                }
            }
        }

        // Parse as JSON
        serde_json::from_str(&output)
            .map_err(ParseError::JsonError)
    }

    fn handle_slash(
        &self,
        chars: &mut CharIndices,
        output: &mut String,
    ) -> StdResult<(), ParseError> {
        match chars.next() {
            Some((_, '/')) => {
                // Line comment - consume until newline
                while let Some((_, ch)) = chars.next() {
                    if ch == '\n' {
                        output.push(ch);
                        break;
                    }
                }
                Ok(())
            }
            Some((_, '*')) => {
                // Block comment
                let mut depth = 1;
                while let Some((_, ch)) = chars.next() {
                    match ch {
                        '/' => {
                            if chars.peek() == Some('*') {
                                chars.next();
                                depth += 1;
                            }
                        }
                        '*' => {
                            if chars.peek() == Some('/') {
                                chars.next();
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if depth > 0 {
                    return Err(ParseError::UnterminatedBlockComment);
                }
                Ok(())
            }
            _ => {
                output.push('/');
                Ok(())
            }
        }
    }

    fn handle_string(
        &self,
        chars: &mut CharIndices,
        output: &mut String,
    ) -> StdResult<(), ParseError> {
        output.push('"');

        while let Some((_, ch)) = chars.next() {
            output.push(ch);

            if ch == '\\' {
                // Escape sequence
                if let Some((_, next)) = chars.next() {
                    output.push(next);
                }
            } else if ch == '"' {
                break;
            }
        }

        Ok(())
    }

    fn track_depth(&self, ch: char, depth: &mut usize) {
        match ch {
            '{' | '[' => *depth += 1,
            '}' | ']' => *depth = depth.saturating_sub(1),
            _ => {}
        }
    }
}

struct CharIndices<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    indices: std::vec::IntoIter<usize>,
}

impl<'a> CharIndices<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            chars: s.chars().peekable(),
            indices: (0..).into_iter(),
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
}

impl<'a> Iterator for CharIndices<'a> {
    type Item = (usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        self.indices.next().map(|i| (i, self.chars.next()?))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("JSON error: {0}")]
    JsonError(serde_json::Error),

    #[error("Unterminated block comment")]
    UnterminatedBlockComment,

    #[error("Unterminated string")]
    UnterminatedString,
}
```

### 2. Substitution Engine

```rust
// crates/rustycode-config/src/substitutions/engine.rs

use std::collections::HashMap;
use std::path::PathBuf;

pub struct SubstitutionEngine {
    cache: HashMap<String, CachedValue>,
    recursion_limit: usize,
}

#[derive(Clone)]
struct CachedValue {
    value: String,
    timestamp: std::time::SystemTime,
    ttl: Option<std::time::Duration>,
}

impl SubstitutionEngine {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            recursion_limit: 10,
        }
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

        let mut result = input.to_string();
        let mut has_substitutions = true;

        // Keep processing until no more substitutions found
        while has_substitutions {
            has_substitutions = false;

            result = self.replace_substitutions(&result, depth, &mut has_substitutions)?;
        }

        Ok(result)
    }

    fn replace_substitutions(
        &mut self,
        input: &str,
        depth: usize,
        has_substitutions: &mut bool,
    ) -> Result<String, SubstitutionError> {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Check for substitution
                if chars.peek() == Some(&'{') {
                    // Escaped {{
                    chars.next();
                    result.push('{');
                    continue;
                }

                let substitution = self.extract_substitution(&mut chars)?;

                *has_substitutions = true;

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
        let mut substitution = String::new();
        let mut brace_depth = 1;

        while let Some(ch) = chars.next() {
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
        // Parse substitution: {kind:value}
        let colon_pos = substitution
            .find(':')
            .ok_or_else(|| SubstitutionError::InvalidFormat(substitution.to_string()))?;

        let kind = &substitution[..colon_pos];
        let value = &substitution[colon_pos + 1..];

        match kind {
            "env" => self.resolve_env(value),
            "file" => self.resolve_file(value),
            "default" => Ok(value.to_string()),
            _ => Err(SubstitutionError::UnknownKind(kind.to_string())),
        }
    }

    fn resolve_env(&self, var_name: &str) -> Result<String, SubstitutionError> {
        std::env::var(var_name)
            .map_err(|_| SubstitutionError::EnvVarNotFound(var_name.to_string()))
    }

    fn resolve_file(&mut self, path: &str) -> Result<String, SubstitutionError> {
        // Expand ~
        let expanded = self.expand_tilde(path)?;

        // Check cache
        if let Some(cached) = self.cache.get(path) {
            if let Some(ttl) = cached.ttl {
                if cached.timestamp.elapsed().unwrap() < ttl {
                    return Ok(cached.value.clone());
                }
            } else {
                return Ok(cached.value.clone());
            }
        }

        // Read file
        let content = std::fs::read_to_string(&expanded)
            .map_err(|e| SubstitutionError::FileReadError(expanded, e.to_string()))?;

        let trimmed = content.trim().to_string();

        // Cache result
        self.cache.insert(
            path.to_string(),
            CachedValue {
                value: trimmed.clone(),
                timestamp: std::time::SystemTime::now(),
                ttl: Some(std::time::Duration::from_secs(300)), // 5 minutes
            },
        );

        Ok(trimmed)
    }

    fn expand_tilde(&self, path: &str) -> Result<PathBuf, SubstitutionError> {
        if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return Ok(home.join(&path[2..]));
            }
        }

        Ok(PathBuf::from(path))
    }
}

#[derive(Debug, thiserror::Error)]
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
}
```

### 3. Configuration Loader

```rust
// crates/rustycode-config/src/loader/mod.rs

use std::path::{Path, PathBuf};

pub struct ConfigLoader {
    parser: JsoncParser,
    substitutions: SubstitutionEngine,
    search_paths: Vec<PathBuf>,
    schema_validator: Option<SchemaValidator>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            parser: JsoncParser::new(),
            substitutions: SubstitutionEngine::new(),
            search_paths: Self::default_search_paths(),
            schema_validator: None,
        }
    }

    pub fn with_schema_validator(mut self, validator: SchemaValidator) -> Self {
        self.schema_validator = Some(validator);
        self
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    pub fn load(&mut self, cwd: &Path) -> Result<Config, ConfigError> {
        // Phase 1: Discover config files
        let config_files = self.discover_configs(cwd)?;

        // Phase 2: Parse all configs
        let mut parsed_configs = Vec::new();
        for (source, path) in config_files {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| ConfigError::FileReadError(path.clone(), e.to_string()))?;

            let parsed = self.parser.parse_str(&content)
                .map_err(|e| ConfigError::ParseError(path.clone(), e.to_string()))?;

            parsed_configs.push((source, parsed));
        }

        // Phase 3: Merge configs
        let merged = self.merge_configs(parsed_configs)?;

        // Phase 4: Process substitutions
        let substituted = self.process_substitutions(merged)?;

        // Phase 5: Validate schema
        if let Some(validator) = &self.schema_validator {
            validator.validate(&substituted)?;
        }

        // Phase 6: Deserialize
        let config: Config = serde_json::from_value(substituted)
            .map_err(|e| ConfigError::DeserializeError(e.to_string()))?;

        Ok(config)
    }

    fn discover_configs(&self, cwd: &Path) -> Result<Vec<(ConfigSource, PathBuf)>, ConfigError> {
        let mut configs = Vec::new();

        // Global configs
        for path in &self.search_paths {
            let config_file = path.join("config.jsonc");
            if config_file.exists() {
                configs.push((ConfigSource::Global, config_file));
            }
        }

        // Workspace config
        let workspace_config = cwd.join(".rustycode/config.jsonc");
        if workspace_config.exists() {
            configs.push((ConfigSource::Workspace, workspace_config));
        }

        // Project config
        let project_config = cwd.join(".rustycode/project.jsonc");
        if project_config.exists() {
            configs.push((ConfigSource::Project, project_config));
        }

        Ok(configs)
    }

    fn merge_configs(
        &self,
        configs: Vec<(ConfigSource, Value)>,
    ) -> Result<Value, ConfigError> {
        let mut merged = serde_json::json!({});

        // Sort by priority (lowest first)
        let mut sorted = configs;
        sorted.sort_by_key(|(source, _)| source.priority());

        for (_source, config) in sorted {
            merged = self.deep_merge(merged, config)?;
        }

        Ok(merged)
    }

    fn deep_merge(&self, base: Value, override_: Value) -> Result<Value, ConfigError> {
        match (base, override_) {
            (Value::Object(mut base_map), Value::Object(override_map)) => {
                for (key, override_value) in override_map {
                    let base_value = base_map.remove(&key);

                    let merged = match (base_value, override_value) {
                        (Some(base_val), override_val) => {
                            if let Some(merge_strategy) = self.extract_merge_strategy(&override_val) {
                                self.apply_merge_strategy(base_val, override_val, merge_strategy)?
                            } else {
                                self.deep_merge(base_val, override_val)?
                            }
                        }
                        (None, override_val) => override_val,
                    };

                    base_map.insert(key, merged);
                }

                Ok(Value::Object(base_map))
            }
            (_, override_value) => Ok(override_value),
        }
    }

    fn extract_merge_strategy(&self, value: &Value) -> Option<MergeStrategy> {
        value.get("$merge")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "override" => Some(MergeStrategy::Override),
                "concat" => Some(MergeStrategy::Concat),
                "deep" => Some(MergeStrategy::Deep),
                _ => None,
            })
    }

    fn apply_merge_strategy(
        &self,
        base: Value,
        override_: Value,
        strategy: MergeStrategy,
    ) -> Result<Value, ConfigError> {
        match strategy {
            MergeStrategy::Override => Ok(override_),
            MergeStrategy::Concat => {
                match (base, override_) {
                    (Value::Array(mut base_arr), Value::Array(override_arr)) => {
                        base_arr.extend(override_arr);
                        Ok(Value::Array(base_arr))
                    }
                    _ => Ok(override_),
                }
            }
            MergeStrategy::Deep => self.deep_merge(base, override_),
        }
    }

    fn process_substitutions(&mut self, value: Value) -> Result<Value, ConfigError> {
        match value {
            Value::String(s) => {
                let processed = self.substitutions.process(&s)
                    .map_err(ConfigError::SubstitutionError)?;
                Ok(Value::String(processed))
            }
            Value::Object(map) => {
                let mut result = serde_json::Map::new();
                for (key, value) in map {
                    result.insert(key, self.process_substitutions(value)?);
                }
                Ok(Value::Object(result))
            }
            Value::Array(arr) => {
                let result: Result<Vec<_>, _> = arr
                    .into_iter()
                    .map(|v| self.process_substitutions(v))
                    .collect();
                Ok(Value::Array(result?))
            }
            _ => Ok(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ConfigSource {
    Global,
    Workspace,
    Project,
}

impl ConfigSource {
    fn priority(&self) -> i32 {
        match self {
            ConfigSource::Global => 0,
            ConfigSource::Workspace => 1,
            ConfigSource::Project => 2,
        }
    }
}

enum MergeStrategy {
    Override,
    Concat,
    Deep,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read file {0}: {1}")]
    FileReadError(PathBuf, String),

    #[error("Failed to parse file {0}: {1}")]
    ParseError(PathBuf, String),

    #[error("Substitution error: {0}")]
    SubstitutionError(#[from] SubstitutionError),

    #[error("Schema validation error: {0}")]
    ValidationError(String),

    #[error("Failed to deserialize config: {0}")]
    DeserializeError(String),
}
```

### 4. Schema Validator

```rust
// crates/rustycode-config/src/schema/validator.rs

use jsonschema::{JSONSchema, ValidationError};

pub struct SchemaValidator {
    schema: JSONSchema,
}

impl SchemaValidator {
    pub fn from_schema(schema: &Value) -> Result<Self, SchemaError> {
        let schema = JSONSchema::compile(schema)
            .map_err(SchemaError::CompileError)?;
        Ok(Self { schema })
    }

    pub fn from_file(path: &Path) -> Result<Self, SchemaError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SchemaError::FileReadError(path.to_path_buf(), e.to_string()))?;

        let schema: Value = serde_json::from_str(&content)
            .map_err(|e| SchemaError::ParseError(e.to_string()))?;

        Self::from_schema(&schema)
    }

    pub fn validate(&self, instance: &Value) -> Result<(), SchemaError> {
        let result = self.schema.validate(instance);

        if let Err(errors) = result {
            let error_messages: Vec<_> = errors
                .map(|e| e.to_string())
                .collect();

            return Err(SchemaError::ValidationErrors(error_messages));
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("Failed to compile schema: {0}")]
    CompileError(jsonschema::CompilationError),

    #[error("Failed to read schema file {0}: {1}")]
    FileReadError(PathBuf, String),

    #[error("Failed to parse schema: {0}")]
    ParseError(serde_json::Error),

    #[error("Validation errors:\n{0}")]
    ValidationErrors(Vec<String>),
}
```

## Usage Examples

### Basic Usage

```rust
use rustycode_config::ConfigLoader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut loader = ConfigLoader::new();

    let cwd = std::env::current_dir()?;
    let config = loader.load(&cwd)?;

    println!("Model: {}", config.model);
    println!("Temperature: {:?}", config.temperature);

    Ok(())
}
```

### Advanced Usage with Schema Validation

```rust
use rustycode_config::ConfigLoader;
use rustycode_config::schema::SchemaValidator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema_validator = SchemaValidator::from_file("schema.json")?;
    let mut loader = ConfigLoader::new()
        .with_schema_validator(schema_validator);

    let cwd = std::env::current_dir()?;
    let config = loader.load(&cwd)?;

    Ok(())
}
```

### Custom Substitutions

```rust
use rustycode_config::ConfigLoader;
use std::path::PathBuf;

let config_content = r#"
{
  "apiKey": "{env:ANTHROPIC_API_KEY}",
  "systemPrompt": "{file:~/prompts/default.txt}",
  "model": "{env:MODEL_OVERRIDE:claude-3-5-sonnet-20250514}"
}
"#;

std::fs::write("config.jsonc", config_content)?;
std::env::set_var("ANTHROPIC_API_KEY", "sk-1234");
std::fs::write(
    PathBuf::from(env!("HOME")).join("prompts/default.txt"),
    "You are a helpful assistant."
)?;

let mut loader = ConfigLoader::new();
let config = loader.load(&PathBuf::from("."))?;

assert_eq!(config.providers.anthropic.api_key, Some("sk-1234".to_string()));
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonc_parsing() {
        let parser = JsoncParser::new();
        let input = r#"
        {
            // This is a comment
            "model": "claude-3-5-sonnet", /* inline comment */
            "temperature": 0.1,
            "features": ["git", "watcher",], // trailing comma
        }
        "#;

        let result = parser.parse_str(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_env_substitution() {
        std::env::set_var("TEST_VAR", "test-value");

        let mut engine = SubstitutionEngine::new();
        let result = engine.process("{env:TEST_VAR}");

        assert_eq!(result.unwrap(), "test-value");
    }

    #[test]
    fn test_file_substitution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "file content").unwrap();

        let mut engine = SubstitutionEngine::new();
        let result = engine.process(&format!("{{file:{}}}", file_path.display()));

        assert_eq!(result.unwrap(), "file content");
    }

    #[test]
    fn test_nested_substitutions() {
        std::env::set_var("KEY_FILE", "/tmp/key.txt");
        std::fs::write("/tmp/key.txt", "secret-key").unwrap();

        let mut engine = SubstitutionEngine::new();
        let result = engine.process("{file:{env:KEY_FILE}}");

        assert_eq!(result.unwrap(), "secret-key");
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_full_config_pipeline() {
    let temp_dir = tempfile::tempdir().unwrap();

    // Create global config
    std::fs::write(
        temp_dir.path().join("config.jsonc"),
        r#"
        {
          "$schema": "./schema.json",
          "model": "claude-3-5-sonnet-20250514",
          "providers": {
            "anthropic": {
              "apiKey": "{env:ANTHROPIC_API_KEY}"
            }
          }
        }
        "#
    ).unwrap();

    std::env::set_var("ANTHROPIC_API_KEY", "sk-test");

    let mut loader = ConfigLoader::new();
    loader.add_search_path(temp_dir.path().to_path_buf());

    let config = loader.load(temp_dir.path()).unwrap();

    assert_eq!(config.model, "claude-3-5-sonnet-20250514");
    assert_eq!(config.providers.anthropic.api_key, Some("sk-test".to_string()));
}
```

## Performance Considerations

### Caching Strategy

1. **File Content Cache**: Substituted file contents are cached for 5 minutes
2. **Parsed Config Cache**: Parsed configs are cached in memory
3. **Schema Cache**: Compiled schemas are cached

### Memory Usage

- JSONC parser: O(n) where n is input size
- Substitution engine: O(d * s) where d is depth, s is string size
- Merge operation: O(m + n) where m, n are object sizes

## Error Handling

All errors are detailed and actionable:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read configuration file {0}: {1}")]
    FileReadError(PathBuf, String),

    #[error("Failed to parse configuration file {0}: {1}")]
    ParseError(PathBuf, String),

    #[error("Configuration validation failed: {0}")]
    ValidationError(String),

    #[error("Environment variable '{0}' not found")]
    EnvVarNotFound(String),

    #[error("Failed to read file for substitution {0}: {1}")]
    FileSubstitutionError(PathBuf, String),
}
```

## Migration Guide

### From TOML to JSONC

**Before (config.toml):**
```toml
model = "claude-3-5-sonnet-20250514"
temperature = 0.1
data_dir = "~/.rustycode"
```

**After (config.jsonc):**
```jsonc
{
  // Model configuration
  "model": "claude-3-5-sonnet-20250514",

  // Generation parameters
  "temperature": 0.1,

  // Data directory
  "data_dir": "{env:RUSTYCODE_DATA_DIR:~/.rustycode}"
}
```

## Dependencies

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
dirs = "6"
shellexpand = "3"
jsonschema = "0.20"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["full"] }
```

## Future Enhancements

1. **Remote Configuration**: Load configs from URLs
2. **Configuration Profiles**: Named config profiles
3. **Configuration Diff**: Show differences between configs
4. **Configuration Watcher**: Reload on file changes
5. **Configuration Encryption**: Encrypt sensitive fields
6. **Configuration Validation**: Lint configurations
7. **Configuration Migration**: Automatic migration from old versions
