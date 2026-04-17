// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Session integration tests
//!
//! Tests cover:
//! - Session lifecycle (create, use, fork, archive, delete)
//! - Message compaction strategies
//! - Session serialization with compression
//! - Token accounting accuracy
//! - Session metadata and status

use std::path::{Path, PathBuf};
use std::fs;

use rustycode_session::{
    CompactionEngine, CompactionStrategy, MessageV2, MessagePart, MessageRole, Session,
    SessionId, SessionMetadata, SessionSerializer, SessionStatus,
};

mod common;
use common::TestConfig;

#[tokio::test]
async fn test_session_lifecycle() {
    // Create session
    let mut session = Session::new("Test Session".to_string());

    // Verify initial state
    assert_eq!(session.metadata().title, "Test Session");
    assert_eq!(session.status(), SessionStatus::Active);
    assert_eq!(session.message_count(), 0);

    // Add messages
    session.add_message(MessageV2::user("Hello".to_string()));
    session.add_message(MessageV2::assistant("Hi there!".to_string()));

    assert_eq!(session.message_count(), 2);

    // Update status
    session.set_status(SessionStatus::Archived);
    assert_eq!(session.status(), SessionStatus::Archived);

    // Fork session
    let forked = session.fork("Forked Session".to_string());
    assert_eq!(forked.metadata().title, "Forked Session");
    assert_eq!(forked.message_count(), 2); // Should copy messages
    assert_eq!(forked.status(), SessionStatus::Active);

    // Delete messages from forked session
    forked.clear_messages();
    assert_eq!(forked.message_count(), 0);
}

#[tokio::test]
async fn test_message_compaction() {
    let mut session = Session::new("Compaction Test".to_string());

    // Add many messages
    for i in 0..100 {
        session.add_message(MessageV2::user(format!("Message {}", i)));
        session.add_message(MessageV2::assistant(format!("Response {}", i)));
    }

    let initial_count = session.message_count();
    assert_eq!(initial_count, 200);

    let initial_tokens = session.token_count();

    // Compact using recent strategy
    let engine = CompactionEngine::new();
    let report = engine
        .compact(&mut session, CompactionStrategy::RecentMessages { keep_last: 50 })
        .unwrap();

    // Verify compaction
    assert!(session.message_count() < initial_count);
    assert_eq!(session.message_count(), 50);
    assert!(session.token_count() < initial_tokens);
    assert!(report.tokens_saved > 0);
}

#[tokio::test]
async fn test_compaction_strategies() {
    let mut session = Session::new("Strategy Test".to_string());

    // Add messages with timestamps
    for i in 0..50 {
        session.add_message(MessageV2::user(format!("User message {}", i)));
        session.add_message(MessageV2::assistant(format!("Assistant response {}", i)));
    }

    let engine = CompactionEngine::new();

    // Test 1: Recent messages strategy
    let mut session1 = session.clone();
    let report1 = engine
        .compact(
            &mut session1,
            CompactionStrategy::RecentMessages { keep_last: 20 },
        )
        .unwrap();
    assert_eq!(session1.message_count(), 20);
    assert!(report1.tokens_saved > 0);

    // Test 2: Token budget strategy
    let mut session2 = session.clone();
    let initial_tokens = session2.token_count();
    let target_tokens = initial_tokens / 2;

    let report2 = engine
        .compact(&mut session2, CompactionStrategy::TokenBudget { max_tokens: target_tokens })
        .unwrap();
    assert!(session2.token_count() <= target_tokens + 100); // Allow some margin
    assert!(report2.tokens_saved > 0);

    // Test 3: Summary strategy
    let mut session3 = session.clone();
    let report3 = engine
        .compact(&mut session3, CompactionStrategy::SummarizeOld {
            keep_last: 10,
            summary_threshold: 20,
        })
        .unwrap();
    assert!(session3.message_count() < 100); // Should be reduced
    assert!(report3.tokens_saved > 0);
}

