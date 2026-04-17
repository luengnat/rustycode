# Migration Guide

This guide helps you migrate from the old RustyCode system to the new architecture. It covers breaking changes, new APIs, configuration updates, and step-by-step migration instructions.

## From Old System

### Breaking Changes

#### 1. Configuration Format

**Old (TOML)**:
```toml
[ai]
provider = "anthropic"
model = "claude-3-5-sonnet"
api_key = "sk-ant-..."

[features]
git = true
```

**New (JSONC)**:
```json
{
  "model": "claude-3-5-sonnet-latest",
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",
      "models": ["claude-3-5-sonnet-latest"]
    }
  },
  "features": {
    "git_integration": true
  }
}
```

**Key Changes**:
- JSONC instead of TOML
- Hierarchical provider configuration
- Environment variable references (`{env:VAR_NAME}`)
- API keys moved to environment variables

#### 2. Provider Management

**Old**:
```rust
use rustycode_llm::create_provider_from_config;

let provider = create_provider_from_config()?;
```

**New**:
```rust
use rustycode_providers::bootstrap_from_env;

let registry = bootstrap_from_env().await;
let provider = registry.get_provider("anthropic")?;
```

**Key Changes**:
- Provider registry with auto-discovery
- Multi-provider support
- Cost tracking built-in

#### 3. Session System

**Old**:
```rust
use rustycode_llm::ChatSession;

let mut session = ChatSession::new();
session.add_message("user", "Hello");
```

**New**:
```rust
use rustycode_session::{Session, MessageV2};

let mut session = Session::new("My Session");
session.add_message(MessageV2::user("Hello".to_string()));
```

**Key Changes**:
- Rich message types (text, images, tools, code, diffs)
- Metadata tracking (tokens, costs, files touched)
- Compaction strategies
- Efficient serialization

#### 4. Agent System

**Old**:
```rust
// No agent system
```

**New**:
```rust
use rustycode_runtime::multi_agent::{MultiAgentOrchestrator, MultiAgentConfig};

let config = MultiAgentConfig {
    content: code.to_string(),
    ..MultiAgentConfig::default()
};

let orchestrator = MultiAgentOrchestrator::from_config(config)?;
let analysis = orchestrator.analyze().await?;
```

**Key Changes**:
- Built-in specialized agents
- Multi-agent orchestration
- Parallel execution
- Consensus building

### New APIs

#### Configuration Loading

**New API**:
```rust
use rustycode_config::Config;
use std::path::Path;

// Load with hierarchical merging
let config = Config::load(Path::new("/my/project"))?;

// Access configuration
println!("Model: {}", config.model);
println!("Temperature: {:?}", config.temperature);

// Save configuration
config.save(Path::new("/my/project/.rustycode/config.json"))?;
```

#### Provider Bootstrap

**New API**:
```rust
use rustycode_providers::bootstrap_from_env;

// Auto-discover providers
let registry = bootstrap_from_env().await;

// List available providers
for provider_id in registry.list_providers() {
    println!("Provider: {}", provider_id);
}

// Get cost tracking
let costs = registry.get_cost_summary();
println!("Total cost: ${}", costs.total_cost);
```

#### Session Management

**New API**:
```rust
use rustycode_session::{Session, MessageV2, CompactionStrategy};

let mut session = Session::new("My Session");

// Add rich messages
session.add_message(MessageV2::user("Hello".to_string()));
session.add_message(MessageV2::assistant_with_reasoning(
    "The answer is 42",
    "I calculated this..."
));

// Track context
session.touch_file("src/main.rs");
session.record_decision("Use async pattern");

// Compact when needed
session.compact(
    CompactionStrategy::TokenThreshold { target_ratio: 0.5 }
).await?;
```

#### Agent Orchestration

**New API**:
```rust
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentOrchestrator, MultiAgentConfig};

let config = MultiAgentConfig {
    roles: vec![
        AgentRole::SecurityExpert,
        AgentRole::SeniorEngineer,
    ],
    content: code.to_string(),
    ..MultiAgentConfig::default()
};

let orchestrator = MultiAgentOrchestrator::from_config(config)?;
let analysis = orchestrator.analyze().await?;
```

#### MCP Integration

**New API**:
```rust
use rustycode_mcp::McpClient;

let mut client = McpClient::default();

// Connect to server
client.connect_stdio(
    "filesystem",
    "mcp-filesystem-server",
    &[]
).await?;

// Call tool
let result = client.call_tool(
    "read_file",
    serde_json::json!({"path": "/path/to/file"})
).await?;
```

### Configuration Changes

#### Provider Configuration

**Old**:
```toml
[ai]
provider = "anthropic"
api_key = "sk-ant-..."
```

**New**:
```json
{
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}"
    }
  }
}
```

