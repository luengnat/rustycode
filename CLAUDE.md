# CLAUDE.md — RustyCode Development Guide

This file provides guidance to anyone (human or AI) working with the RustyCode codebase.

## Project Overview

RustyCode is an AI-powered autonomous development framework built in Rust. It provides an interactive TUI, a CLI, and an autonomous development mode (Autonomous Mode) with multi-provider LLM support.

**Repository**: https://github.com/luengnat/rustycode
**License**: MIT
**Rust Edition**: 2021
**Minimum Rust Version**: See `Cargo.toml` (MSRV not formally specified; use latest stable)

## Repository Structure

```
rustycode/
├── crates/              # 35 workspace member crates
│   ├── rustycode-cli/       # CLI binary (default workspace member)
│   ├── rustycode-tui/       # Terminal UI binary (ratatui-based)
│   ├── rustycode-core/      # Core runtime, session, headless execution
│   ├── rustycode-orchestra/    # Autonomous Mode autonomous development framework
│   ├── rustycode-llm/       # LLM provider abstractions (Anthropic, OpenAI, etc.)
│   ├── rustycode-tools/     # Tool execution framework + permissions
│   ├── rustycode-runtime/   # Async runtime, orchestration, monitoring
│   ├── rustycode-protocol/  # Shared types and protocol definitions
│   ├── rustycode-bus/       # Event bus for inter-module communication
│   ├── rustycode-config/    # Configuration loading and validation
│   ├── rustycode-storage/   # Session persistence and caching
│   ├── rustycode-git/       # Git operations and worktree management
│   ├── rustycode-lsp/       # LSP client integration
│   ├── rustycode-memory/    # Short-term memory and context management
│   ├── rustycode-vector-memory/ # Vector-based semantic memory (HNSW)
│   ├── rustycode-skill/     # Skill discovery and loading
│   ├── rustycode-ui-core/   # Shared UI components (markdown rendering, etc.)
│   ├── rustycode-mcp/       # MCP (Model Context Protocol) server/client
│   ├── rustycode-auth/      # Authentication (API keys, GitHub Copilot)
│   ├── rustycode-prompt/    # Prompt templating (Handlebars/Tera)
│   ├── rustycode-providers/ # Provider registry and discovery
│   ├── rustycode-session/   # Session lifecycle management
│   ├── rustycode-learning/  # Conversation learning and extraction
│   ├── rustycode-load/      # Load testing utilities
│   ├── rustycode-macros/    # Procedural macros
│   ├── rustycode-observability/ # Tracing and metrics
│   ├── rustycode-plugins/   # Plugin system
│   ├── rustycode-connector/ # Terminal connector abstraction (tmux, iTerm2)
│   ├── rustycode-acp/       # Agent Client Protocol
│   ├── rustycode-thread-guard/ # Thread safety utilities
│   ├── rustycode-tools-api/ # Tool trait definitions
│   ├── rustycode-shared-runtime/ # Shared tokio runtime
│   ├── rustycode-tool-server/ # Standalone tool server
│   └── rustycode-bench/     # Benchmark runner
├── docs/                # Architecture docs, guides, specs
├── scripts/             # Build, test, and utility scripts
├── tests/               # Integration tests
├── benches/             # Criterion benchmarks
├── examples/            # Usage examples
├── apps/                # External applications
├── harbor-agent/        # Agent integration
└── mcp-test-server/     # MCP test server
```

Excluded from workspace: `crates/ratzilla-wasm/`, `crates/rustycode-web/` (separate WASM build).

## Build & Run

```bash
# Build CLI (default)
cargo build --release

# Build TUI
cargo build --release -p rustycode-tui

# Build all workspace members
cargo build --workspace --all-targets

# Run CLI
cargo run -p rustycode-cli -- [args]

# Run tests
cargo test --workspace

# Run clippy (CI enforces this)
cargo clippy --workspace --all-targets -- -D warnings

# Format check
cargo fmt --check
```

## Coding Standards

### Lint Configuration

The workspace enforces strict lints via `Cargo.toml`:

- **Clippy pedantic + nursery**: Enabled as warnings
- **`unwrap_used` / `expect_used`**: Warn (use `?` or `.context()`)
- **`unsafe_code`**: Forbidden (must opt-in per crate with `#![allow(unsafe_code)]`)
- **`dead_code`**: Warn (CI will flag unused items)

Allowed lints (with documented rationale in `Cargo.toml`):
- `type_complexity`, `too_many_arguments`, `module_inception`
- `upper_case_acronyms`, `wildcard_imports`, `must_use_candidate`
- `cast_possible_truncation`, `cast_sign_loss`
- `missing_errors_doc`, `missing_panics_doc`

### Error Handling

**Use `anyhow` for application code, `thiserror` for library error types.**

```rust
use anyhow::{Context, Result};

// Always provide context for errors
fn read_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config from {}", path.display()))?;
    Ok(toml::from_str(&content)?)
}
```

