# RustyCode Architecture Upgrade Documentation

This directory contains comprehensive documentation for the new RustyCode architecture, covering all major systems and their integration.

## Documentation Overview

### [ARCHITECTURE.md](ARCHITECTURE.md)
**System Architecture and Design**

High-level overview of the RustyCode system, including:
- Component diagram and data flow
- Key design decisions and technology choices
- Crate organization and responsibilities
- Integration patterns between systems

**Best for**: Understanding the big picture and how everything fits together.

---

### [CONFIGURATION.md](CONFIGURATION.md)
**Configuration System Guide**

Complete guide to RustyCode's hierarchical configuration system:
- Configuration hierarchy (global → workspace → project)
- JSONC syntax with comments and trailing commas
- Environment variable substitutions (`{env:VAR_NAME}`)
- File references (`{file:path}`)
- Directory customization with `CODEX_HOME`
- Complete configuration reference

**Best for**: Setting up and customizing RustyCode for your environment.

---

### [PROVIDERS.md](PROVIDERS.md)
**LLM Provider Guide**

Comprehensive guide to LLM provider integration:
- Supported providers (Anthropic, OpenAI, OpenRouter, Gemini, Ollama)
- Provider configuration and API key management
- Model selection and pricing information
- Cost tracking and token usage
- Extending with custom providers

**Best for**: Understanding provider options and managing costs.

---

### [AGENTS.md](AGENTS.md)
**Agent System Guide**

Detailed guide to RustyCode's agent orchestration system:
- Agent architecture and trait interface
- Built-in agents (code review, security, testing, performance)
- Single agent execution
- Parallel multi-agent analysis
- Creating custom agents
- Agent patterns and workflows

**Best for**: Leveraging agents for comprehensive code analysis.

---

### [MCP.md](MCP.md)
**MCP Integration Guide**

Complete guide to Model Context Protocol integration:
- MCP overview and benefits
- Server management (starting, health monitoring, auto-recovery)
- Tool discovery and calling
- Resource access and monitoring
- Configuration (connection pooling, rate limiting, timeouts)
- Parallel operations and error handling

**Best for**: Extending RustyCode with external tools and resources.

---

### [SESSIONS.md](SESSIONS.md)
**Session Management Guide**

In-depth guide to session and message management:
- Session concepts and lifecycle
- Rich message types (text, images, tools, code, diffs)
- Session context tracking
- Compaction strategies (token threshold, age, semantic importance)
- Serialization (JSON, binary, compression)
- Performance tips and best practices

**Best for**: Managing conversations and optimizing token usage.

---

### [MIGRATION.md](MIGRATION.md)
**Migration Guide from Old System**

Step-by-step guide for migrating from the old RustyCode system:
- Breaking changes and what's new
- Configuration format changes (TOML → JSONC)
- API changes and migration steps
- Provider system updates
- Session system migration
- Agent system adoption
- MCP integration
- Testing and validation
- Troubleshooting common issues

**Best for**: Upgrading from previous versions of RustyCode.

---

## Quick Start

### New Users

1. Start with [ARCHITECTURE.md](ARCHITECTURE.md) for system overview
2. Read [CONFIGURATION.md](CONFIGURATION.md) to set up your environment
3. Review [PROVIDERS.md](PROVIDERS.md) to understand LLM options
4. Explore [AGENTS.md](AGENTS.md) for code analysis capabilities

### Migrating Users

1. Read [MIGRATION.md](MIGRATION.md) for migration overview
2. Follow configuration migration steps
3. Update provider and session code
4. Adopt agent system for enhanced capabilities
5. Integrate MCP for external tools (optional)

### Advanced Users

1. [AGENTS.md](AGENTS.md) - Create custom agents
2. [MCP.md](MCP.md) - Build custom MCP servers
3. [SESSIONS.md](SESSIONS.md) - Implement custom compaction strategies
4. [PROVIDERS.md](PROVIDERS.md) - Add new LLM providers

## Documentation Features

### Comprehensive Coverage
- All major systems documented
- Code examples throughout
- Best practices and patterns
- Troubleshooting sections

### Practical Focus
- Real-world usage examples
- Step-by-step guides
- Common scenarios covered
- Performance optimization tips

### Up-to-Date
- Reflects current architecture
- Includes latest features
- Tested code examples
- Accurate API references

## Contributing

When updating documentation:

1. **Keep examples current**: Test all code examples
2. **Add diagrams**: Use Mermaid or ASCII for visual aids
3. **Include troubleshooting**: Add common issues and solutions
4. **Cross-reference**: Link to related documentation
5. **Version control**: Note which version changes apply to

## Support

For questions or issues:

1. Check the troubleshooting section in relevant guide
2. Review architecture documentation for system overview
3. Examine code examples for usage patterns
4. Report issues via GitHub issues

## Architecture Summary

```
┌─────────────────────────────────────────────────────────┐
│                   RustyCode System                      │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────┐ │
│  │   Config    │───▶│  Providers   │───▶│ Sessions  │ │
│  │ (hierarchy)│    │ (multi-prov) │    │(compact)  │ │
│  └─────────────┘    └──────────────┘    └───────────┘ │
│         │                    │                  │       │
│         └────────────────────┼──────────────────┘       │
│                              │                           │
│                    ┌─────────▼─────────┐                │
│                    │  Agent System     │                │
│                    │ (orchestration)   │                │
│                    └─────────┬─────────┘                │
│                              │                           │
│         ┌────────────────────┼──────────────────┐       │
│         │                    │                  │       │
│  ┌──────▼──────┐    ┌───────▼───────┐    ┌────▼────┐  │
│  │     LLM     │    │      MCP      │    │  Tools  │  │
│  │ (providers) │    │ (integration) │    │(execution)│ │
│  └─────────────┘    └───────────────┘    └─────────┘  │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

## Key Concepts

### Configuration
- **Hierarchical**: Global → Workspace → Project
- **Flexible**: JSONC with comments and substitutions
- **Secure**: Environment variables for sensitive data

### Providers
- **Multi-Provider**: Support for many LLM providers
- **Auto-Discovery**: Scan environment for API keys
- **Cost Tracking**: Monitor token usage and costs

### Sessions
- **Rich Messages**: Text, images, tools, code, diffs
- **Smart Compaction**: Reduce token usage intelligently
- **Efficient Storage**: Compressed binary format

### Agents
- **Specialized**: Pre-built agents for specific tasks
- **Parallel**: Run multiple agents simultaneously
- **Layered Prompts**: Provider + role + task context

### MCP
- **Extensible**: Add tools via external servers
- **Standardized**: Common interface for all tools
- **Managed**: Health monitoring and auto-recovery

## Version Information

- **Architecture Version**: 0.2.0
- **Last Updated**: 2025-03-16
- **Status**: Beta (Stable for production use)

---

**Documentation Maintained By**: RustyCode Ensemble
**License**: MIT
**Repository**: https://github.com/luengnat/rustycode
