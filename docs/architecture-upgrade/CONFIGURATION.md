# Configuration Guide

## Configuration Hierarchy

RustyCode uses a hierarchical configuration system that merges settings from multiple sources in priority order. This allows for sensible defaults at the global level while enabling project-specific overrides.

### Configuration Levels

```
1. Library Defaults (lowest priority)
   └─ Built-in default configuration in code

2. Global Config
   └─ ~/.config/rustycode/config.json
   └─ XDG standard location

3. Workspace Config
   └─ .rustycode-workspace/config.json
   └─ Found by searching upward from project directory

4. Project Config (highest priority)
   └─ .rustycode/config.json
   └─ Project-specific overrides
```

### Merge Strategy

Configuration values are deep-merged, meaning:
- **Nested objects** are merged recursively
- **Arrays** are replaced (not merged)
- **Scalar values** from higher levels override lower levels

Example:

**Global Config** (`~/.config/rustycode/config.json`):
```json
{
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "features": {
    "git_integration": true,
    "file_watcher": false
  }
}
```

**Project Config** (`.rustycode/config.json`):
```json
{
  "model": "claude-opus-4-6",
  "features": {
    "file_watcher": true
  }
}
```

**Merged Result**:
```json
{
  "model": "claude-opus-4-6",           // Project override
  "temperature": 0.1,                   // Inherited from global
  "features": {
    "git_integration": true,            // Inherited from global
    "file_watcher": true                // Project override
  }
}
```

## Configuration Features

### JSONC Syntax

RustyCode supports JSONC (JSON with Comments), which allows:
- **Comments**: Both `// single-line` and `/* multi-line */`
- **Trailing commas**: Commas after last item in objects/arrays
- **Quotes**: Standard JSON quoting rules

Example:
```jsonc
{
  // This is a comment
  "model": "claude-3-5-sonnet-latest",

  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",  // Environment variable
      "models": ["claude-3-5-sonnet-latest", "claude-opus-4-6"]
    }
  },

  "features": {
    "git_integration": true,
    "file_watcher": false,
  }  // Trailing comma is OK
}
```

### Environment Variables

Reference environment variables using the `{env:VAR_NAME}` syntax:

```json
{
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}"
    },
    "openai": {
      "api_key": "{env:OPENAI_API_KEY}"
    }
  }
}
```

**Benefits**:
- **Security**: Don't store API keys in config files
- **Flexibility**: Different keys for different environments
- **Standard**: Use existing environment variable conventions

### File References

Include contents of other files using the `{file:path}` syntax:

```json
{
  "providers": {
    "anthropic": {
      "api_key": "{file:/secure/api-keys.txt:anthropic}"
    }
  }
}
```

**Supported file formats**:
- Plain text files (first line or keyed access)
- JSON files (specific key access)
- Environment files (`.env` format)

### Directory Customization

Customize directory locations using the `CODEX_HOME` environment variable:

```bash
# Set custom base directory
export CODEX_HOME=/custom/path

# Directories will be created under:
# - /custom/path/rustycode/data
# - /custom/path/rustycode/memory
# - /custom/path/rustycode/skills
```

**In config**:
```json
{
  "data_dir": "/custom/path/rustycode/data",
  "memory_dir": "/custom/path/rustycode/memory",
  "skills_dir": "/custom/path/rustycode/skills"
}
```

## Configuration Examples

### Basic Configuration

Minimal configuration for getting started:

```json
{
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "max_tokens": 4096
}
```

### Multi-Provider Setup

Configure multiple LLM providers:

```json
{
  "model": "claude-3-5-sonnet-latest",

  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",
      "models": [
        "claude-3-5-sonnet-latest",
        "claude-opus-4-6",
        "claude-haiku-4-5"
      ]
    },
    "openai": {
      "api_key": "{env:OPENAI_API_KEY}",
      "base_url": "https://api.openai.com/v1",
      "models": ["gpt-4o", "gpt-4-turbo"]
    },
    "openrouter": {
      "api_key": "{env:OPENROUTER_API_KEY}",
      "base_url": "https://openrouter.ai/api/v1"
    }
  }
}
```