**Migration Steps**:
1. Export API key to environment:
   ```bash
   export ANTHROPIC_API_KEY=sk-ant-...
   ```

2. Update config:
   ```json
   {
     "providers": {
       "anthropic": {
         "api_key": "{env:ANTHROPIC_API_KEY}"
       }
     }
   }
   ```

#### Model Configuration

**Old**:
```toml
[ai]
model = "claude-3-5-sonnet"
```

**New**:
```json
{
  "model": "claude-3-5-sonnet-latest"
}
```

**Migration Steps**:
1. Update model name to include version suffix
2. Move to top-level config field

#### Feature Flags

**Old**:
```toml
[features]
git = true
```

**New**:
```json
{
  "features": {
    "git_integration": true
  }
}
```

**Migration Steps**:
1. Rename feature flags to use underscores
2. Nest under `features` object

## Migration Steps

### Phase 1: Update Configuration

1. **Export API keys to environment**:
```bash
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
export OPENROUTER_API_KEY=sk-or-...
```

2. **⚠️ TOML Configuration No Longer Supported**:
```bash
# If you have old TOML config, it must be converted to JSON
# TOML files are no longer read by RustyCode

# Check if you have old TOML config
ls ~/.rustycode/config.toml 2>/dev/null && echo "Found TOML config - needs conversion"

# Create new JSON config (see structure below)
```

**Note:** If you have `config.toml`, you must manually convert it to `config.json` format. TOML parsing has been removed from the codebase.

3. **Update config structure**:
```jsonc
{
  // Model selection
  "model": "claude-3-5-sonnet-latest",
  "temperature": 0.1,
  "max_tokens": 4096,

  // Provider configuration
  "providers": {
    "anthropic": {
      "api_key": "{env:ANTHROPIC_API_KEY}",
      "models": ["claude-3-5-sonnet-latest", "claude-opus-4-6"]
    }
  },

  // Feature flags
  "features": {
    "git_integration": true,
    "mcp_servers": ["filesystem", "git"]
  }
}
```

4. **Verify configuration**:
```bash
rustycode config validate
```

### Phase 2: Update Provider Code

1. **Old provider creation**:
```rust
use rustycode_llm::create_provider_from_config;

let provider = create_provider_from_config()?;
```

2. **New provider bootstrap**:
```rust
use rustycode_providers::bootstrap_from_env;

#[tokio::main]
async fn main() {
    let registry = bootstrap_from_env().await;
    let provider = registry.get_provider("anthropic")?;

    // Use provider
    let result = provider.complete(request).await?;
}
```

3. **Update imports**:
```rust
// Old
use rustycode_llm::{LLMProvider, ProviderConfig};

// New
use rustycode_llm::LLMProvider;
use rustycode_providers::ModelRegistry;
```

### Phase 3: Migrate Session Code

1. **Old session usage**:
```rust
use rustycode_llm::ChatSession;

let mut session = ChatSession::new();
session.add_message("user", "Hello");
```

2. **New session usage**:
```rust
use rustycode_session::{Session, MessageV2};

let mut session = Session::new("My Session");
session.add_message(MessageV2::user("Hello".to_string()));

// Track context
session.touch_file("src/main.rs");
session.record_decision("Use async pattern");
```

3. **Update message creation**:
```rust
// Old
session.add_message("user", "Hello");

// New
use rustycode_session::{MessageV2, MessagePart};

let msg = MessageV2::new(
    MessageRole::User,
    vec![MessagePart::text("Hello")]
);

session.add_message(msg);
```

4. **Add compaction**:
```rust
use rustycode_session::CompactionStrategy;

if session.estimate_tokens() > 100_000 {
    session.compact(
        CompactionStrategy::TokenThreshold { target_ratio: 0.5 }
    ).await?;
}
```

### Phase 4: Adapt to New Agent System

1. **Identify code that can use agents**:
```rust
// Old: Direct LLM calls for analysis
let analysis = provider.complete(request).await?;
```

2. **Use specialized agents**:
```rust
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentOrchestrator, MultiAgentConfig};

let config = MultiAgentConfig {
    roles: vec![AgentRole::SecurityExpert],
    content: code.to_string(),
    ..MultiAgentConfig::default()
};

let orchestrator = MultiAgentOrchestrator::from_config(config)?;
let analysis = orchestrator.analyze().await?;
```

3. **Use multi-agent for comprehensive analysis**:
```rust
let config = MultiAgentConfig {
    roles: vec![
        AgentRole::SecurityExpert,
        AgentRole::SeniorEngineer,
        AgentRole::PerformanceAnalyst,
    ],
    content: code.to_string(),
    max_parallelism: 3,
    ..MultiAgentConfig::default()
};
```

