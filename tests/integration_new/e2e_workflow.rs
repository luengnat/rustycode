// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! End-to-end workflow integration tests
//!
//! Tests cover:
//! - Complete coding workflow with multiple agents
//! - Debugging workflow with error analysis
//! - Multi-step refactoring workflow
//! - Tool calling integration
//! - Session persistence across workflows

use std::path::{Path, PathBuf};
use std::fs;

use rustycode_config::Config;
use rustycode_session::{MessageV2, MessagePart, MessageRole, Session, SessionStatus};
use rustycode_providers::{bootstrap_from_env, CostTracker};

mod common;
use common::TestConfig;

#[tokio::test]
async fn test_complete_coding_workflow() {
    let test_config = TestConfig::new();

    // Step 1: Load configuration
    let config = Config::load(test_config.project_dir()).unwrap();
    assert!(!config.model.is_empty());

    // Step 2: Bootstrap providers
    let registry = bootstrap_from_env().await;
    assert!(registry.count() >= 1);

    // Step 3: Create session
    let mut session = Session::new("Coding Task".to_string());
    session.add_message(MessageV2::user("Help me implement a binary search function".to_string()));

    // Step 4: Simulate agent response
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I'll help you implement binary search in Rust:"),
        MessagePart::code_block(
            "rust",
            r#"fn binary_search(arr: &[i32], target: i32) -> Option<usize> {
    let mut left = 0;
    let mut right = arr.len();

    while left < right {
        let mid = left + (right - left) / 2;
        match arr[mid].cmp(&target) {
            std::cmp::Ordering::Equal => return Some(mid),
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Greater => right = mid,
        }
    }

    None
}"#,
        ),
        MessagePart::text("This implementation has O(log n) time complexity."),
    ]));

    // Verify workflow steps
    assert_eq!(session.message_count(), 2);
    assert!(session.token_count() > 0);
    assert_eq!(session.status(), SessionStatus::Active);

    // Step 5: Follow-up question
    session.add_message(MessageV2::user("Can you add tests?".to_string()));
    assert_eq!(session.message_count(), 3);

    // Step 6: Track costs
    let mut cost_tracker = CostTracker::new();
    cost_tracker
        .track_usage("anthropic", "claude-3-5-sonnet", 500, 300)
        .unwrap();

    let summary = cost_tracker.get_summary();
    assert!(summary.total_cost > 0.0);
}

#[tokio::test]
async fn test_debugging_workflow() {
    let test_config = TestConfig::new();

    // Setup: Load config
    let config = Config::load(test_config.project_dir()).unwrap();

    // Create debugging session
    let mut session = Session::new("Debugging Task".to_string());

    // User reports error
    session.add_message(MessageV2::user(vec![
        MessagePart::text("I'm getting a panic in my code:"),
        MessagePart::code_block("rust", "thread 'main' panicked at 'index out of bounds: the len is 3 but the index is 5'"),
        MessagePart::text("Here's the code:"),
        MessagePart::code_block("rust", r#"fn main() {
    let arr = [1, 2, 3];
    println!("{}", arr[5]);
}"#),
    ]));

    // Agent analyzes error
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("The panic occurs because you're trying to access index 5 in an array with only 3 elements (indices 0, 1, 2)."),
        MessagePart::text("\nHere's the fix:"),
        MessagePart::code_block("rust", r#"fn main() {
    let arr = [1, 2, 3];
    let index = 5;

    // Check bounds before accessing
    if index < arr.len() {
        println!("{}", arr[index]);
    } else {
        eprintln!("Index {} out of bounds for array of length {}", index, arr.len());
    }

    // Or use get() which returns Option
    match arr.get(5) {
        Some(&value) => println!("{}", value),
        None => eprintln!("Index out of bounds!"),
    }
}"#),
        MessagePart::text("The first approach uses bounds checking, while the second uses Rust's safe `get()` method that returns an Option."),
    ]));

    // Verify debugging workflow
    assert_eq!(session.message_count(), 2);

    let messages = session.messages();
    let assistant_msg = &messages[1];

    assert!(assistant_msg.parts.len() > 2); // Should have multiple parts

    // Verify response includes code blocks
    let has_code = assistant_msg.parts.iter().any(|part| {
        matches!(part, MessagePart::CodeBlock { .. })
    });
    assert!(has_code, "Assistant should provide code examples");
}