### Custom Directories

Customize where RustyCode stores data:

```json
{
  "data_dir": "/custom/rustycode/data",
  "memory_dir": "/custom/rustycode/memory",
  "skills_dir": "/custom/rustycode/skills"
}
```

Or using `CODEX_HOME`:
```bash
export CODEX_HOME=/custom/rustycode
# Automatically uses:
# - /custom/rustycode/data
# - /custom/rustycode/memory
# - /custom/rustycode/skills
```

### Feature Flags

Enable/disable features:

```json
{
  "features": {
    "git_integration": true,
    "file_watcher": true,
    "mcp_servers": ["filesystem", "git"],
    "agents": ["code-reviewer", "security-expert", "test-coverage-analyst"]
  }
}
```

### Advanced Configuration

Advanced options for power users:

```json
{
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "max_tokens": 8192,

  "advanced": {
    "log_level": "debug",
    "cache_enabled": true,
    "telemetry_enabled": false,
    "experimental": {
      "parallel_agents": true,
      "semantic_compaction": false
    }
  },

  "lsp_servers": [
    "rust-analyzer",
    "typescript-language-server",
    "pyright"
  ]
}
```

## Configuration Reference

### Complete Config Schema

```typescript
interface Config {
  // Schema for validation (optional)
  $schema?: string;

  // Core LLM settings
  model: string;
  temperature?: number;        // Default: 0.1
  max_tokens?: number;         // Default: 4096

  // Provider configuration
  providers?: ProvidersConfig;

  // Workspace settings
  workspace?: WorkspaceConfig;

  // Feature flags
  features?: FeaturesConfig;

  // Advanced settings
  advanced?: AdvancedConfig;

  // Directory configuration
  data_dir?: string;           // Default: ~/.rustycode/data
  memory_dir?: string;         // Default: ~/.rustycode/memory
  skills_dir?: string;         // Default: ~/.rustycode/skills

  // LSP servers
  lsp_servers?: string[];
}

interface ProvidersConfig {
  anthropic?: ProviderConfig;
  openai?: ProviderConfig;
  openrouter?: ProviderConfig;
  gemini?: ProviderConfig;
  ollama?: ProviderConfig;
  [key: string]: ProviderConfig | undefined;
}

interface ProviderConfig {
  api_key?: string;            // Can use {env:VAR_NAME}
  base_url?: string;           // Custom endpoint
  models?: string[];           // Available models
  headers?: Record<string, string>;  // Custom headers
}

interface WorkspaceConfig {
  name?: string;
  root?: string;
  features?: string[];
}

interface FeaturesConfig {
  git_integration?: boolean;   // Default: false
  file_watcher?: boolean;      // Default: false
  mcp_servers?: string[];      // Default: []
  agents?: string[];           // Default: []
}

interface AdvancedConfig {
  log_level?: string;          // Default: "info"
  cache_enabled?: boolean;     // Default: false
  telemetry_enabled?: boolean; // Default: false
  experimental?: Record<string, any>;
}
```

### Default Values

```json
{
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "max_tokens": 4096,
  "data_dir": "~/.rustycode/data",
  "memory_dir": "~/.rustycode/memory",
  "skills_dir": "~/.rustycode/skills",
  "features": {
    "git_integration": false,
    "file_watcher": false,
    "mcp_servers": [],
    "agents": []
  },
  "advanced": {
    "log_level": "info",
    "cache_enabled": false,
    "telemetry_enabled": false,
    "experimental": {}
  },
  "lsp_servers": []
}
```

## Configuration Loading

### Programmatic Usage

```rust
use rustycode_config::Config;
use std::path::Path;

// Load from project directory
let config = Config::load(Path::new("/my/project"))?;

// Access configuration
println!("Using model: {}", config.model);
println!("Temperature: {:?}", config.temperature);

// Save configuration
config.save(Path::new("/my/project/.rustycode/config.json"))?;
```

### Custom Config Loader

```rust
use rustycode_config::{ConfigLoader, SubstitutionEngine};

let mut loader = ConfigLoader::new();

// Load from specific path
let config_value = loader.load_from_path(&path)?;

// Access raw JSON value
println!("{:#}", config_value);
```