For crate-level error types:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BusError {
    #[error("channel closed")]
    ChannelClosed,
    #[error("handler not found: {0}")]
    HandlerNotFound(String),
}
```

### Secrets & API Keys

**Always use `secrecy::SecretString` for API keys and tokens.** Never log or display raw secrets.

```rust
use secrecy::SecretString;

pub struct ProviderConfig {
    pub api_key: Option<SecretString>,
}
```

The `sanitize_for_log()` function in `rustycode-tools/src/security.rs` and `rustycode-orchestra/src/sanitize.rs` strips API key patterns from log output.

**Never commit real API keys.** The `.gitignore` blocks `.env`, `credentials.json`, `config.json`. The `.gitleaks.toml` config provides pre-commit secret scanning.

### Async Patterns

- Use `tokio` for all async operations
- Use `tokio::fs` over `std::fs` in async contexts
- Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared state
- Prefer `tokio::sync` primitives over `std::sync` in async code

### Module Organization

```rust
// lib.rs — re-export public API
pub mod config;
pub mod error;
pub mod types;

pub use config::Config;
pub use error::{Error, Result};
```

### Testing

- Inline `#[cfg(test)] mod tests` for unit tests within the source file
- Separate `tests/` directory for integration tests
- Use `#[tokio::test]` for async tests
- Benchmark with Criterion (`benches/`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parsing() {
        let config = Config::parse("key = \"value\"").unwrap();
        assert_eq!(config.key, "value");
    }
}
```

### Adding Dependencies

1. Add to the crate's `Cargo.toml`
2. If the dependency is shared across multiple crates, add it to the workspace `[workspace.dependencies]` section and reference it as `dep.workspace = true`
3. Prefer async-compatible crates (tokio-based)

## Architecture

### Layer Diagram

```
┌─────────────────────────────────────────────────────────────┐
│  rustycode-cli / rustycode-tui / rustycode-guard (binaries) │
├─────────────────────────────────────────────────────────────┤
│  rustycode-core (session, headless runtime)                 │
│  rustycode-orchestra (autonomous development)                  │
├─────────────────────────────────────────────────────────────┤
│  rustycode-llm (providers)  │  rustycode-tools             │
│  rustycode-bus (events)      │  rustycode-guard (security)  │
├─────────────────────────────────────────────────────────────┤
│  rustycode-protocol  │  rustycode-config  │  rustycode-skill │
│  rustycode-storage   │  rustycode-auth     │  rustycode-session│
└─────────────────────────────────────────────────────────────┘
```

### Inter-Crate Communication

- **Types**: Use types from `rustycode-protocol` for cross-crate messages
- **Events**: Use `rustycode-bus::EventBus` for pub/sub
- **Tools**: Use `rustycode-tools-api` trait definitions
- **LLM**: Use `rustycode-llm::LLMProvider` trait
- **Config**: Use `rustycode-config` for loading configuration
- **Skills**: Use `rustycode-skill` for skill discovery and YAML frontmatter

### Key Traits

| Trait | Crate | Purpose |
|-------|-------|---------|
| `LLMProvider` | `rustycode-llm` | LLM provider abstraction |
| `ToolExecutor` | `rustycode-tools-api` | Tool execution interface |
| `EventHandler` | `rustycode-bus` | Event subscription |
| `Provider` | `rustycode-llm` | Provider v2 trait |

## Security

- **Permission system**: `rustycode-tools/src/security.rs` validates all file/command operations
- **Path validation**: Blocks `.env`, `credentials.json`, and sensitive file access
- **Command validation**: `rustycode-tools/src/bash.rs` validates shell commands before execution
- **Secret sanitization**: API keys are stripped from logs and debug output
- **Pre-commit hooks**: `.pre-commit-config.yaml` runs gitleaks for secret detection

## Common Tasks

### Adding a new LLM provider

1. Create a new file in `crates/rustycode-llm/src/` (e.g., `my_provider.rs`)
2. Implement the `LLMProvider` or `Provider` trait
3. Register in `crates/rustycode-llm/src/lib.rs`
4. Add provider config in `crates/rustycode-llm/src/provider_v2.rs`
5. Add tests following existing provider test patterns

### Adding a new tool

1. Define the tool in `crates/rustycode-tools/src/`
2. Implement the tool trait from `rustycode-tools-api`
3. Register in `crates/rustycode-tools/src/lib.rs`
4. Add security validation if the tool touches files or runs commands
5. Add tests

### Adding a new crate

1. Create `crates/rustycode-newcrate/` with `Cargo.toml` and `src/lib.rs`
2. Add to workspace `members` in root `Cargo.toml` (one per line)
3. Add `lints.workspace = true` to the crate's `Cargo.toml`
4. Use workspace dependencies where possible (`dep.workspace = true`)

## CI

CI runs:
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo fmt --check
```

Pre-commit hooks (`.pre-commit-config.yaml`):
- gitleaks — secret detection
- cargo fmt
- cargo clippy