#[tokio::test]
async fn test_refactoring_workflow() {
    let test_config = TestConfig::new();

    // Create refactoring session
    let mut session = Session::new("Refactoring Task".to_string());

    // Step 1: User presents original code
    session.add_message(MessageV2::user(vec![
        MessagePart::text("I have this function that's too long. Can you help refactor it?"),
        MessagePart::code_block("rust", r#"fn process_data(data: Vec<String>) -> Vec<String> {
    let mut results = Vec::new();
    for item in data {
        if item.len() > 0 {
            let trimmed = item.trim();
            if trimmed.len() > 5 {
                let upper = trimmed.to_uppercase();
                if !upper.contains("ERROR") {
                    results.push(upper);
                }
            }
        }
    }
    results
}"#),
    ]));

    // Step 2: Agent refactors into smaller functions
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I'll refactor this into smaller, more testable functions:"),
        MessagePart::code_block("rust", r#"fn is_valid_item(item: &str) -> bool {
    !item.is_empty() && item.len() > 5
}

fn should_include(item: &str) -> bool {
    !item.to_uppercase().contains("ERROR")
}

fn process_item(item: &str) -> Option<String> {
    let trimmed = item.trim();
    if is_valid_item(trimmed) && should_include(trimmed) {
        Some(trimmed.to_uppercase())
    } else {
        None
    }
}

fn process_data(data: Vec<String>) -> Vec<String> {
    data.iter()
        .filter_map(|item| process_item(item))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_item() {
        assert!(is_valid_item("validitem"));
        assert!(!is_valid_item(""));
        assert!(!is_valid_item("short"));
    }

    #[test]
    fn test_should_include() {
        assert!(should_include("valid data"));
        assert!(!should_include("ERROR message"));
    }
}"#),
        MessagePart::text("\nBenefits of this refactoring:"),
        MessagePart::text("1. Each function has a single responsibility"),
        MessagePart::text("2. Functions are easier to test independently"),
        MessagePart::text("3. Main function is more readable using iterator chains"),
        MessagePart::text("4. Included unit tests for validation logic"),
    ]));

    // Step 3: User asks for alternative approach
    session.add_message(MessageV2::user("Can you show me a version using more idiomatic Rust?".to_string()));

    // Step 4: Agent provides alternative
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Here's a more idiomatic version:"),
        MessagePart::code_block("rust", r#"fn process_data(data: Vec<String>) -> Vec<String> {
    data.into_iter()
        .map(str::trim)
        .filter(|s| s.len() > 5 && !s.to_uppercase().contains("ERROR"))
        .map(|s| s.to_uppercase())
        .collect()
}"#),
        MessagePart::text("This version:"),
        MessagePart::text("• Uses `into_iter()` to consume the input"),
        MessagePart::text("• Combines transformations in a single pipeline"),
        MessagePart::text("• More concise and follows Rust iterator patterns"),
    ]));

    // Verify refactoring workflow
    assert_eq!(session.message_count(), 4);

    // Check that session has grown
    let token_count = session.token_count();
    assert!(token_count > 100, "Session should have significant content");
}

#[tokio::test]
async fn test_tool_calling_integration() {
    let test_config = TestConfig::new();

    // Create session with tool usage
    let mut session = Session::new("Tool Usage".to_string());

    // User asks to read a file
    session.add_message(MessageV2::user(vec![
        MessagePart::text("Read the file README.md and summarize it"),
    ]));

    // Agent calls Read tool
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I'll read the README.md file for you."),
        MessagePart::tool_use("Read", serde_json::json!({
            "file_path": "README.md"
        })),
    ]));

    // Tool response
    session.add_message(MessageV2::user(vec![
        MessagePart::tool_result("Read", serde_json::json!({
            "content": "# My Project\n\nThis is a sample project.\n\n## Features\n- Feature 1\n- Feature 2"
        })),
    ]));

    // Agent summarizes
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Based on the README.md file:\n\n**Project**: My Project\n\n**Description**: This is a sample project.\n\n**Features**:\n- Feature 1\n- Feature 2\n\nThe project appears to be in early development with basic features planned."),
    ]));

    // Verify tool calling workflow
    assert_eq!(session.message_count(), 4);

    let messages = session.messages();
    let assistant_msg1 = &messages[1];
    let assistant_msg2 = &messages[3];

    // Verify tool use
    assert!(assistant_msg1.parts.iter().any(|p| matches!(p, MessagePart::ToolUse { .. })));

    // Verify tool result handling
    assert!(messages[2].parts.iter().any(|p| matches!(p, MessagePart::ToolResult { .. })));

    // Verify final response
    assert!(assistant_msg2.parts.iter().any(|p| matches!(p, MessagePart::Text(_))));
}