## Best Practices

### Security
1. **Never commit API keys**: Use `{env:VAR_NAME}` instead
2. **Use environment files**: Load secrets from `.env` files (gitignored)
3. **Restrict permissions**: Config files should be readable only by user

### Organization
1. **Global defaults**: Set sensible defaults in global config
2. **Workspace overrides**: Use workspace config for ensemble settings
3. **Project specifics**: Keep project-specific overrides minimal

### Validation
1. **Use schema**: Add `$schema` for IDE validation support
2. **Test configs**: Use `Config::load()` to validate before deployment
3. **Document overrides**: Comment why specific values are overridden

### Performance
1. **Cache configs**: Config is loaded once and cached
2. **Avoid large files**: Keep configs focused and minimal
3. **Use substitutions**: Reference external files instead of embedding

## Troubleshooting

### Config Not Loading

**Problem**: Configuration changes not taking effect

**Solutions**:
1. Check config file location:
   ```bash
   # Find where config is being loaded from
   ls -la ~/.config/rustycode/config.json
   ls -la .rustycode/config.json
   ```

2. Verify JSON syntax:
   ```bash
   # Validate JSONC syntax
   rustycode config validate
   ```

3. Check merge order:
   ```bash
   # Show effective configuration
   rustycode config show
   ```

### Environment Variables Not Expanding

**Problem**: `{env:VAR_NAME}` not working

**Solutions**:
1. Verify environment variable is set:
   ```bash
   echo $ANTHROPIC_API_KEY
   ```

2. Check syntax in config:
   ```json
   {
     "api_key": "{env:ANTHROPIC_API_KEY}"  // Correct
     "api_key": "env:ANTHROPIC_API_KEY"    // Wrong
   }
   ```

3. Enable debug logging:
   ```json
   {
     "advanced": {
       "log_level": "debug"
     }
   }
   ```

### Directory Creation Errors

**Problem**: "Failed to create directory" error

**Solutions**:
1. Check parent directory permissions:
   ```bash
   ls -la ~/.config/
   ```

2. Create directories manually:
   ```bash
   mkdir -p ~/.config/rustycode
   mkdir -p ~/.rustycode/data
   mkdir -p ~/.rustycode/memory
   mkdir -p ~/.rustycode/skills
   ```

3. Use `CODEX_HOME` for custom location:
   ```bash
   export CODEX_HOME=/tmp/rustycode_test
   ```

### Merge Conflicts

**Problem**: Unexpected configuration values

**Solutions**:
1. Check effective configuration:
   ```bash
   rustycode config show
   ```

2. View merge sources:
   ```bash
   rustycode config explain
   ```

3. Test specific config file:
   ```bash
   rustycode config validate .rustycode/config.json
   ```

## Migration from Old Configuration

### ⚠️ TOML Support Removed

**TOML configuration is no longer supported.** All configuration files must use JSON/JSONC format.

If you still have TOML config files, you must convert them to JSON:

**Old (TOML) - No longer supported:**
```toml
[model]
name = "claude-3-5-sonnet-latest"
temperature = 0.1

[providers.anthropic]
api_key = "sk-ant-..."
```

**New (JSONC) - Required:**
```json
{
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}"
    }
  }
}
```

### Flat to Hierarchical

If you're migrating from flat configuration:

**Old**:
```json
{
  "anthropic_api_key": "sk-ant-...",
  "anthropic_model": "claude-3-5-sonnet-latest"
}
```

**New**:
```json
{
  "model": "claude-3-5-sonnet-latest",
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}"
    }
  }
}
```

## Conclusion

The RustyCode configuration system is designed to be:
- **Flexible**: Support multiple providers and features
- **Secure**: Environment variables for sensitive data
- **Hierarchical**: Global, workspace, and project levels
- **User-Friendly**: JSONC with comments and trailing commas
- **Validatable**: Schema support for IDE validation

For more information, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Provider Guide](PROVIDERS.md)
- [Agent System Guide](AGENTS.md)
