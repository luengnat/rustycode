# Session System & Data Layer - Detailed Design

## Overview

Dedicated session crate with rich message types, compaction, summarization, and persistence.

## Architecture

```
Session System Architecture:
┌─────────────────────────────────────────────────────────────┐
│ Session                                                    │
│ ├─ SessionId                                               │
│ ├─ Metadata                                                │
│ ├─ Messages (Vec<MessageV2>)                               │
│ └─ Status                                                  │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ MessageV2 (Rich Content Types)                             │
│ ├─ Text, ToolCall, ToolResult                              │
│ ├─ Reasoning, File, Image                                  │
│ ├─ Code, Diff                                              │
│ └─ Metadata                                                │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ Session Operations                                         │
│ ├─ Compaction (token reduction)                            │
│ ├─ Summarization (content summary)                         │
│ ├─ Revert (undo/redo)                                      │
│ └─ Fork/Continue (branching)                               │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ Repository Layer                                           │
│ ├─ SessionRepository                                       │
│ ├─ MessageRepository                                       │
│ └─ Transaction Support                                     │
└─────────────────────────────────────────────────────────────┘
```

## Data Structures

### Session

```rust
// crates/rustycode-session/src/session.rs

use crate::message_v2::MessageV2;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub messages: Vec<MessageV2>,
    pub metadata: SessionMetadata,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    pub fn new() -> Self {
        Self(format!("sess_{}", nanoid::nanoid!(10)))
    }
}

#[derive(Debug, Clone)]
pub struct SessionMetadata {
    pub project_path: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub model_used: Option<String>,
    pub total_tokens: usize,
    pub total_cost: f64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Archived,
    Deleted,
}

impl Session {
    pub fn new(name: String) -> Self {
        Self {
            id: SessionId::new(),
            name,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            messages: Vec::new(),
            metadata: SessionMetadata::default(),
            status: SessionStatus::Active,
        }
    }

    pub fn add_message(&mut self, message: MessageV2) {
        self.messages.push(message);
        self.updated_at = SystemTime::now();
    }

    pub fn token_count(&self) -> usize {
        self.messages.iter().map(|m| m.estimate_tokens()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn clone_for_branch(&self) -> Self {
        Self {
            id: SessionId::new(),
            name: format!("{} (branch)", self.name),
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            messages: self.messages.clone(),
            metadata: self.metadata.clone(),
            status: SessionStatus::Active,
        }
    }
}
```

### MessageV2

```rust
// crates/rustycode-session/src/message_v2.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageV2 {
    pub id: String,
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
    pub timestamp: SystemTime,
    pub metadata: MessageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePart {
    Text { content: String },
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
        is_error: bool,
    },
    Reasoning { content: String },
    File {
        url: String,
        filename: String,
        mime_type: String,
    },
    Image {
        url: String,
        alt_text: Option<String>,
    },
    Code {
        language: String,
        code: String,
    },
    Diff {
        filepath: String,
        old_string: String,
        new_string: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub tokens: Option<usize>,
    pub cost: Option<f64>,
    pub model: Option<String>,
    pub cached: bool,
}

impl MessageV2 {
    pub fn user(content: String) -> Self {
        Self {
            id: nanoid::nanoid!(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text { content }],
            timestamp: SystemTime::now(),
            metadata: MessageMetadata::default(),
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            id: nanoid::nanoid!(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Text { content }],
            timestamp: SystemTime::now(),
            metadata: MessageMetadata::default(),
        }
    }

    pub fn estimate_tokens(&self) -> usize {
        self.parts.iter().map(|part| match part {
            MessagePart::Text { content } => content.len() / 4,
            MessagePart::Reasoning { content } => content.len() / 4,
            MessagePart::Code { code, .. } => code.len() / 4,
            MessagePart::Diff { old_string, new_string, .. } => {
                (old_string.len() + new_string.len()) / 4
            }
            _ => 100,
        }).sum()
    }
}
```

### Compaction

```rust
// crates/rustycode-session/src/compaction.rs

use crate::{Session, MessageV2, MessageRole};

pub struct SessionCompactor {
    target_ratio: f64,
    min_messages: usize,
}

impl SessionCompactor {
    pub fn new(target_ratio: f64) -> Self {
        Self {
            target_ratio,
            min_messages: 4,
        }
    }

    pub fn compact(&self, session: &Session) -> Result<Vec<MessageV2>, CompactionError> {
        let target_tokens = (session.token_count() as f64 * self.target_ratio) as usize;

        if session.messages.len() <= self.min_messages {
            return Ok(session.messages.clone());
        }

        let mut compacted = Vec::new();
        let mut tokens = 0;

        // Keep recent messages
        for message in session.messages.iter().rev() {
            let message_tokens = message.estimate_tokens();

            if tokens + message_tokens > target_tokens && compacted.len() >= self.min_messages {
                break;
            }

            compacted.push(message.clone());
            tokens += message_tokens;
        }

        compacted.reverse();

        // Add summary if we dropped messages
        if compacted.len() < session.messages.len() {
            let dropped_count = session.messages.len() - compacted.len();
            let summary = MessageV2::system(format!(
                "[Compacted {} previous messages to save tokens. Context preserved.]",
                dropped_count
            ));
            compacted.insert(0, summary);
        }

        Ok(compacted)
    }
}
```

### Repository Pattern

```rust
// crates/rustycode-storage/src/repositories/session.rs

use rustycode_session::{Session, SessionId};

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn find_by_id(&self, id: &SessionId) -> Result<Option<Session>>;
    async fn save(&self, session: &Session) -> Result<()>;
    async fn delete(&self, id: &SessionId) -> Result<()>;
    async fn list_all(&self) -> Result<Vec<Session>>;
}

pub struct SqliteSessionRepository {
    db: Arc<SqlitePool>,
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn find_by_id(&self, id: &SessionId) -> Result<Option<Session>> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, Option<i64>)>(
            "SELECT id, name, project_path, created_at FROM sessions WHERE id = ?"
        )
        .bind(&id.0)
        .fetch_optional(&*self.db)
        .await?;

        // ... convert row to Session
        Ok(None)
    }

    async fn save(&self, session: &Session) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, name, created_at) VALUES (?, ?, ?)"
        )
        .bind(&session.id.0)
        .bind(&session.name)
        .bind(session.created_at)
        .execute(&*self.db)
        .await?;

        Ok(())
    }
}
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let session = Session::new("Test Session".into());
        session.add_message(MessageV2::user("Hello".into()));

        assert_eq!(session.message_count(), 1);
        assert_eq!(session.token_count(), 1); // "Hello" = ~1 token
    }

    #[tokio::test]
    async fn test_compaction() {
        let session = Session::new("Test".into());

        for i in 0..10 {
            session.add_message(MessageV2::user(format!("Message {}", i)));
        }

        let compactor = SessionCompactor::new(0.5); // Keep 50%
        let compacted = compactor.compact(&session).unwrap();

        assert!(compacted.len() < 10);
        assert!(compacted.len() >= 4); // min_messages
    }
}
```

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
sqlx = { version = "0.8", features = ["sqlite"] }
nanoid = "0.4"
```
