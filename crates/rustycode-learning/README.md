# RustyCode Learning System

A pattern extraction and automatic application system that learns from coding sessions.

## Overview

The Learning System (Instincts v2) provides:

- **Pattern Extraction**: Automatically extracts reusable patterns from sessions
- **Pattern Storage**: Persistent storage for learned patterns and instincts
- **Auto-Application**: Automatically applies learned patterns when context matches
- **Learning Loop**: Continuous improvement through feedback collection
- **Built-in Patterns**: Pre-configured patterns for common Rust workflows

## Features

### Pattern Categories

- **Coding**: Common coding patterns (error handling, async/await, traits)
- **Debugging**: Compilation and runtime error handling strategies
- **Refactoring**: Code improvement patterns (extract function, naming)
- **Testing**: Test-driven development workflows
- **Documentation**: Documentation generation patterns
- **Architecture**: Design pattern implementations
- **Optimization**: Performance improvement strategies

### Built-in Patterns

The system includes 10 built-in patterns:

1. **Rust Error Handling** - Replace unwrap/expect with proper error handling
2. **Rust Async/Await** - Proper async/await usage patterns
3. **Rust Trait Implementations** - Manual and derived trait implementations
4. **Debugging Compilation Errors** - Systematic error fixing approach
5. **Debugging Runtime Errors** - Panic debugging strategies
6. **Test-Driven Development** - TDD workflow guidance
7. **Extract Function Refactoring** - Function extraction patterns
8. **Generate Rust Documentation** - Comprehensive documentation generation
9. **Performance Optimization** - Systematic optimization approach
10. **Git Workflow** - Best practices for git operations

## Usage

### Basic Setup

```rust
use rustycode_learning::{PatternStorage, InstinctExtractor, LearningLoop};

// Create storage and extractor
let storage = PatternStorage::new(&path)?;
let extractor = InstinctExtractor::new();

// Create learning loop
let mut learning_loop = LearningLoop::new(extractor, storage);

// Process a session to extract patterns
let report = learning_loop.process_session(&session).await?;
println!("Extracted {} patterns", report.patterns_extracted);
```

### Pattern Extraction

```rust
// Extract patterns from a single session
let patterns = extractor.extract_from_session(&session).await?;

// Extract patterns from multiple sessions
let patterns = extractor.extract_from_history(&sessions).await?;

// Learn a pattern manually
let pattern = Pattern::new(
    "my-pattern".to_string(),
    "My Pattern".to_string(),
    PatternCategory::Coding,
    "Description of my pattern".to_string(),
);
extractor.learn_pattern(pattern);
```

### Pattern Storage

```rust
// Add patterns and instincts
storage.add_pattern(pattern);
storage.add_instinct(instinct);

// Save to disk
storage.save()?;

// Load from disk
storage.load()?;

// Find matching instincts for a context
let context = Context::new()
    .with_text("Help me fix this error".to_string())
    .with_language("rust".to_string());

let matching = storage.find_matching_instincts(&context);
```

### Auto-Application

```rust
// Auto-apply learned patterns
let results = learning_loop.storage().auto_apply(&context).await;

for result in results {
    if result.success {
        println!("Applied: {}", result.feedback);
        for change in result.changes {
            println!("  - {}", change.description);
        }
    }
}
```

### Feedback Collection

```rust
// Collect feedback on an instinct
let feedback = Feedback {
    instinct_id: "rust-error-handling".to_string(),
    session_id: session.id.to_string(),
    was_helpful: true,
    rating: Some(0.9),
    comment: Some("Very helpful!".to_string()),
    timestamp: chrono::Utc::now(),
};

learning_loop.collect_feedback(feedback).await;

// Update patterns based on feedback
let update_report = learning_loop.update_patterns().await?;
```

## Architecture

### Components

1. **InstinctExtractor** - Extracts patterns from sessions
2. **PatternStorage** - Persists patterns and instincts
3. **TriggerMatcher** - Matches triggers to contexts
4. **LearningLoop** - Coordinates the learning process
5. **FeedbackCollector** - Collects and processes feedback

### Pattern Structure

```rust
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub category: PatternCategory,
    pub description: String,
    pub examples: Vec<String>,
    pub confidence: f32,
    pub trigger_conditions: Vec<TriggerCondition>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub usage_count: usize,
}
```

### Instinct Structure

```rust
pub struct Instinct {
    pub id: String,
    pub pattern: Pattern,
    pub trigger: TriggerCondition,
    pub action: SuggestedAction,
    pub success_rate: f32,
    pub usage_count: usize,
    pub last_used: Option<DateTime<Utc>>,
}
```

### Trigger Types

- **Keyword** - Triggered by keywords in text
- **FileType** - Triggered by file extension
- **ErrorPattern** - Triggered by error messages
- **CodePattern** - Triggered by code patterns
- **Context** - Triggered by context requirements
- **Intent** - Triggered by user intent
- **Composite** - Combines multiple conditions

## Storage

Patterns and instincts are stored in `~/.config/rustycode/instincts/patterns.json`

The storage format is JSON with automatic versioning for future compatibility.

## Testing

Run tests with:

```bash
cargo test -p rustycode-learning
```

Run tests with output:

```bash
cargo test -p rustycode-learning -- --nocapture
```

## Examples

See the tests in `src/storage.rs`, `src/learning_loop.rs`, and `src/triggers.rs` for usage examples.

## License

MIT
