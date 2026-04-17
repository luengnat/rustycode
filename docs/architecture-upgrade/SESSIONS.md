# Session Management Guide

## Session Concepts

A **Session** represents a conversation with an AI assistant, including all messages, metadata, and context. Sessions enable:

- **Context Retention**: Remember conversation history
- **Token Management**: Track and optimize token usage
- **Cost Tracking**: Monitor API costs across conversations
- **Persistence**: Save and resume conversations
- **Compaction**: Reduce token usage while preserving context

### Session Lifecycle

```
1. Create Session
   └─ Generate unique ID
   └─ Initialize metadata

2. Add Messages
   └─ User messages
   └─ Assistant responses
   └─ Tool calls and results

3. Track Metadata
   └─ Token counts
   └─ Costs
   └─ Files touched
   └─ Decisions made

4. Compact (when needed)
   └─ Summarize old messages
   └─ Keep recent context
   └─ Reduce token usage

5. Serialize/Deserialize
   └─ Save to disk
   └─ Load from disk
   └─ Compress for storage

6. Archive/Delete
   └─ Mark as completed
   └─ Clean up old sessions
```

### Message Types

```rust
use rustycode_session::{MessageV2, MessageRole};

// Text message
let text_msg = MessageV2::user("Hello, world!".to_string());

// Tool call
let tool_msg = MessageV2::tool_call(
    "read_file",
    serde_json::json!({"path": "/path/to/file"}),
    "call_123"
);

// Image
let image_msg = MessageV2::user_with_image(
    "What's in this image?",
    "/path/to/image.png"
);

// Reasoning
let reasoning_msg = MessageV2::assistant_with_reasoning(
    "The answer is 42",
    "I calculated this by..."
);

// Code
let code_msg = MessageV2::assistant_with_code(
    "Here's the solution",
    "fn main() { println!(\"Hello\"); }",
    "rust"
);
```

### Session Context

```rust
use rustycode_session::{Session, SessionContext};

let mut session = Session::new("My Session");

// Track task
session.set_task("Implement feature X");

// Track files
session.touch_file("src/main.rs");
session.touch_file("src/lib.rs");

// Track decisions
session.record_decision("Use async pattern");
session.record_decision("Add error handling");

// Track error resolutions
session.record_error_resolution(
    "Null pointer error",
    "Added null check"
);

// Track phase
session.set_phase("Implementation");
```

## Compaction Strategies

### Token Threshold Compaction

Compact when session exceeds a token threshold:

```rust
use rustycode_session::{Session, CompactionStrategy};

let mut session = Session::new("My Session");

// Add many messages
for i in 0..100 {
    session.add_message(MessageV2::user(format!("Message {}", i)));
}

// Compact if token count exceeds threshold
if session.estimate_tokens() > 10_000 {
    session.compact(
        CompactionStrategy::TokenThreshold { target_ratio: 0.5 }
    ).await?;
}
```

### Message Age Compaction

Remove messages older than a certain age:

```rust
use rustycode_session::{Session, CompactionStrategy};
use chrono::{Utc, Duration};

let mut session = Session::new("My Session");

// Compact messages older than 1 hour
session.compact(
    CompactionStrategy::MessageAge {
        max_age: Utc::now() - Duration::hours(1)
    }
).await?;
```

### Semantic Importance Compaction

Keep important messages, summarize others:

```rust
use rustycode_session::{Session, CompactionStrategy};

let mut session = Session::new("My Session");

// Compact based on semantic importance
session.compact(
    CompactionStrategy::SemanticImportance {
        keep_count: 20,  // Keep 20 most important messages
        summarize_rest: true
    }
).await?;
```

### Custom Compaction

Implement custom compaction logic:

```rust
use rustycode_session::{Session, CompactionStrategy};

let mut session = Session::new("My Session");

// Custom compaction: keep only user messages
session.compact(
    CompactionStrategy::Custom(Box::new(|messages| {
        messages.into_iter()
            .filter(|m| m.role == MessageRole::User)
            .collect()
    }))
).await?;
```

## Serialization

### JSON Format

Serialize sessions to JSON:

```rust
use rustycode_session::{Session, SessionSerializer};

let session = Session::new("My Session");
let serializer = SessionSerializer::new();

// Serialize to JSON
let json = serializer.to_json(&session)?;
println!("{}", json);

// Deserialize from JSON
let loaded = serializer.from_json(&json)?;
```

### Binary Format

Serialize sessions to binary format (more efficient):