#[tokio::test]
async fn test_session_persistence_across_workflows() {
    let test_config = TestConfig::new();

    // Workflow 1: Initial conversation
    let mut session1 = Session::new("Multi-Workflow Session".to_string());
    session1.add_message(MessageV2::user("Help me understand Rust ownership".to_string()));
    session1.add_message(MessageV2::assistant("Ownership is Rust's key feature for memory safety...".to_string()));

    // Save session
    let save_path = test_config.data_dir.join("persisted_session.bin");
    let serializer = rustycode_session::SessionSerializer::new();
    serializer.save(&session1, &save_path).await.unwrap();

    // Workflow 2: Load and continue
    let mut session2 = serializer.load(&save_path).await.unwrap();
    assert_eq!(session2.message_count(), 2);

    // Continue conversation
    session2.add_message(MessageV2::user("Can you give me an example?".to_string()));
    session2.add_message(MessageV2::assistant(vec![
        MessagePart::text("Here's an example:"),
        MessagePart::code_block("rust", r#"fn main() {
    let s1 = String::from("hello");
    let s2 = s1; // s1 is moved to s2
    // println!("{}", s1); // Error: s1 no longer valid
    println!("{}", s2); // This works
}"#),
    ]));

    // Verify persistence
    assert_eq!(session2.message_count(), 4);

    // Save again
    serializer.save(&session2, &save_path).await.unwrap();

    // Workflow 3: Load again and fork
    let session3 = serializer.load(&save_path).await.unwrap();
    let session3_fork = session3.fork("Forked Discussion".to_string());

    assert_eq!(session3_fork.message_count(), 4);

    // Add new conversation to fork
    let mut session3_fork = session3_fork;
    session3_fork.add_message(MessageV2::user("What about borrowing?".to_string()));

    // Original should be unchanged
    assert_eq!(session3.message_count(), 4);
    assert_eq!(session3_fork.message_count(), 5);
}

#[tokio::test]
async fn test_complex_multi_agent_workflow() {
    let test_config = TestConfig::new();

    // Simulate multi-agent workflow
    let mut session = Session::new("Multi-Agent Task".to_string());

    // User request
    session.add_message(MessageV2::user("I need to add authentication to my API".to_string()));

    // Agent 1: Planner
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I'll create a plan for adding authentication:\n\n1. **Design Phase**\n   - Choose authentication method (JWT)\n   - Design database schema\n   - Define API endpoints\n\n2. **Implementation Phase**\n   - Implement user registration\n   - Implement login endpoint\n   - Add middleware for protected routes\n\n3. **Testing Phase**\n   - Unit tests for auth functions\n   - Integration tests for endpoints\n   - Security tests"),
    ]));

    // Agent 2: Developer
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I'll implement the JWT authentication system:"),
        MessagePart::code_block("rust", r#"use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

fn generate_token(user_id: &str) -> Result<String, Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: user_id.to_owned(),
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("your-secret-key".as_ref()),
    )
}

fn validate_token(token: &str) -> Result<Claims, Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret("your-secret-key".as_ref()),
        &Validation::default(),
    )
    .map(|data| data.claims)
}"#),
    ]));

    // Agent 3: Security Reviewer
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Security review of the implementation:\n\n**Issues Found:**\n\n1. ❌ Secret key is hardcoded - should use environment variable\n2. ❌ No token refresh mechanism\n3. ❌ Missing token revocation support\n4. ❌ No rate limiting on auth endpoints\n\n**Recommendations:**\n\n1. Use `std::env::var` for secret key\n2. Implement refresh token rotation\n3. Add token blacklist for logout\n4. Add rate limiting middleware"),
    ]));

    // Verify multi-agent workflow
    assert_eq!(session.message_count(), 4);

    // Verify different agents contributed
    let messages = session.messages();
    assert!(messages.len() >= 4);

    // Check that we have planning, implementation, and review
    let has_planning = messages[1].parts.iter().any(|p| {
        matches!(p, MessagePart::Text(t) if t.contains("Plan") || t.contains("Phase"))
    });
    assert!(has_planning);

    let has_implementation = messages[2].parts.iter().any(|p| {
        matches!(p, MessagePart::CodeBlock { .. })
    });
    assert!(has_implementation);

    let has_review = messages[3].parts.iter().any(|p| {
        matches!(p, MessagePart::Text(t) if t.contains("Security") || t.contains("review"))
    });
    assert!(has_review);
}

