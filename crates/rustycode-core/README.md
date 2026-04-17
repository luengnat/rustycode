# RustyCode Core

Core runtime and execution logic for the RustyCode AI coding assistant.

## Features

- **Plan Validation**: Pre-execution validation to prevent failures
- **Context Management**: Budget-aware context assembly and prioritization
- **Error Recovery**: Intelligent error classification and recovery strategies
- **Step Execution**: Orchestration of plan step execution with error handling
- **Event Publishing**: Integration with the event bus for observability

## Modules

- `validation`: Plan validation before execution
- `context`: Context assembly and budget management
- `error`: Error classification and recovery
- `workflow`: Step execution orchestration

## Usage

```rust
use rustycode_core::validation::validate_plan;
```