### Phase 5: Integrate MCP (Optional)

1. **Identify tool usage**:
```rust
// Old: Direct tool execution
let result = execute_tool("read_file", args)?;
```

2. **Use MCP for tools**:
```rust
use rustycode_mcp::McpClient;

let mut client = McpClient::default();
client.connect_stdio("filesystem", "mcp-filesystem-server", &[]).await?;

let result = client.call_tool("read_file", args).await?;
```

3. **Configure MCP servers**:
```json
{
  "features": {
    "mcp_servers": ["filesystem", "git"]
  }
}
```

## Compatibility

### Backward Compatibility

The new system maintains limited backward compatibility:

- **Old config files**: Can be converted to new format
- **API keys**: Can be in config or environment (with warning)
- **Session files**: Can be migrated to new format

### Deprecation Timeline

| Version | Features | Status |
|---------|----------|--------|
| v0.1.0 | Old system | Deprecated |
| v0.2.0 | New system (alpha) | Testing |
| v0.3.0 | New system (beta) | Stable |
| v0.4.0 | Old system removed | Breaking change |

### Migration Tools

#### Config Converter

```bash
# Convert old TOML config to new JSONC
rustycode migrate config

# Convert specific file
rustycode migrate config --input old.toml --output new.json
```

#### Session Migrator

```bash
# Migrate old sessions to new format
rustycode migrate sessions

# Migrate specific session
rustycode migrate sessions --session-id sess_123
```

#### Validation Tool

```bash
# Validate new configuration
rustycode config validate

# Check migration readiness
rustycode migrate check
```

## Testing

### Test Migration

1. **Backup current setup**:
```bash
cp -r .rustycode .rustycode.backup
```

2. **Test new configuration**:
```bash
rustycode config validate
rustycode config show
```

3. **Test provider bootstrap**:
```bash
rustycode providers list
rustycode providers test anthropic
```

4. **Test session migration**:
```bash
rustycode sessions migrate
rustycode sessions list
```

### Rollback Plan

If migration fails:

1. **Restore backup**:
```bash
rm -rf .rustycode
cp -r .rustycode.backup .rustycode
```

2. **Revert code changes**:
```bash
git checkout HEAD -- Cargo.toml src/
```

3. **Report issues**:
```bash
rustycode migrate report
```

## Common Issues

### Issue: Config Not Loading

**Problem**: New config format not recognized

**Solution**:
```bash
# Check config syntax
rustycode config validate

# Show effective config
rustycode config show

# Convert old config
rustycode migrate config
```

### Issue: Provider Not Found

**Problem**: Provider not available after bootstrap

**Solution**:
```bash
# Check environment variables
echo $ANTHROPIC_API_KEY

# List available providers
rustycode providers list

# Test provider connection
rustycode providers test anthropic
```

### Issue: Session Migration Fails

**Problem**: Cannot load old sessions

**Solution**:
```bash
# Migrate sessions
rustycode sessions migrate

# Check session integrity
rustycode sessions check

# Backup and recreate
rustycode sessions backup
rustycode sessions clear
```

### Issue: Agent Execution Fails

**Problem**: Agents not working

**Solution**:
```bash
# Check agent availability
rustycode agents list

# Test agent
rustycode agents test code-reviewer

# Check provider
rustycode providers list
```

## Best Practices

### Migration Strategy

1. **Test in development first**:
   - Create dev environment
   - Test all features
   - Verify functionality

2. **Migrate incrementally**:
   - Start with configuration
   - Then providers
   - Then sessions
   - Finally agents

3. **Monitor costs**:
   - Track token usage
   - Compare with old system
   - Optimize as needed

4. **Document changes**:
   - Note breaking changes
   - Record workarounds
   - Share with ensemble

### Validation Checklist

- [ ] Configuration loads correctly
- [ ] Providers are accessible
- [ ] Sessions can be created/loaded
- [ ] Agents execute successfully
- [ ] MCP servers connect
- [ ] Tools work as expected
- [ ] Costs are tracked
- [ ] Performance is acceptable

## Conclusion

The new RustyCode architecture provides:
- **Better modularity**: Clear separation of concerns
- **Enhanced flexibility**: Multiple providers and agents
- **Improved performance**: Smart compaction and caching
- **Greater extensibility**: MCP integration and custom agents

Migration requires:
- **Configuration updates**: JSONC format and environment variables
- **Code changes**: New APIs and patterns
- **Testing**: Verify all functionality works
- **Patience**: Take time to understand new system

For more information, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Configuration Guide](CONFIGURATION.md)
- [Provider Guide](PROVIDERS.md)
- [Agent System Guide](AGENTS.md)
- [MCP Integration Guide](MCP.md)
- [Session Management Guide](SESSIONS.md)