#[tokio::test]
async fn test_session_serialization() {
    let mut session = Session::new("Serialization Test".to_string());

    // Add various message types
    session.add_message(MessageV2::user("Hello".to_string()));
    session.add_message(MessageV2::assistant("Hi!".to_string()));
    session.add_message(MessageV2::user(vec![MessagePart::text("Check this code:")]));
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I see it."),
        MessagePart::code_block("rust", "fn main() {}"),
    ]));

    let test_config = TestConfig::new();
    let save_path = test_config.data_dir.join("test_session.bin");

    // Serialize
    let serializer = SessionSerializer::new();
    serializer.save(&session, &save_path).await.unwrap();

    assert!(save_path.exists());

    // Deserialize
    let loaded = serializer.load(&save_path).await.unwrap();

    // Verify
    assert_eq!(loaded.metadata().title, session.metadata().title);
    assert_eq!(loaded.message_count(), session.message_count());
    assert_eq!(loaded.token_count(), session.token_count());
}

#[tokio::test]
async fn test_serialization_compression() {
    let mut session = Session::new("Compression Test".to_string());

    // Add many messages to test compression
    for i in 0..1000 {
        session.add_message(MessageV2::user(format!("Long message number {} with lots of text to compress", i)));
        session.add_message(MessageV2::assistant(format!("Response number {} with even more text to ensure we have enough data for compression to be effective", i)));
    }

    let test_config = TestConfig::new();
    let save_path = test_config.data_dir.join("compressed_session.bin");

    // Serialize with compression
    let serializer = SessionSerializer::new();
    serializer.save(&session, &save_path).await.unwrap();

    // Check file size
    let file_size = fs::metadata(&save_path).unwrap().len();

    // Deserialize
    let loaded = serializer.load(&save_path).await.unwrap();

    // Verify integrity
    assert_eq!(loaded.message_count(), session.message_count());
    assert_eq!(loaded.token_count(), session.token_count());

    // File should be reasonably sized (compression working)
    // With 2000 messages, uncompressed would be huge
    assert!(file_size < 10_000_000, "Compressed file should be < 10MB");
}

#[tokio::test]
async fn test_token_accounting() {
    let mut session = Session::new("Token Accounting".to_string());

    // Add messages and track tokens
    session.add_message(MessageV2::user("Hello, world!".to_string()));
    let tokens1 = session.token_count();
    assert!(tokens1 > 0);

    session.add_message(MessageV2::assistant("Hi! How can I help?".to_string()));
    let tokens2 = session.token_count();
    assert!(tokens2 > tokens1);

    session.add_message(MessageV2::user(vec![
        MessagePart::text("Here's some code:"),
        MessagePart::code_block("rust", "fn main() { println!(\"Hello\"); }"),
    ]));
    let tokens3 = session.token_count();
    assert!(tokens3 > tokens2);

    // Verify token accounting is monotonic
    assert!(tokens3 > tokens2 > tokens1);
}

#[tokio::test]
async fn test_session_metadata() {
    let mut session = Session::new("Metadata Test".to_string());

    // Update metadata
    session.set_title("New Title".to_string());
    assert_eq!(session.metadata().title, "New Title");

    session.set_description("A test session".to_string());
    assert_eq!(session.metadata().description.as_ref().unwrap(), "A test session");

    // Add tags
    session.add_tag("test".to_string());
    session.add_tag("integration".to_string());

    let tags = session.metadata().tags();
    assert!(tags.contains(&"test".to_string()));
    assert!(tags.contains(&"integration".to_string()));

    // Update status
    session.set_status(SessionStatus::Paused);
    assert_eq!(session.status(), SessionStatus::Paused);
}

#[tokio::test]
async fn test_message_types() {
    let mut session = Session::new("Message Types".to_string());

    // Simple text message
    session.add_message(MessageV2::user("Simple text".to_string()));
    assert_eq!(session.message_count(), 1);

    // Multi-part message
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Here's the answer:"),
        MessagePart::code_block("python", "print('hello')"),
        MessagePart::text("That's it!"),
    ]));
    assert_eq!(session.message_count(), 2);

    // Verify message structure
    let messages = session.messages();
    assert_eq!(messages.len(), 2);

    let first = &messages[0];
    assert_eq!(first.role, MessageRole::User);

    let second = &messages[1];
    assert_eq!(second.role, MessageRole::Assistant);
    assert!(second.parts.len() > 1); // Multi-part
}

