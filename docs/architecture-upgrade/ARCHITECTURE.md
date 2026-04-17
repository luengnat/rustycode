# RustyCode Architecture Overview

## System Architecture

RustyCode is a modular, extensible AI-powered development assistant built with Rust. The system is organized around a core set of crates that provide specialized functionality, enabling features like multi-provider LLM support, session management, agent orchestration, and MCP (Model Context Protocol) integration.

### High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         RustyCode System                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────┐    ┌──────────────┐    ┌─────────────┐            │
│  │     CLI     │    │     TUI      │    │  Web UI     │            │
│  │  (rustycode-│    │  (rustycode- │    │ (future)    │            │
│  │    cli)     │    │    tui)      │    │             │            │
│  └──────┬──────┘    └──────┬───────┘    └──────┬──────┘            │
│         │                  │                    │                   │
│         └──────────────────┼────────────────────┘                   │
│                            │                                        │
│                   ┌────────▼────────┐                               │
│                   │  rustycode-     │                               │
│                   │    runtime      │                               │
│                   │  (orchestration)│                               │
│                   └────────┬────────┘                               │
│                            │                                        │
│         ┌──────────────────┼──────────────────┐                    │
│         │                  │                  │                    │
│  ┌──────▼──────┐   ┌──────▼──────┐   ┌──────▼──────┐             │
│  │   Config    │   │  Providers  │   │   Session   │             │
│  │ (rustycode- │   │ (rustycode- │   │ (rustycode- │             │
│  │   config)   │   │ providers)  │   │  session)   │             │
│  └─────────────┘   └──────┬──────┘   └─────────────┘             │
│                           │                                        │
│                   ┌───────▼────────┐                               │
│                   │   rustycode-   │                               │
│                   │      llm       │                               │
│                   │  (LLM providers)│                              │
│                   └─────────────────┘                               │
│                                                                       │
│  ┌─────────────┐    ┌──────────────┐    ┌─────────────┐            │
│  │    Agents   │    │     MCP      │    │   Tools     │            │
│  │ (rustycode- │    │ (rustycode-  │    │ (rustycode- │            │
│  │   core)     │    │    mcp)      │    │   tools)    │            │
│  └─────────────┘    └──────────────┘    └─────────────┘            │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

### Data Flow Between Systems

```
User Input (CLI/TUI)
       │
       ▼
┌──────────────────┐
│  Config Loader   │ ← Load configuration (global → workspace → project)
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Provider Registry│ ← Auto-discover providers from environment
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Session Manager │ ← Create/load session with message history
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Agent Orchestrator│ ← Route to specialized agents or direct LLM call
└────────┬─────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌────────┐ ┌──────┐
│  LLM   │ │ MCP  │ ← Execute tools via MCP if needed
│Provider│ │Server│
└───┬────┘ └──┬───┘
    │         │
    └────┬────┘
         │
         ▼
┌──────────────────┐
│ Session Manager  │ ← Add response to session
└────────┬─────────┘
         │
         ▼
    User Output
```

## Key Design Decisions

### 1. **Modular Crate Organization**
- **Separation of Concerns**: Each crate has a single, well-defined responsibility
- **Independent Testing**: Crates can be tested in isolation
- **Reusable Components**: Lower-level crates (config, providers) can be used independently

### 2. **Hierarchical Configuration**
- **Global → Workspace → Project**: Configuration merges hierarchically
- **JSONC Support**: Comments and trailing commas for better UX
- **Environment Variables**: Secure API key management via `{env:VAR_NAME}` syntax
- **File References**: Include other config files with `{file:path}` syntax

### 3. **Provider Abstraction**
- **Unified Interface**: All LLM providers implement the same trait
- **Auto-Discovery**: Scan environment for API keys and bootstrap providers
- **Cost Tracking**: Built-in token counting and cost calculation
- **Multi-Provider**: Support for Anthropic, OpenAI, OpenRouter, Gemini, Ollama, and more

