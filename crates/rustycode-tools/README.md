# rustycode-tools

Tool execution framework for RustyCode, providing safe, efficient tool invocation with comprehensive metadata tracking.

## Overview

`rustycode-tools` provides a pluggable tool system for AI code assistance, supporting:

- **File Operations** - Read, write, list directories with filtering
- **Search** - Grep and glob with statistics
- **Command Execution** - Safe bash execution with timing
- **Web Fetch** - HTTP requests with full metadata
- **Version Control** - Git operations
- **LSP Integration** - Language server protocol tools
- **Plugin System** - Extensible tool architecture

## Features

### File Operations

```rust
use rustycode_tools::{ToolExecutor, ReadFileTool, WriteFileTool, ListDirTool};
use serde_json::json;

let executor = ToolExecutor::new(std::path::PathBuf::from("/project"));

// Read a file
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({"path": "src/main.rs"}),
};
let result = executor.execute(&call);

// Write a file
let call = ToolCall {
    call_id: "2".to_string(),
    name: "write_file".to_string(),
    arguments: json!({
        "path": "src/new.rs",
        "content": "fn main() {}"
    }),
};

// List directory with filtering
let call = ToolCall {
    call_id: "3".to_string(),
    name: "list_dir".to_string(),
    arguments: json!({
        "path": "src",
        "recursive": true,
        "filter": ".rs"
    }),
};
```

### Search Operations

```rust
// Grep with statistics
let call = ToolCall {
    name: "grep".to_string(),
    arguments: json!({
        "pattern": r"async\s+fn",
        "path": "src"
    }),
};
// Returns metadata with: files_with_matches, top_files

// Glob with extension breakdown
let call = ToolCall {
    name: "glob".to_string(),
    arguments: json!({
        "pattern": "**/*.rs"
    }),
};
// Returns metadata with: extensions array
```

### Command Execution

```rust
// Bash with timing
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "cargo test",
        "timeout_secs": 60
    }),
};
// Returns metadata with: execution_time_ms, exit_code, failed
```

### Web Fetch

```rust
// HTTP with full metadata
let call = ToolCall {
    name: "web_fetch".to_string(),
    arguments: json!({
        "url": "https://example.com"
    }),
};
// Returns metadata with: status_code, headers, timing
```

## Tool Metadata

All tools return structured metadata alongside their output:

| Field | Description |
|-------|-------------|
| `truncated` | Whether output was truncated |
| `total_*` | Total counts (lines, bytes, matches) |
| `shown_*` | Counts actually shown |
| `execution_time_ms` | Execution duration |
| `content_hash` | SHA-256 for caching |

See [TOOL_METADATA.md](docs/TOOL_METADATA.md) for complete reference.

## Available Tools

| Tool | Permission | Description |
|------|------------|-------------|
| `read_file` | Read | Read text files with optional line ranges |
| `write_file` | Write | Write files with line count tracking |
| `list_dir` | Read | List directories with filtering |
| `grep` | Read | Search with regex and statistics |
| `glob` | Read | Pattern matching with extension stats |
| `bash` | Execute | Execute shell commands with timing |
| `web_fetch` | Network | Fetch URLs with response metadata |
| `git_status` | Read | Git status information |
| `git_diff` | Read | Git diff with stats |
| `git_log` | Read | Git commit history |
| `git_commit` | Write | Create commits |
| `lsp_diagnostics` | Read | LSP diagnostics |
| `lsp_hover` | Read | LSP hover information |
| `lsp_definition` | Read | Go to definition |
| `lsp_completion` | Read | Code completion |

## Recent Enhancements

See [TOOL_ENHANCEMENTS.md](docs/TOOL_ENHANCEMENTS.md) for details on:

- File type filtering in `list_dir`
- Line count tracking in `write_file`
- Binary file detection (70+ extensions)
- Execution time tracking
- Enhanced error messages
- Web fetch response headers
- SHA-256 content hashing
- Usage statistics

## Security

- **Path Validation** - All paths validated within workspace
- **Symlink Protection** - Symbolic links blocked
- **Binary Detection** - Binary files blocked with hints
- **Rate Limiting** - Configurable rate limits
- **Permission Levels** - Read/Write/Execute/Network

## Usage Tracking

Track tool usage for analytics and optimization:

```rust
use rustycode_tools::UsageTracker;

let mut tracker = UsageTracker::new();
tracker.record_use("read_file");
tracker.record_use("grep");

let stats = tracker.get_statistics();
// Returns: [("read_file", 1, Some(timestamp)), ("grep", 1, Some(timestamp))]

let total = tracker.total_uses();    // 2
let unique = tracker.unique_tools(); // 2
```

## LLM Integration

rustycode-tools is designed for seamless LLM integration:

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;

// Executor converts tool calls to LLM-compatible responses
let executor = ToolExecutor::new(workspace);

// LLM makes tool call
let llm_tool_call = ToolCall {
    call_id: "call_123".to_string(),
    name: "grep".to_string(),
    arguments: json!({
        "pattern": r"async\s+fn\s+\w+",
        "path": "src"
    }),
};

// Execute and get LLM-formatted response
let response = executor.execute(&llm_tool_call)?;