```rust
use rustycode_session::{Session, SessionSerializer, SerializationFormat};

let session = Session::new("My Session");
let mut serializer = SessionSerializer::new();

// Serialize to binary
let binary = serializer.to_binary(&session)?;

// Deserialize from binary
let loaded = serializer.from_binary(&binary)?;
```

### Compression

Use compression to reduce storage size:

```rust
use rustycode_session::{Session, SessionSerializer, SerializationFormat};

let session = Session::new("My Session");
let serializer = SessionSerializer::with_compression();

// Serialize with compression
let compressed = serializer.to_compressed(&session)?;

// Check compression ratio
let original_size = serializer.to_json(&session)?.len();
let compressed_size = compressed.len();
let ratio = compressed_size as f64 / original_size as f64;

println!("Compression ratio: {:.2}%", ratio * 100.0);
```

### Save/Load Operations

```rust
use rustycode_session::{Session, SessionSerializer};
use std::path::Path;

let session = Session::new("My Session");
let serializer = SessionSerializer::new();

// Save to file
serializer.save(&session, Path::new("session.json"))?;

// Load from file
let loaded = serializer.load(Path::new("session.json"))?;
```

## Usage Examples

### Basic Session

```rust
use rustycode_session::{Session, MessageV2, MessageRole};

let mut session = Session::new("My Session");

// Add messages
session.add_message(MessageV2::user("Hello!".to_string()));
session.add_message(MessageV2::assistant("Hi! How can I help?".to_string()));

// Get message count
println!("Messages: {}", session.message_count());

// Get token count
println!("Tokens: {}", session.estimate_tokens());
```

### Session with Metadata

```rust
use rustycode_session::{Session, MessageV2};
use std::path::PathBuf;

let mut session = Session::new("Code Review");

// Set metadata
session.metadata.project_path = Some(PathBuf::from("/my/project"));
session.metadata.git_branch = Some("main".to_string());
session.metadata.model_used = Some("claude-3-5-sonnet".to_string());

// Add message with cost tracking
let msg = MessageV2::user("Review this code".to_string())
    .with_tokens(100)
    .with_cost(0.003);

session.add_message(msg);

// Check totals
println!("Total tokens: {}", session.metadata.total_tokens);
println!("Total cost: ${}", session.metadata.total_cost);
```

### Session with Context

```rust
use rustycode_session::Session;

let mut session = Session::new("Feature Implementation");

// Set context
session.set_task("Implement user authentication");
session.set_phase("Planning");
session.touch_file("src/auth.rs");
session.record_decision("Use JWT tokens");
session.record_error_resolution(
    "Database connection failed",
    "Added retry logic"
);

// Add tag
session.add_tag("security");
session.add_tag("authentication");

// Access context
println!("Task: {:?}", session.context.task);
println!("Files: {:?}", session.context.files_touched);
println!("Phase: {:?}", session.context.current_phase);
```

### Session Forking

```rust
use rustycode_session::Session;

let mut session = Session::new("Original");

session.add_message(MessageV2::user("Hello".to_string()));
session.touch_file("src/main.rs");

// Fork session
let forked = session.fork();

// Fork has new ID but same content
assert_ne!(session.id, forked.id);
assert_eq!(session.message_count(), forked.message_count());

// Fork is independent
forked.add_message(MessageV2::user("Different message".to_string()));
assert_eq!(session.message_count(), 1);
assert_eq!(forked.message_count(), 2);
```

### Session Compaction

```rust
use rustycode_session::{Session, CompactionStrategy};

let mut session = Session::new("Long Conversation");

// Add many messages
for i in 0..1000 {
    session.add_message(MessageV2::user(format!("Message {}", i)));
    session.add_message(MessageV2::assistant(format!("Response {}", i)));
}

println!("Before: {} messages", session.message_count());
println!("Tokens: {}", session.estimate_tokens());

// Compact to 50% of original size
session.compact(
    CompactionStrategy::TokenThreshold { target_ratio: 0.5 }
).await?;

println!("After: {} messages", session.message_count());
println!("Tokens: {}", session.estimate_tokens());
```

## Best Practices

### When to Compact

1. **Before API calls**: Ensure session fits within context limits
2. **After long conversations**: Reduce token usage
3. **Periodically**: Prevent sessions from growing too large
4. **When switching tasks**: Focus on relevant context

```rust
use rustycode_session::{Session, CompactionStrategy};

let mut session = Session::new("My Session");

// Compact before API call if needed
if session.estimate_tokens() > 100_000 {
    session.compact(
        CompactionStrategy::TokenThreshold { target_ratio: 0.5 }
    ).await?;
}

// Now safe to make API call
```

