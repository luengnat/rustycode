# RustyCode Config

Hierarchical configuration system with JSON/JSONC parsing, environment variables, and schema validation.

## Features

- **JSON/JSONC Parsing**: Support for comments and trailing commas
- **Environment Substitution**: `{env:VAR_NAME}` syntax
- **File References**: `{file:path}` syntax
- **Hierarchical Merging**: Global → workspace → project
- **Schema Validation**: Validate configuration against schemas

## Usage

```rust
use rustycode_config::{Config, load_config};
```
