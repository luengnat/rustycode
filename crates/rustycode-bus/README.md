# rustycode-bus

A type-safe, asynchronous event bus for RustyCode enabling decoupled communication between crates.

## Features

- **Type Safety**: Compile-time guarantee that event publishers and subscribers match
- **Wildcard Subscriptions**: Subscribe to event patterns (e.g., `session.*`, `git.*`, `*`)
- **Hook System**: Pre/post processing for logging, metrics, error handling
- **Async/Await**: Built on tokio for high-throughput scenarios
- **Thread-Safe**: All operations are safe to use across threads
- **Automatic Cleanup**: Subscription handles automatically unsubscribe on drop

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rustycode-bus = { path = "../rustycode-bus" }
```

## Quick Start

```rust
use rustycode_bus::{EventBus, SessionStartedEvent};
use rustycode_protocol::SessionId;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bus = EventBus::default();

    // Subscribe to session events
    let (_id, mut rx) = bus.subscribe("session.*").await?;

    // Spawn task to handle events
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            println!("Received: {}", event.event_type());
        }
    });

    // Publish an event
    let event = SessionStartedEvent::new(
        SessionId::new(),
        "Analyze codebase".to_string(),
        "Initial session".to_string(),
    );
    bus.publish(event).await?;

    Ok(())
}
```

## Wildcard Patterns

The event bus supports flexible wildcard patterns:

- `session.started` - Exact match only
- `session.*` - All session events
- `*.error` - All error events
- `*` - All events

## Hook System

Register hooks for cross-cutting concerns:

```rust
use rustycode_bus::{EventBus, HookPhase};

// Pre-publish hook
bus.register_hook(HookPhase::PrePublish, |event| {
    tracing::info!("Publishing: {}", event.event_type());
    Ok(())
}).await;

// Post-publish hook
bus.register_hook(HookPhase::PostPublish, |event| {
    metrics::counter("events", event.event_type());
    Ok(())
}).await;
```

## Event Types

The crate includes several built-in event types:

- `SessionStartedEvent` - Emitted when a new session begins
- `ContextAssembledEvent` - Emitted when context is assembled
- `ToolExecutedEvent` - Emitted when a tool is executed

## Custom Events

Create your own events by implementing the `Event` trait:

```rust
use rustycode_bus::Event;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyCustomEvent {
    pub timestamp: DateTime<Utc>,
    pub data: String,
}

impl Event for MyCustomEvent {
    fn event_type(&self) -> &'static str {
        "my.custom.event"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}
```

## Metrics

Access event bus metrics:

```rust
let metrics = bus.metrics();
println!("Events published: {}", metrics.events_published);
println!("Events delivered: {}", metrics.events_delivered);
println!("Active subscribers: {}", metrics.subscriber_count);
```

## Examples

Run the included examples:

```bash
# Basic usage
cargo run --example basic_usage --package rustycode-bus

# Wildcard matching
cargo run --example wildcard_matching --package rustycode-bus
```

## License

MIT