### 4. **Session Management**
- **Rich Message Types**: Text, images, tool calls, reasoning, code, diffs
- **Smart Compaction**: Multiple strategies to reduce token usage
- **Efficient Serialization**: Binary format with zstd compression
- **Context Tracking**: Files touched, decisions made, errors resolved

### 5. **Agent Orchestration**
- **Specialized Agents**: Pre-built agents for code review, security, testing, etc.
- **Multi-Agent Analysis**: Parallel agent execution with consensus building
- **Layered Prompts**: Provider capabilities + agent role + task context
- **Extensible**: Easy to add custom agents

### 6. **MCP Integration**
- **Full Protocol Support**: Tools, resources, prompts, and notifications
- **Async/Await**: Non-blocking operations throughout
- **Server Management**: Start, monitor, and recover MCP servers
- **Tool Proxying**: Delegate tool execution to external MCP servers

## Technology Choices

### **Rust**
- **Performance**: Zero-cost abstractions and memory safety
- **Concurrency**: Async/await with tokio for efficient I/O
- **Type Safety**: Catch errors at compile time
- **Ecosystem**: Excellent crates for serialization (serde), async (tokio), etc.

### **Tokio**
- **Async Runtime**: Industry-standard async runtime for Rust
- **Concurrency**: Efficiently handle many concurrent operations
- **Networking**: Robust support for HTTP and other protocols

### **Serde + Serde JSON**
- **Serialization**: De facto standard for Rust serialization
- **JSON Support**: First-class JSON support for config and data
- **Flexibility**: Works with many data formats

### **JSONC**
- **User-Friendly**: Comments and trailing commas in config files
- **Familiar**: JSON-like syntax with developer-friendly features
- **Parsed to JSON**: Converts to standard JSON for processing

## Crate Organization

### **Core Infrastructure Crates**

#### `rustycode-config`
Configuration management with hierarchical merging and substitution support.
- **Features**: JSONC parsing, environment variables, file references
- **Dependencies**: serde, serde_json

#### `rustycode-providers`
Provider registry with auto-discovery and cost tracking.
- **Features**: Multi-provider support, pricing data, cost tracking
- **Dependencies**: serde, tokio

#### `rustycode-session`
Session and message management with compaction and serialization.
- **Features**: Rich message types, smart compaction, efficient serialization
- **Dependencies**: serde, chrono, zstd

#### `rustycode-llm`
LLM provider implementations and abstractions.
- **Features**: Unified provider interface, streaming, function calling
- **Dependencies**: reqwest, async-trait

### **Agent & Orchestration Crates**

#### `rustycode-core`
Core agent system with orchestrator and subagent patterns.
- **Features**: Agent registry, orchestrator, multi-agent workflows
- **Dependencies**: rustycode-llm, rustycode-session

#### `rustycode-runtime`
Main runtime that coordinates all systems.
- **Features**: Multi-agent orchestration, provider management
- **Dependencies**: rustycode-core, rustycode-providers, rustycode-config

### **Integration Crates**

#### `rustycode-mcp`
Model Context Protocol implementation.
- **Features**: Client, server, tools, resources, prompts
- **Dependencies**: serde_json, tokio

#### `rustycode-tools`
Tool execution and management.
- **Features**: File operations, git integration, custom tools
- **Dependencies**: rustycode-mcp

### **User Interface Crates**

#### `rustycode-cli`
Command-line interface.
- **Features**: Interactive commands, script execution
- **Dependencies**: rustycode-runtime, clap

#### `rustycode-tui`
Terminal user interface.
- **Features**: Interactive TUI with ratatui
- **Dependencies**: rustycode-runtime, ratatui, crossterm

## Integration Patterns

### **Configuration Flow**
```
1. ConfigLoader searches for config files:
   - Global: ~/.config/rustycode/config.json
   - Workspace: .rustycode-workspace/config.json
   - Project: .rustycode/config.json

2. Parse JSONC (allowing comments, trailing commas)

3. Apply substitutions:
   - {env:VAR_NAME} → environment variable
   - {file:path} → contents of referenced file

4. Deep merge configs (project overrides workspace overrides global)

5. Validate against schema
```