// Response includes:
// - output: Text output for LLM context
// - structured: Metadata for LLM reasoning
// - error: Error information if failed
```

**Key Integration Features:**
- **Structured Metadata** - All tools return JSON metadata for LLM reasoning
- **Error Context** - Detailed error messages with recovery suggestions
- **Performance Tracking** - Execution timing for optimization decisions
- **Content Hashing** - SHA-256 hashes for caching decisions

See [docs/INTEGRATION.md](docs/INTEGRATION.md) for complete LLM provider integration guide.

## Performance

rustycode-tools is optimized for production use:

**Benchmarks (Rust 1.85, Apple M3):**
- File read (1000 lines): ~50μs
- Grep (1000 files): ~15ms
- Directory listing (1000 files): ~2ms
- Bash execution (simple): ~500μs overhead

**Performance Features:**
- **Compile-Time Tools** - Zero-cost dispatch for hot paths
- **Regex Optimization** - Pre-compiled regex patterns
- **Early Filtering** - Filter during traversal, not after
- **Smart Truncation** - Critical errors never truncated
- **Lazy Evaluation** - Only compute needed metadata

**Optimization Strategies:**
```rust
// Use compile-time tools for maximum performance
use rustycode_tools::compile_time::*;

let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("Cargo.toml"),
    start_line: None,
    end_line: None,
})?; // Zero-cost abstraction
```

See [PERFORMANCE_REPORT.md](PERFORMANCE_REPORT.md) for detailed benchmarks.

## Migration Guide

Upgrading from previous versions? See [MIGRATION.md](MIGRATION.md) for:

- Breaking changes
- New tool parameters
- Deprecated features
- Migration examples
- Version compatibility

**Key Changes in Latest Version:**
- Enhanced metadata structure
- New execution time tracking
- SHA-256 content hashing
- Improved error messages
- Extended file type detection

## Quick Start

Get started with rustycode-tools in minutes:

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

// Create executor with workspace root
let executor = ToolExecutor::new(std::path::PathBuf::from("/my/project"));

// Read a file
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({"path": "src/main.rs"}),
};
let result = executor.execute(&call)?;
println!("{}", result.output);

// Access structured metadata
if let Some(metadata) = result.structured {
    println!("Total lines: {}", metadata["total_lines"]);
    println!("Content hash: {}", metadata["content_hash"]);
}
```

**Installation:**
```toml
[dependencies]
rustycode-tools = "0.1"
rustycode-protocol = "0.1"
```

**Common Use Cases:**
- Read code: `read_file` with line ranges
- Search code: `grep` with regex patterns
- Execute commands: `bash` with timeout
- List files: `list_dir` with filtering

See [QUICKSTART_QUICK.md](QUICKSTART_QUICK.md) for a 5-minute guide, or [docs/QUICKSTART.md](docs/QUICKSTART.md) for comprehensive getting started.

## Examples

We provide 11 comprehensive examples demonstrating all features:

### Basic Examples
- **[examples/search_operations.rs](examples/search_operations.rs)** - Grep and glob search
- **[examples/web_operations.rs](examples/web_operations.rs)** - Web fetching
- **[examples/compile_time_tools.rs](examples/compile_time_tools.rs)** - Zero-cost tools
- **[examples/simple_plugin.rs](examples/simple_plugin.rs)** - Custom plugin
- **[examples/verify_tools.rs](examples/verify_tools.rs)** - Tool verification

### Advanced Examples
- **[examples/integration_patterns.rs](examples/integration_patterns.rs)** - LLM integration patterns
- **[examples/multi_tool_workflows.rs](examples/multi_tool_workflows.rs)** - Complex workflows
- **[examples/performance_optimization.rs](examples/performance_optimization.rs)** - Performance tuning
- **[examples/real_world_use_cases.rs](examples/real_world_use_cases.rs)** - Real-world scenarios
- **[examples/advanced_error_handling.rs](examples/advanced_error_handling.rs)** - Error recovery
- **[examples/advanced_plugin.rs](examples/advanced_plugin.rs)** - Advanced plugin patterns

**Examples Index:** See [examples/INDEX.md](examples/INDEX.md) for complete documentation of all examples.

**Run Examples:**
```bash
cd crates/rustycode-tools

# Basic examples
cargo run --example search_operations
cargo run --example web_operations
cargo run --example compile_time_tools

# Advanced examples
cargo run --example integration_patterns
cargo run --example multi_tool_workflows
cargo run --example performance_optimization
```

## Documentation

**Getting Started:**
- **[Quick Start Guide (5 min)](QUICKSTART_QUICK.md)** - Get up and running quickly
- [Quick Start Guide (comprehensive)](docs/QUICKSTART.md) - Detailed getting started
- [Quick Reference](docs/QUICK_REFERENCE.md) - Parameter and metadata lookup
- **[Migration Guide](MIGRATION.md)** - Upgrading from previous versions

**Core Documentation:**
- [API Reference](docs/API_REFERENCE.md) - Complete API documentation
- [Examples Guide](docs/EXAMPLES.md) - Real-world usage patterns
- [Integration Guide](docs/INTEGRATION.md) - LLM provider integration
- [Troubleshooting](docs/TROUBLESHOOTING.md) - Common issues and solutions

**Advanced:**
- [Tool Metadata](docs/TOOL_METADATA.md) - Complete metadata reference
- [Tool Enhancements](docs/TOOL_ENHANCEMENTS.md) - Recent enhancements
- [LLM Metadata Spec](docs/LLM_METADATA_SPEC.md) - LLM metadata format
- [Compile-Time Tools](COMPILE_TIME_TOOLS.md) - Compile-time tool system
- [Performance Report](PERFORMANCE_REPORT.md) - Benchmarks and optimization
- [Security Guide](SECURITY.md) - Security features and best practices

**Documentation Index:**
- [Documentation Index](docs/INDEX.md) - Complete documentation overview

**Examples:**
- [Examples Index](examples/INDEX.md) - All 11 examples with learning path

## License

MIT