### Choosing Strategies

| Strategy | Best For | Trade-offs |
|----------|----------|------------|
| Token Threshold | General use | Simple, predictable |
| Message Age | Time-sensitive conversations | May lose old context |
| Semantic Importance | Complex conversations | Slower, requires LLM |
| Custom | Specific needs | Most flexible |

### Token Estimation

```rust
use rustycode_session::Session;

let session = Session::new("My Session");

// Estimate tokens (approximate)
let estimated = session.estimate_tokens();

// Actual tokens (if tracked)
let actual = session.metadata.total_tokens;

// Use estimation when actual not available
let tokens = if actual > 0 { actual } else { estimated };
```

### Performance Tips

1. **Batch message additions**:
```rust
// Instead of:
for msg in messages {
    session.add_message(msg);
}

// Use:
session.messages.extend(messages);
session.updated_at = std::time::SystemTime::now();
```

2. **Use binary format** for large sessions:
```rust
// More efficient than JSON
let binary = serializer.to_binary(&session)?;
```

3. **Enable compression** for storage:
```rust
let serializer = SessionSerializer::with_compression();
```

## Troubleshooting

### Session Too Large

**Problem**: Session exceeds context window

**Solutions**:
```rust
// Check size
let tokens = session.estimate_tokens();
println!("Session size: {} tokens", tokens);

// Compact aggressively
session.compact(
    CompactionStrategy::TokenThreshold { target_ratio: 0.3 }
).await?;

// Or start fresh
let mut new_session = session.fork();
new_session.clear();
```

### Compaction Removes Important Context

**Problem**: Compaction removes crucial information

**Solutions**:
```rust
// Use semantic importance to keep important messages
session.compact(
    CompactionStrategy::SemanticImportance {
        keep_count: 50,
        summarize_rest: true
    }
).await?;

// Or manually preserve important messages
let important = session.messages.iter()
    .filter(|m| is_important(m))
    .cloned()
    .collect();

session.messages = important;
```

### Serialization Fails

**Problem**: Cannot serialize session

**Solutions**:
```rust
// Check for unsupported types
// Ensure all data is serializable

// Use JSON format for debugging
let json = serializer.to_json(&session)?;

// Check JSON size
println!("JSON size: {} bytes", json.len());

// Use compression for large sessions
let serializer = SessionSerializer::with_compression();
```

### Poor Compression Ratio

**Problem**: Compression doesn't reduce size much

**Solutions**:
```rust
// Check if data is already compressed
// Remove redundant data

// Compact before compressing
session.compact(
    CompactionStrategy::TokenThreshold { target_ratio: 0.5 }
).await?;

// Then compress
let compressed = serializer.to_compressed(&session)?;
```

## Advanced Topics

### Custom Message Types

```rust
use rustycode_session::{MessageV2, MessagePart};

let msg = MessageV2::new(
    MessageRole::Assistant,
    vec![
        MessagePart::text("Here's the result:"),
        MessagePart::code("fn main() {}", "rust"),
        MessagePart::image("/path/to/image.png"),
    ]
);
```

### Session Filtering

```rust
use rustycode_session::Session;

let session = Session::new("My Session");

// Filter by role
let user_msgs = session.user_messages();
let assistant_msgs = session.assistant_messages();
let tool_msgs = session.tool_messages();

// Filter by date
let recent = session.messages.iter()
    .filter(|m| {
        m.metadata.timestamp > some_cutoff
    })
    .collect::<Vec<_>>();
```

### Session Statistics

```rust
use rustycode_session::Session;

fn print_session_stats(session: &Session) {
    println!("=== Session Statistics ===");
    println!("ID: {}", session.id);
    println!("Name: {}", session.name);
    println!("Messages: {}", session.message_count());
    println!("Tokens: {}", session.estimate_tokens());
    println!("Cost: ${}", session.metadata.total_cost);
    println!("Files touched: {}", session.context.files_touched.len());
    println!("Decisions: {}", session.context.decisions.len());
    println!("Errors resolved: {}", session.context.errors_resolved.len());
}
```

## Conclusion

The RustyCode session management system is designed to be:
- **Efficient**: Smart compaction and compression
- **Flexible**: Multiple strategies for different use cases
- **Transparent**: Clear token and cost tracking
- **Production-Ready**: Robust serialization and error handling

For more information, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Provider Guide](PROVIDERS.md)
- [Agent System Guide](AGENTS.md)