#[tokio::test]
async fn test_error_recovery_workflow() {
    let test_config = TestConfig::new();

    // Simulate error and recovery
    let mut session = Session::new("Error Recovery".to_string());

    // Initial request that fails
    session.add_message(MessageV2::user("Connect to the database".to_string()));

    // Error response
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("I attempted to connect but encountered an error:"),
        MessagePart::code_block("text", "Error: Connection refused (os error 61)\nDatabase might not be running"),
        MessagePart::text("\nLet me troubleshoot this:"),
        MessagePart::tool_use("Bash", serde_json::json!({
            "command": "pg_isready"
        })),
    ]));

    // Tool result - database not running
    session.add_message(MessageV2::user(vec![
        MessagePart::tool_result("Bash", serde_json::json!({
            "exit_code": 1,
            "stdout": "",
            "stderr": "pg_isready: server not running"
        })),
    ]));

    // Recovery attempt
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("The database is not running. Let me start it:"),
        MessagePart::tool_use("Bash", serde_json::json!({
            "command": "brew services start postgresql"
        })),
    ]));

    // Success
    session.add_message(MessageV2::user(vec![
        MessagePart::tool_result("Bash", serde_json::json!({
            "exit_code": 0,
            "stdout": "Successfully started postgresql"
        })),
    ]));

    // Retry original request
    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Database is now running. Let me retry the connection:"),
        MessagePart::tool_use("Bash", serde_json::json!({
            "command": "psql -c 'SELECT version();'"
        })),
    ]));

    // Verify error recovery workflow
    assert_eq!(session.message_count(), 6);

    // Should show progression from error to recovery
    let has_error = session.messages().iter().any(|m| {
        m.parts.iter().any(|p| matches!(p, MessagePart::Text(t) if t.contains("error") || t.contains("Error")))
    });
    assert!(has_error);

    let has_recovery = session.messages().iter().any(|m| {
        m.parts.iter().any(|p| matches!(p, MessagePart::Text(t) if t.contains("start") || t.contains("running")))
    });
    assert!(has_recovery);
}

#[tokio::test]
async fn test_iterative_refinement_workflow() {
    let test_config = TestConfig::new();

    // Simulate iterative improvement
    let mut session = Session::new("Iterative Refinement".to_string());

    // Initial solution
    session.add_message(MessageV2::user("Write a function to reverse a string".to_string()));

    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Here's a simple solution:"),
        MessagePart::code_block("rust", r#"fn reverse_string(s: String) -> String {
    s.chars().rev().collect()
}"#),
    ]));

    // User asks for optimization
    session.add_message(MessageV2::user("Can you make it more efficient for ASCII strings?".to_string()));

    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("For ASCII strings, we can work with bytes directly:"),
        MessagePart::code_block("rust", r#"fn reverse_ascii_string(mut s: String) -> String {
    unsafe {
        let bytes = s.as_bytes_mut();
        bytes.reverse();
    }
    s
}"#),
        MessagePart::text("\n⚠️ **Note**: This is only safe for ASCII strings. For UTF-8 strings, use the first version."),
    ]));

    // User asks about safety
    session.add_message(MessageV2::user("Is that safe? What about Unicode?".to_string()));

    session.add_message(MessageV2::assistant(vec![
        MessagePart::text("Good question! The unsafe version has limitations:\n\n**Unsafe Version Issues:**\n- Breaks multi-byte UTF-8 characters\n- Only works for ASCII (0-127)\n- Can produce invalid UTF-8\n\n**Safe UTF-8 Version:**"),
        MessagePart::code_block("rust", r#"fn reverse_utf8_string(s: String) -> String {
    // Collect graphemes (user-perceived characters)
    unicode_segmentation::UnicodeSegmentation::graphemes(s.as_str(), true)
        .rev()
        .collect()
}

// Or, for better performance on mostly-ASCII text:
fn reverse_string_optimized(s: String) -> String {
    if s.is_ascii() {
        // Fast path for ASCII
        unsafe {
            let mut bytes = s.into_bytes();
            bytes.reverse();
            String::from_utf8_unchecked(bytes)
        }
    } else {
        // Safe path for UTF-8
        s.chars().rev().collect()
    }
}"#),
        MessagePart::text("\nThe optimized version checks if the string is ASCII first, using the fast path when safe."),
    ]));

    // Verify iterative refinement
    assert_eq!(session.message_count(), 6);

    // Should show progression from basic → optimized → safe
    let solutions: Vec<_> = session.messages()
        .iter()
        .filter(|m| m.role == MessageRole::Assistant)
        .collect();

    assert_eq!(solutions.len(), 3);

    // Each solution should build on the previous
    let has_code_block = |msg: &&MessageV2| {
        msg.parts.iter().any(|p| matches!(p, MessagePart::CodeBlock { .. }))
    };

    assert!(has_code_block(&solutions[0]));
    assert!(has_code_block(&solutions[1]));
    assert!(has_code_block(&solutions[2]));
}
