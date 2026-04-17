# RustyCode Documentation

## Directory Structure

| Directory | Contents |
|-----------|----------|
| [architecture/](architecture/) | System design — crate relationships, event bus, request dedup, extensibility |
| [architecture-upgrade/](architecture-upgrade/) | Future architecture plans — detailed design docs for next-gen systems |
| [orchestra/](orchestra/) | Autonomous Mode autonomous development — architecture, workflow, commands, prompts |
| [guides/](guides/) | User guides — quickstart, tutorial, troubleshooting, benchmarking |
| [reference/](reference/) | API reference, tool specs, coding standards, migration guide, CI/CD |
| [design/](design/) | Design proposals — enhanced agents, semantic search, event bus, TUI redesign |
| [adr/](adr/) | Architecture Decision Records |
| [diagrams/](diagrams/) | Architecture and flow diagrams |
| [security/](security/) | Security model and threat analysis |

## Quick Navigation

### Getting Started
1. [Quickstart](guides/QUICKSTART.md)
2. [Tutorial](guides/TUTORIAL.md)
3. [Developer Guide](guides/developer-guide.md)

### Autonomous Mode (Autonomous Development)
1. [Orchestra Architecture](orchestra/orchestra-architecture.md) — source of truth for runtime kernel
2. [Orchestra File Structure](orchestra/orchestra-file-structure.md) — `.orchestra/` layout
3. [Orchestra Workflow](orchestra/orchestra-workflow.md) — end-to-end workflow
4. [Orchestra Implementation](orchestra/orchestra-implementation.md) — module map and priorities
5. [Orchestra Prompts](orchestra/orchestra-prompts.md) — prompt strategy
6. [Orchestra Commands](orchestra/orchestra-commands.md) — command surface

### Architecture
- [System Architecture](architecture/architecture.md)
- [Event Bus Integration](architecture/EVENT_BUS_INTEGRATION.md)
- [Extensibility Architecture](architecture/EXTENSIBILITY_ARCHITECTURE.md)
- [Request Deduplication](architecture/REQUEST_DEDUPLICATION.md)
- [Agent Lifetime Visualization](architecture/AGENT_LIFETIME_VISUALIZATION.md)

### Reference
- [API Reference](reference/api-reference.md)
- [Tool Interface Spec](reference/TOOL_INTERFACE_SPEC.md)
- [Tool Permissions](reference/TOOL_PERMISSIONS.md)
- [Coding Standards](reference/coding-standards.md)
- [CI/CD Reference](reference/ci-cd-reference.md)
- [Sortable IDs](reference/sortable-ids.md)
- [Codebase Review & Cleanup Log](reference/CODEBASE_REVIEW_AND_CLEANUP.md)
