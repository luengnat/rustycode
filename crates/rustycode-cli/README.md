# RustyCode CLI

The command-line interface for RustyCode, an AI-powered coding assistant.

## Features

- **Interactive TUI**: Terminal-based user interface for coding sessions
- **Multi-Provider Support**: Works with Anthropic, OpenAI, OpenRouter, and more
- **Tool Execution**: Safe execution of code editing, file operations, and shell commands
- **Session Management**: Persistent conversations with history
- **Worktree Support**: Git worktree-based project isolation

## Usage

```bash
# Start interactive session
cargo run --bin rustycode-cli

# Or build and run
cd crates/rustycode-cli && cargo run
```

## Dependencies

- rustycode-core
- rustycode-runtime
- rustycode-tui
- rustycode-llm
- rustycode-tools

## License

MIT