### **Provider Bootstrap Flow**
```
1. Scan environment for API keys:
   - ANTHROPIC_API_KEY
   - OPENAI_API_KEY
   - OPENROUTER_API_KEY
   - GEMINI_API_KEY
   - etc.

2. For each key found, register provider with:
   - Default models
   - Pricing information
   - Capabilities (streaming, vision, etc.)

3. Return ModelRegistry with all available providers
```

### **Session Lifecycle**
```
1. Create session with unique ID
2. Add messages (user, assistant, tool)
3. Track metadata (tokens, cost, files touched)
4. Compact when token threshold reached:
   - Remove old messages
   - Summarize important context
   - Keep recent history
5. Serialize to disk with compression
6. Load and resume on restart
```

### **Agent Execution Flow**
```
1. User request received
2. Orchestrator analyzes request
3. Route to appropriate agent:
   - Code review → code-reviewer agent
   - Security check → security-expert agent
   - Or direct LLM call for simple tasks
4. Agent builds layered prompt:
   - Layer 1: Provider capabilities (tools, format)
   - Layer 2: Agent role and perspective
   - Layer 3: Task-specific context
5. Execute LLM call
6. Parse response and extract findings
7. Return structured results
```

### **MCP Tool Calling Flow**
```
1. LLM requests tool use
2. RustyCode checks if tool is available:
   - Built-in tools (rustycode-tools)
   - MCP server tools (rustycode-mcp)
3. If MCP tool:
   - Call MCP server via JSON-RPC
   - Wait for response
   - Return result to LLM
4. LLM processes tool result
5. Continue conversation
```

## Core Concepts

### **Configuration Hierarchy**
Configuration flows from global to local, with each level overriding the previous:
- **Global**: User-wide defaults
- **Workspace**: Repository-wide settings (monorepo support)
- **Project**: Specific project overrides

### **Provider Abstraction**
All LLM providers implement a common interface, enabling:
- **Easy switching**: Change providers without code changes
- **Multi-provider**: Use different providers for different tasks
- **Cost optimization**: Choose the right provider for the job

### **Session Management**
Sessions provide:
- **Context retention**: Remember conversation history
- **Token optimization**: Compact to stay within limits
- **Cost tracking**: Monitor usage across sessions
- **Persistence**: Save and resume conversations

### **Agent Orchestration**
Agents enable:
- **Specialized expertise**: Different agents for different tasks
- **Parallel execution**: Run multiple agents simultaneously
- **Consensus building**: Combine insights from multiple perspectives
- **Layered prompts**: Provider-specific instructions

### **MCP Integration**
MCP provides:
- **Tool extensibility**: Add tools via external servers
- **Resource access**: Read files, prompts, templates
- **Protocol standardization**: Common interface for tools
- **Server management**: Lifecycle and health monitoring

## Future Directions

### **Planned Enhancements**
1. **Continuous Learning**: Learn from user feedback and code patterns
2. **Advanced Agents**: More specialized agents for specific domains
3. **Performance Optimization**: Faster session compaction and serialization
4. **Web UI**: Browser-based interface for collaborative development
5. **Enterprise Features**: Ensemble collaboration, audit logs, compliance

### **Extensibility Points**
- **Custom Agents**: Add domain-specific agents
- **Custom Tools**: Implement tools for any workflow
- **Custom Providers**: Add support for new LLM providers
- **Custom Compaction**: Implement specialized compaction strategies
- **MCP Servers**: Create MCP servers for any external system

## Conclusion

RustyCode's architecture is designed to be:
- **Modular**: Easy to understand, test, and extend
- **Performant**: Efficient resource usage and fast operations
- **Flexible**: Support for multiple providers, agents, and workflows
- **User-Friendly**: Simple configuration with sensible defaults
- **Production-Ready**: Robust error handling and comprehensive testing

The system is built on solid Rust foundations, leveraging the ecosystem's strengths while maintaining a clear separation of concerns and enabling rapid iteration and extension.