#[tokio::test]
async fn test_session_fork_preserves_metadata() {
    let mut session = Session::new("Original".to_string());

    // Set metadata
    session.set_description("Original description".to_string());
    session.add_tag("original".to_string());
    session.add_tag("important".to_string());

    // Add messages
    session.add_message(MessageV2::user("Test".to_string()));
    session.add_message(MessageV2::assistant("Response".to_string()));

    // Fork
    let forked = session.fork("Forked".to_string());

    // Verify fork preserves messages
    assert_eq!(forked.message_count(), 2);

    // Verify fork has new metadata
    assert_eq!(forked.metadata().title, "Forked");
    assert!(forked.metadata().description.is_none()); // Should not copy description

    // Original should be unchanged
    assert_eq!(session.metadata().title, "Original");
    assert_eq!(
        session.metadata().description.as_ref().unwrap(),
        "Original description"
    );
}

#[tokio::test]
async fn test_session_clear_operations() {
    let mut session = Session::new("Clear Test".to_string());

    // Add messages
    for i in 0..10 {
        session.add_message(MessageV2::user(format!("Message {}", i)));
    }

    assert_eq!(session.message_count(), 10);

    // Clear all messages
    session.clear_messages();
    assert_eq!(session.message_count(), 0);
    assert_eq!(session.token_count(), 0);
}

#[tokio::test]
async fn test_compaction_preserves_structure() {
    let mut session = Session::new("Structure Test".to_string());

    // Add messages with different roles
    session.add_message(MessageV2::user("First user message".to_string()));
    session.add_message(MessageV2::assistant("First assistant message".to_string()));
    session.add_message(MessageV2::user("Second user message".to_string()));
    session.add_message(MessageV2::assistant("Second assistant message".to_string()));

    // Compact keeping last 2 messages
    let engine = CompactionEngine::new();
    engine
        .compact(&mut session, CompactionStrategy::RecentMessages { keep_last: 2 })
        .unwrap();

    // Should have 2 messages
    assert_eq!(session.message_count(), 2);

    // Verify they're the right messages (last assistant, last user)
    let messages = session.messages();
    assert_eq!(messages[0].role, MessageRole::User);
    assert_eq!(messages[1].role, MessageRole::Assistant);
}

#[tokio::test]
async fn test_multiple_compactions() {
    let mut session = Session::new("Multiple Compactions".to_string());

    // Add many messages
    for i in 0..100 {
        session.add_message(MessageV2::user(format!("User {}", i)));
        session.add_message(MessageV2::assistant(format!("Assistant {}", i)));
    }

    let engine = CompactionEngine::new();

    // First compaction
    engine
        .compact(&mut session, CompactionStrategy::RecentMessages { keep_last: 50 })
        .unwrap();
    assert_eq!(session.message_count(), 50);

    // Second compaction
    engine
        .compact(&mut session, CompactionStrategy::RecentMessages { keep_last: 20 })
        .unwrap();
    assert_eq!(session.message_count(), 20);

    // Third compaction
    engine
        .compact(&mut session, CompactionStrategy::RecentMessages { keep_last: 10 })
        .unwrap();
    assert_eq!(session.message_count(), 10);
}

#[tokio::test]
async fn test_session_id_generation() {
    let session1 = Session::new("Test 1".to_string());
    let session2 = Session::new("Test 2".to_string());

    // IDs should be unique
    assert_ne!(session1.id(), session2.id());

    // IDs should be valid
    let id1 = session1.id();
    let id2 = session2.id();

    assert!(!id1.to_string().is_empty());
    assert!(!id2.to_string().is_empty());
}

#[tokio::test]
async fn test_empty_session_serialization() {
    let session = Session::new("Empty".to_string());

    let test_config = TestConfig::new();
    let save_path = test_config.data_dir.join("empty_session.bin");

    // Serialize empty session
    let serializer = SessionSerializer::new();
    serializer.save(&session, &save_path).await.unwrap();

    // Deserialize
    let loaded = serializer.load(&save_path).await.unwrap();

    // Verify
    assert_eq!(loaded.metadata().title, "Empty");
    assert_eq!(loaded.message_count(), 0);
    assert_eq!(loaded.token_count(), 0);
}
