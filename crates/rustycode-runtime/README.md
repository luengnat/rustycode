# RustyCode Runtime

Execution runtime for running AI coding sessions with tool execution, session management, and worktree support.

## Features

- **Session Management**: Create and manage coding sessions with history
- **Tool Execution**: Execute tools safely with timeout and error handling
- **Git Worktree**: Project isolation using git worktrees
- **Multi-Agent Support**: Coordinate multiple agents for complex tasks
- **Workflow Engine**: Execute multi-step plans with retries and fallbacks

## Usage

```rust
use rustycode_runtime::session::Session;
```
