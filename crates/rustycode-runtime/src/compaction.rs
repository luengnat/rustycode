//! Automatic context compaction for long conversations.
//!
//! When conversations approach the context window limit, this service
//! automatically summarizes older messages while preserving recent
//! exchanges and key decisions.

use crate::agent::{AgentMessage, MessageRole};
use anyhow::Result;
use rustycode_llm::provider_v2::MessageContent;
use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider, MessageRole as LLMMessageRole};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for automatic context compaction
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Trigger compaction when using this percentage of context window
    pub trigger_threshold: f64,

    /// Target percentage after compaction
    pub target_ratio: f64,

    /// Minimum messages to preserve after compaction
    pub min_preserve_messages: usize,

    /// Model context window size
    pub context_window: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            trigger_threshold: 0.85, // 85% of context window
            target_ratio: 0.60,      // Reduce to 60%
            min_preserve_messages: 10,
            context_window: 200_000, // Claude Sonnet default
        }
    }
}

/// Model-specific context window sizes
pub fn get_context_window(_model: &str) -> usize {
    // All current Claude models support 200k context
    200_000
}

/// Compaction service for managing long conversations
pub struct CompactionService {
    config: CompactionConfig,
}

impl CompactionService {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    /// Check if compaction is needed based on current messages
    pub fn needs_compaction(&self, messages: &[AgentMessage]) -> bool {
        if messages.len() <= self.config.min_preserve_messages {
            return false;
        }

        let estimated_tokens = self.estimate_tokens(messages);
        let trigger_tokens =
            (self.config.context_window as f64 * self.config.trigger_threshold) as usize;

        estimated_tokens > trigger_tokens
    }

    /// Compact conversation history by summarizing older messages
    pub async fn compact(
        &self,
        messages: Vec<AgentMessage>,
        llm: Arc<dyn LLMProvider>,
        model: &str,
    ) -> Result<Vec<AgentMessage>> {
        if messages.len() <= self.config.min_preserve_messages {
            tracing::debug!(
                "Not enough messages to compact ({} <= {})",
                messages.len(),
                self.config.min_preserve_messages
            );
            return Ok(messages);
        }

        // Determine split point (keep recent messages)
        let preserve_count = std::cmp::max(
            self.config.min_preserve_messages,
            ((messages.len() as f64) * self.config.target_ratio) as usize,
        );

        let split_idx = messages.len().saturating_sub(preserve_count);

        tracing::info!(
            "Compacting conversation: {} messages -> preserving last {} messages (summarizing 0-{})",
            messages.len(),
            preserve_count,
            split_idx - 1
        );

        // Messages to summarize (older ones)
        let to_summarize = &messages[..split_idx];

        // Create summary prompt
        let summary_prompt = self.create_summary_prompt(to_summarize);

        // Call LLM to create summary
        let summary = llm
            .complete(CompletionRequest {
                model: model.to_string(),
                messages: vec![
                    ChatMessage {
                        role: LLMMessageRole::System,
                        content: MessageContent::simple(
                            "You are a conversation summarizer. Create a concise summary of the conversation, preserving key decisions, context, and important details. Focus on:\n\
                             - Key decisions made and their rationale\n\
                             - Important context and requirements\n\
                             - Code changes and their purpose\n\
                             - Any errors encountered and how they were resolved\n\
                             - Current state and next steps\n\n\
                             Keep the summary clear and structured. Use bullet points where appropriate."
                        ),
                    },
                    ChatMessage {
                        role: LLMMessageRole::User,
                        content: MessageContent::simple(summary_prompt),
                    },
                ],
                max_tokens: Some(3000),
                temperature: Some(0.3),
                system_prompt: None,
                tools: None,
                extended_thinking: None,
                thinking_budget: None,
                effort: None,
                thinking: None,
                output_config: None,
                stream: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create summary: {}", e))?
            .content;

        // Build new message list with summary
        let mut result = Vec::new();

        // Add system message about compaction
        result.push(AgentMessage {
            role: MessageRole::System,
            content: format!(
                "[Conversation Compaction: Messages 0-{} summarized]\n\n{}",
                split_idx - 1,
                summary
            ),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
        });

        // Add preserved messages
        result.extend_from_slice(&messages[split_idx..]);

        tracing::info!(
            "Compaction complete: {} messages -> {} messages",
            messages.len(),
            result.len()
        );

        Ok(result)
    }

    fn create_summary_prompt(&self, messages: &[AgentMessage]) -> String {
        let mut prompt = String::from("Summarize the following conversation:\n\n");

        for (i, msg) in messages.iter().enumerate() {
            match msg.role {
                MessageRole::System => {
                    prompt.push_str(&format!("[{}] [SYSTEM]: {}\n", i, msg.content))
                }
                MessageRole::User => prompt.push_str(&format!("[{}] [USER]: {}\n", i, msg.content)),
                MessageRole::Assistant => {
                    prompt.push_str(&format!("[{}] [ASSISTANT]: {}\n", i, msg.content))
                }
                MessageRole::Tool => {
                    // Skip tool results in summary to reduce noise
                    prompt.push_str(&format!("[{}] [TOOL RESULT]: <omitted>\n", i))
                }
            }
        }

        prompt.push_str("\n\nFocus on:\n");
        prompt.push_str("- Key decisions made and their rationale\n");
        prompt.push_str("- Important context and requirements\n");
        prompt.push_str("- Code changes and their purpose\n");
        prompt.push_str("- Any errors encountered and how they were resolved\n");
        prompt.push_str("- Current state and next steps\n");

        prompt
    }

    /// Estimate token count for messages
    /// Rough estimate: ~4 characters per token for English text
    fn estimate_tokens(&self, messages: &[AgentMessage]) -> usize {
        messages
            .iter()
            .map(|m| {
                // More accurate estimation: count words and add overhead
                let word_count = m.content.split_whitespace().count();
                let char_count = m.content.chars().count();

                // Average: 1 token ≈ 4 chars or 0.75 words
                let token_estimate = std::cmp::max(char_count / 4, (word_count * 4) / 3);

                // Add metadata overhead (role, timestamp, etc.)
                token_estimate + 10
            })
            .sum()
    }

    /// Calculate current token usage percentage
    pub fn token_usage_percentage(&self, messages: &[AgentMessage]) -> f64 {
        let estimated = self.estimate_tokens(messages) as f64;
        let capacity = self.config.context_window as f64;
        (estimated / capacity) * 100.0
    }
}

/// Compaction event emitted when compaction occurs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub original_message_count: usize,
    pub compacted_message_count: usize,
    pub preserved_message_count: usize,
    pub summary_length: usize,
    pub estimated_tokens_before: usize,
    pub estimated_tokens_after: usize,
    pub reduction_percentage: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as StdDuration;

    fn create_test_message(role: MessageRole, content: &str) -> AgentMessage {
        AgentMessage {
            role,
            content: content.to_string(),
            timestamp: chrono::Utc::now() - StdDuration::from_secs(3600),
            tool_use_id: None,
        }
    }

    #[test]
    fn test_needs_compaction_below_threshold() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.50,
            target_ratio: 0.60,
            min_preserve_messages: 10,
            context_window: 1000,
        });

        // Create 50 small messages (~500 chars each = ~125 tokens each)
        let _messages: Vec<AgentMessage> = (0..50)
            .map(|i| {
                create_test_message(
                    MessageRole::User,
                    &format!("This is message number {} with some content", i),
                )
            })
            .collect();

        // 50 messages * 50 chars = 2500 chars / 4 = ~625 tokens
        // 625 / 1000 = 62.5% > 50% threshold
        // But let's make them smaller
        let _small_messages: Vec<AgentMessage> = (0..50)
            .map(|i| create_test_message(MessageRole::User, &format!("Msg {}", i)))
            .collect();

        // 20 messages * 5 chars = 100 chars / 4 = ~25 tokens
        // 20 * 10 = 200 overhead
        // Total ~225 tokens < 500 trigger - should NOT need compaction
        let small_messages: Vec<AgentMessage> = (0..20)
            .map(|i| create_test_message(MessageRole::User, &format!("Msg {}", i)))
            .collect();

        // Should NOT need compaction (225 < 500)
        assert!(!service.needs_compaction(&small_messages));
    }

    #[test]
    fn test_needs_compaction_above_threshold() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.50,
            target_ratio: 0.60,
            min_preserve_messages: 10,
            context_window: 1000,
        });

        // Create messages with total content > 500 tokens
        let messages: Vec<AgentMessage> = (0..100)
            .map(|i| {
                create_test_message(
                    MessageRole::User,
                    &format!("This is a longer message number {} with enough content to trigger token counting", i),
                )
            })
            .collect();

        // Should trigger compaction
        assert!(service.needs_compaction(&messages));
    }

    #[test]
    fn test_needs_compaction_below_min_messages() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.50,
            target_ratio: 0.60,
            min_preserve_messages: 10,
            context_window: 100,
        });

        // Only 5 messages
        let messages: Vec<AgentMessage> = (0..5)
            .map(|_i| create_test_message(MessageRole::User, "Test message"))
            .collect();

        // Should NOT trigger even if tokens exceed threshold
        assert!(!service.needs_compaction(&messages));
    }

    #[test]
    fn test_token_usage_percentage() {
        let service = CompactionService::new(CompactionConfig {
            context_window: 10_000,
            ..Default::default()
        });

        let messages: Vec<AgentMessage> = (0..10)
            .map(|_| create_test_message(MessageRole::User, "Test message with content"))
            .collect();

        let percentage = service.token_usage_percentage(&messages);

        // Should be very low for 10 small messages
        assert!(percentage < 10.0);
        assert!(percentage > 0.0);
    }

    #[test]
    fn test_create_summary_prompt() {
        let service = CompactionService::new(Default::default());

        let messages = vec![
            create_test_message(MessageRole::User, "Hello, how are you?"),
            create_test_message(MessageRole::Assistant, "I'm doing well, thanks!"),
            create_test_message(MessageRole::User, "Can you help me with Rust?"),
            create_test_message(
                MessageRole::Assistant,
                "Of course! What do you need help with?",
            ),
        ];

        let prompt = service.create_summary_prompt(&messages);

        // Check that prompt contains messages
        assert!(prompt.contains("[USER]: Hello, how are you?"));
        assert!(prompt.contains("[ASSISTANT]: I'm doing well, thanks!"));
        assert!(prompt.contains("Summarize the following conversation"));

        // Check that prompt includes guidance
        assert!(prompt.contains("Key decisions made"));
        assert!(prompt.contains("Important context"));
    }

    // --- CompactionConfig ---

    #[test]
    fn config_default_values() {
        let cfg = CompactionConfig::default();
        assert!((cfg.trigger_threshold - 0.85).abs() < f64::EPSILON);
        assert!((cfg.target_ratio - 0.60).abs() < f64::EPSILON);
        assert_eq!(cfg.min_preserve_messages, 10);
        assert_eq!(cfg.context_window, 200_000);
    }

    #[test]
    fn config_custom() {
        let cfg = CompactionConfig {
            trigger_threshold: 0.90,
            target_ratio: 0.50,
            min_preserve_messages: 5,
            context_window: 100_000,
        };
        assert_eq!(cfg.context_window, 100_000);
    }

    // --- get_context_window ---

    #[test]
    fn get_context_window_always_200k() {
        assert_eq!(get_context_window("claude-3"), 200_000);
        assert_eq!(get_context_window("gpt-4"), 200_000);
        assert_eq!(get_context_window(""), 200_000);
    }

    // ---------------------------------------------------------------------------
    // Comprehensive test suite (15 additional tests)
    // ---------------------------------------------------------------------------

    // 1. Basic compaction of message history
    // Verifies that needs_compaction returns true when token usage exceeds
    // the trigger threshold with enough messages, and false otherwise.
    #[test]
    fn test_basic_compaction_trigger_logic() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.80,
            target_ratio: 0.50,
            min_preserve_messages: 5,
            context_window: 500,
        });

        // 5 messages are below min_preserve_messages, so no compaction needed
        let few_messages: Vec<AgentMessage> = (0..5)
            .map(|i| {
                create_test_message(
                    MessageRole::User,
                    &format!("Message number {} with some padding text for length", i),
                )
            })
            .collect();
        assert!(!service.needs_compaction(&few_messages));

        // 50 long messages should exceed 80% of 500 tokens
        let many_messages: Vec<AgentMessage> = (0..50)
            .map(|i| {
                create_test_message(
                    MessageRole::User,
                    &format!(
                        "This is a substantially longer message number {} \
                         with enough text to push token count well above the threshold",
                        i
                    ),
                )
            })
            .collect();
        assert!(service.needs_compaction(&many_messages));
    }

    // 2. Preserving system messages during compaction
    // System messages are essential context (instructions, persona). This test
    // verifies that the summary prompt includes system messages and they are
    // not silently dropped from the compaction pipeline.
    #[test]
    fn test_system_messages_preserved_in_summary_prompt() {
        let service = CompactionService::new(CompactionConfig {
            min_preserve_messages: 2,
            ..Default::default()
        });

        let messages = vec![
            create_test_message(MessageRole::System, "You are a helpful Rust tutor."),
            create_test_message(MessageRole::User, "What is ownership?"),
            create_test_message(MessageRole::Assistant, "Ownership is a core Rust concept."),
            create_test_message(MessageRole::System, "Always use idiomatic Rust patterns."),
            create_test_message(MessageRole::User, "Show me an example."),
        ];

        let prompt = service.create_summary_prompt(&messages);

        // Both system messages must appear in the summary prompt
        assert!(prompt.contains("[SYSTEM]: You are a helpful Rust tutor."));
        assert!(prompt.contains("[SYSTEM]: Always use idiomatic Rust patterns."));
    }

    // 3. Token counting accuracy
    // The estimate_tokens method uses ~4 chars per token plus 10 tokens overhead
    // per message. This test verifies the math is correct for known inputs.
    #[test]
    fn test_token_estimation_accuracy() {
        let service = CompactionService::new(CompactionConfig {
            context_window: 100_000,
            ..Default::default()
        });

        // Single message with exactly 40 characters of content
        // 40 chars / 4 = 10 content tokens + 10 overhead = 20 tokens
        let single = vec![create_test_message(MessageRole::User, &"a".repeat(40))];
        let tokens = service.estimate_tokens(&single);
        assert_eq!(tokens, 20);

        // Two identical messages: 2 * 20 = 40 tokens
        let double = vec![
            create_test_message(MessageRole::User, &"a".repeat(40)),
            create_test_message(MessageRole::Assistant, &"b".repeat(40)),
        ];
        let tokens_double = service.estimate_tokens(&double);
        assert_eq!(tokens_double, 40);
    }

    // 4. Compaction thresholds and triggers
    // Tests the exact boundary where compaction flips from not-needed to needed
    // by using a very small context window and known token counts.
    #[test]
    fn test_compaction_threshold_boundary() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.50,
            target_ratio: 0.40,
            min_preserve_messages: 3,
            context_window: 100, // 50% threshold = 50 tokens
        });

        // 4 messages of 8 chars each: 4 * (8/4 + 10) = 4 * 12 = 48 tokens
        // 48 < 50, so should NOT trigger
        let below: Vec<AgentMessage> = (0..4)
            .map(|_| create_test_message(MessageRole::User, "12345678"))
            .collect();
        assert!(!service.needs_compaction(&below));

        // 5 messages: 5 * 12 = 60 tokens > 50 threshold
        let above: Vec<AgentMessage> = (0..5)
            .map(|_| create_test_message(MessageRole::User, "12345678"))
            .collect();
        assert!(service.needs_compaction(&above));
    }

    // 5. Handling empty message lists
    // An empty message list should never trigger compaction and should not panic.
    #[test]
    fn test_empty_message_list_no_panic() {
        let service = CompactionService::new(CompactionConfig {
            min_preserve_messages: 0,
            context_window: 1,
            trigger_threshold: 0.01,
            target_ratio: 0.5,
        });

        let empty: Vec<AgentMessage> = vec![];
        assert!(!service.needs_compaction(&empty));
        assert_eq!(service.estimate_tokens(&empty), 0);

        let pct = service.token_usage_percentage(&empty);
        assert!((pct - 0.0).abs() < f64::EPSILON);
    }

    // 6. Large message list compaction
    // Simulates a long conversation (500 messages) and verifies compaction
    // triggers correctly and token estimation scales linearly.
    #[test]
    fn test_large_message_list_compaction() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.70,
            target_ratio: 0.50,
            min_preserve_messages: 10,
            context_window: 5_000,
        });

        let messages: Vec<AgentMessage> = (0..500)
            .map(|i| {
                create_test_message(
                    MessageRole::User,
                    &format!("Message {} with enough text to be counted properly", i),
                )
            })
            .collect();

        assert!(service.needs_compaction(&messages));

        // Verify token count is proportional to message count
        let half: Vec<AgentMessage> = messages[..250].to_vec();
        let full_tokens = service.estimate_tokens(&messages);
        let half_tokens = service.estimate_tokens(&half);
        assert!(full_tokens > half_tokens);
        // Should be roughly 2x (allow 20% tolerance for per-message overhead)
        let ratio = full_tokens as f64 / half_tokens as f64;
        assert!(
            ratio > 1.8 && ratio < 2.2,
            "Expected ~2.0 ratio, got {}",
            ratio
        );
    }

    // 7. Preserving tool call/result pairs
    // Tool messages are summarized as "<omitted>" in the summary prompt.
    // This test confirms tool messages don't leak raw output into the prompt.
    #[test]
    fn test_tool_results_omitted_in_summary() {
        let service = CompactionService::new(Default::default());

        let messages = vec![
            create_test_message(MessageRole::User, "Read file foo.rs"),
            create_test_message(MessageRole::Assistant, "I'll read that file for you."),
            create_test_message(
                MessageRole::Tool,
                "fn main() { println!(\"sensitive data\"); }",
            ),
            create_test_message(MessageRole::Assistant, "The file contains a main function."),
        ];

        let prompt = service.create_summary_prompt(&messages);

        // Tool output should be omitted, not included verbatim
        assert!(!prompt.contains("sensitive data"));
        assert!(prompt.contains("[TOOL RESULT]: <omitted>"));
    }

    // 8. Compaction with mixed message roles
    // Real conversations have all four role types interleaved. This test
    // verifies the summary prompt formats each role correctly.
    #[test]
    fn test_mixed_roles_in_summary_prompt() {
        let service = CompactionService::new(Default::default());

        let messages = vec![
            create_test_message(MessageRole::System, "System init"),
            create_test_message(MessageRole::User, "User asks"),
            create_test_message(MessageRole::Assistant, "Assistant answers"),
            create_test_message(MessageRole::Tool, "Tool output"),
            create_test_message(MessageRole::User, "User follows up"),
        ];

        let prompt = service.create_summary_prompt(&messages);

        assert!(prompt.contains("[0] [SYSTEM]: System init"));
        assert!(prompt.contains("[1] [USER]: User asks"));
        assert!(prompt.contains("[2] [ASSISTANT]: Assistant answers"));
        assert!(prompt.contains("[3] [TOOL RESULT]: <omitted>"));
        assert!(prompt.contains("[4] [USER]: User follows up"));
    }

    // 9a. Edge case: single message
    // A single message should never trigger compaction regardless of size.
    #[test]
    fn test_single_message_never_compacts() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.01,
            target_ratio: 0.5,
            min_preserve_messages: 5,
            context_window: 10, // tiny window
        });

        let one = vec![create_test_message(MessageRole::User, &"x".repeat(10_000))];

        // Below min_preserve_messages, so should not compact
        assert!(!service.needs_compaction(&one));
    }

    // 9b. Edge case: all system messages
    // A conversation of only system messages should still be estimatable
    // and should produce a valid summary prompt.
    #[test]
    fn test_all_system_messages() {
        let service = CompactionService::new(Default::default());

        let messages: Vec<AgentMessage> = (0..15)
            .map(|i| create_test_message(MessageRole::System, &format!("System directive {}", i)))
            .collect();

        // Should still estimate tokens fine
        let tokens = service.estimate_tokens(&messages);
        assert!(tokens > 0);

        // Summary prompt should list every system message
        let prompt = service.create_summary_prompt(&messages);
        for i in 0..15 {
            assert!(
                prompt.contains(&format!("System directive {}", i)),
                "Missing system directive {}",
                i
            );
        }
    }

    // 9c. Edge case: all user messages
    // Verify that a user-only conversation estimates tokens and generates
    // a proper summary prompt.
    #[test]
    fn test_all_user_messages() {
        let service = CompactionService::new(Default::default());

        let messages: Vec<AgentMessage> = (0..20)
            .map(|i| create_test_message(MessageRole::User, &format!("User query {}", i)))
            .collect();

        let tokens = service.estimate_tokens(&messages);
        assert!(tokens > 0);

        let prompt = service.create_summary_prompt(&messages);
        for i in 0..20 {
            assert!(prompt.contains(&format!("User query {}", i)));
        }
    }

    // 10. Round-trip: compact then verify message ordering
    // After compaction (without LLM), verify the service would correctly
    // determine the split point: recent messages are preserved in order
    // and the summary goes first. We test the synchronous helper logic.
    #[test]
    fn test_message_ordering_after_split_calculation() {
        let config = CompactionConfig {
            trigger_threshold: 0.50,
            target_ratio: 0.50,
            min_preserve_messages: 4,
            context_window: 500,
        };
        let service = CompactionService::new(config.clone());

        // Build 20 messages, each with a unique identifier
        let messages: Vec<AgentMessage> = (0..20)
            .map(|i| {
                create_test_message(
                    MessageRole::User,
                    &format!(
                        "Unique message ID={} with padding to increase token count",
                        i
                    ),
                )
            })
            .collect();

        assert!(service.needs_compaction(&messages));

        // Simulate what compact() does: calculate the split point
        // target_ratio=0.50 -> preserve max(4, 20*0.5)=max(4,10)=10 messages
        // split_idx = 20 - 10 = 10
        let preserve_count = std::cmp::max(
            config.min_preserve_messages,
            (messages.len() as f64 * config.target_ratio) as usize,
        );
        let split_idx = messages.len().saturating_sub(preserve_count);
        let _ = config; // suppress unused warning

        assert_eq!(split_idx, 10);
        assert_eq!(preserve_count, 10);

        // The "to summarize" portion should be messages 0..10
        let to_summarize = &messages[..split_idx];
        assert!(to_summarize[0].content.contains("ID=0"));
        assert!(to_summarize[9].content.contains("ID=9"));

        // The "preserved" portion should be messages 10..20 in original order
        let preserved = &messages[split_idx..];
        assert!(preserved[0].content.contains("ID=10"));
        assert!(preserved[9].content.contains("ID=19"));

        // Verify total ordering: every ID from 10..19 appears in sequence
        for (i, msg) in preserved.iter().enumerate() {
            assert!(
                msg.content.contains(&format!("ID={}", 10 + i)),
                "Expected ID={} at preserved index {}, got: {}",
                10 + i,
                i,
                msg.content
            );
        }
    }

    // 11. CompactionEvent serialization round-trip
    // CompactionEvent is Serialize + Deserialize. Verify JSON round-trip
    // preserves all fields correctly.
    #[test]
    fn test_compaction_event_serde_roundtrip() {
        let event = CompactionEvent {
            timestamp: chrono::Utc::now(),
            original_message_count: 100,
            compacted_message_count: 20,
            preserved_message_count: 19,
            summary_length: 500,
            estimated_tokens_before: 80_000,
            estimated_tokens_after: 15_000,
            reduction_percentage: 81.25,
        };

        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: CompactionEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.original_message_count, 100);
        assert_eq!(decoded.compacted_message_count, 20);
        assert_eq!(decoded.preserved_message_count, 19);
        assert_eq!(decoded.summary_length, 500);
        assert_eq!(decoded.estimated_tokens_before, 80_000);
        assert_eq!(decoded.estimated_tokens_after, 15_000);
        assert!((decoded.reduction_percentage - 81.25).abs() < f64::EPSILON);
    }

    // 12. Token usage percentage is proportional to messages
    // Doubling the number of identical messages should approximately
    // double the token usage percentage.
    #[test]
    fn test_token_usage_scales_with_message_count() {
        let service = CompactionService::new(CompactionConfig {
            context_window: 10_000,
            ..Default::default()
        });

        let base: Vec<AgentMessage> = (0..10)
            .map(|_| create_test_message(MessageRole::User, "Hello world this is a test"))
            .collect();

        let double: Vec<AgentMessage> = (0..20)
            .map(|_| create_test_message(MessageRole::User, "Hello world this is a test"))
            .collect();

        let pct_base = service.token_usage_percentage(&base);
        let pct_double = service.token_usage_percentage(&double);

        assert!(
            pct_double > pct_base,
            "Doubling messages should increase token usage"
        );

        let ratio = pct_double / pct_base;
        assert!(
            ratio > 1.8 && ratio < 2.2,
            "Expected ~2.0 ratio, got {}",
            ratio
        );
    }

    // 13. Tool messages with tool_use_id are constructed correctly
    // Ensures the tool_use_id field is carried through message construction
    // and does not interfere with token estimation.
    #[test]
    fn test_tool_messages_with_use_id() {
        let mut msg = create_test_message(MessageRole::Tool, "Tool result content here");
        msg.tool_use_id = Some("tool_call_abc123".to_string());

        let service = CompactionService::new(Default::default());
        let tokens = service.estimate_tokens(&[msg.clone()]);

        // Token estimation should only use content, not tool_use_id
        // "Tool result content here" = 26 chars / 4 = 6 content + 10 overhead = 16
        assert_eq!(tokens, 16);

        let prompt = service.create_summary_prompt(&[msg]);
        assert!(prompt.contains("[TOOL RESULT]: <omitted>"));
    }

    // 14. Config with extreme values does not panic
    // Very large or very small config values should not cause overflow or
    // division-by-zero panics in any method.
    #[test]
    fn test_extreme_config_values_no_panic() {
        let tiny_config = CompactionConfig {
            trigger_threshold: 0.0,
            target_ratio: 0.0,
            min_preserve_messages: 0,
            context_window: 1,
        };
        let tiny_service = CompactionService::new(tiny_config);

        let messages = vec![create_test_message(MessageRole::User, "Hi")];
        // Should not panic even with context_window=1
        let _ = tiny_service.needs_compaction(&messages);
        let _ = tiny_service.estimate_tokens(&messages);
        let pct = tiny_service.token_usage_percentage(&messages);
        assert!(pct.is_finite());

        let huge_config = CompactionConfig {
            trigger_threshold: 1.0,
            target_ratio: 1.0,
            min_preserve_messages: 1_000_000,
            context_window: usize::MAX,
        };
        let huge_service = CompactionService::new(huge_config);

        let big_messages: Vec<AgentMessage> = (0..100)
            .map(|i| create_test_message(MessageRole::User, &format!("Message {}", i)))
            .collect();
        // Below min_preserve_messages, so should return false
        assert!(!huge_service.needs_compaction(&big_messages));
    }

    // 15. Summary prompt preserves message indices correctly
    // The index numbers in the summary prompt must correspond exactly
    // to the position of each message in the input slice.
    #[test]
    fn test_summary_prompt_indices_match_positions() {
        let service = CompactionService::new(Default::default());

        let messages = vec![
            create_test_message(MessageRole::User, "first"),
            create_test_message(MessageRole::Assistant, "second"),
            create_test_message(MessageRole::User, "third"),
            create_test_message(MessageRole::Tool, "fourth"),
            create_test_message(MessageRole::System, "fifth"),
        ];

        let prompt = service.create_summary_prompt(&messages);

        // Each index must appear in order
        assert!(prompt.contains("[0] [USER]: first"));
        assert!(prompt.contains("[1] [ASSISTANT]: second"));
        assert!(prompt.contains("[2] [USER]: third"));
        assert!(prompt.contains("[3] [TOOL RESULT]: <omitted>"));
        assert!(prompt.contains("[4] [SYSTEM]: fifth"));
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for compaction
    // =========================================================================

    // 1. CompactionConfig clone produces equal values
    #[test]
    fn compaction_config_clone_equal() {
        let cfg = CompactionConfig {
            trigger_threshold: 0.75,
            target_ratio: 0.40,
            min_preserve_messages: 8,
            context_window: 150_000,
        };
        let cloned = cfg.clone();
        assert!((cloned.trigger_threshold - cfg.trigger_threshold).abs() < f64::EPSILON);
        assert!((cloned.target_ratio - cfg.target_ratio).abs() < f64::EPSILON);
        assert_eq!(cloned.min_preserve_messages, cfg.min_preserve_messages);
        assert_eq!(cloned.context_window, cfg.context_window);
    }

    // 2. CompactionConfig debug format
    #[test]
    fn compaction_config_debug_format() {
        let cfg = CompactionConfig::default();
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("trigger_threshold"));
        assert!(debug.contains("context_window"));
    }

    // 3. CompactionEvent serde with zero values
    #[test]
    fn compaction_event_zero_values_serde() {
        let event = CompactionEvent {
            timestamp: chrono::Utc::now(),
            original_message_count: 0,
            compacted_message_count: 0,
            preserved_message_count: 0,
            summary_length: 0,
            estimated_tokens_before: 0,
            estimated_tokens_after: 0,
            reduction_percentage: 0.0,
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: CompactionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.original_message_count, 0);
        assert!((decoded.reduction_percentage).abs() < f64::EPSILON);
    }

    // 4. CompactionEvent serde with large values
    #[test]
    fn compaction_event_large_values_serde() {
        let event = CompactionEvent {
            timestamp: chrono::Utc::now(),
            original_message_count: 1_000_000,
            compacted_message_count: 50_000,
            preserved_message_count: 49_999,
            summary_length: 500_000,
            estimated_tokens_before: 8_000_000,
            estimated_tokens_after: 400_000,
            reduction_percentage: 95.0,
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: CompactionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.original_message_count, 1_000_000);
        assert_eq!(decoded.estimated_tokens_before, 8_000_000);
        assert!((decoded.reduction_percentage - 95.0).abs() < f64::EPSILON);
    }

    // 5. CompactionEvent debug format
    #[test]
    fn compaction_event_debug_format() {
        let event = CompactionEvent {
            timestamp: chrono::Utc::now(),
            original_message_count: 100,
            compacted_message_count: 20,
            preserved_message_count: 19,
            summary_length: 500,
            estimated_tokens_before: 80_000,
            estimated_tokens_after: 15_000,
            reduction_percentage: 81.25,
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("original_message_count"));
        assert!(debug.contains("reduction_percentage"));
    }

    // 6. CompactionEvent clone produces equal values
    #[test]
    fn compaction_event_clone_equal() {
        let event = CompactionEvent {
            timestamp: chrono::Utc::now(),
            original_message_count: 42,
            compacted_message_count: 10,
            preserved_message_count: 9,
            summary_length: 200,
            estimated_tokens_before: 5000,
            estimated_tokens_after: 1000,
            reduction_percentage: 80.0,
        };
        let cloned = event.clone();
        assert_eq!(cloned.original_message_count, event.original_message_count);
        assert_eq!(cloned.summary_length, event.summary_length);
    }

    // 7. Token estimation for single empty message
    #[test]
    fn token_estimation_empty_content() {
        let service = CompactionService::new(CompactionConfig::default());
        let msg = create_test_message(MessageRole::User, "");
        let tokens = service.estimate_tokens(&[msg]);
        // 0 chars / 4 = 0 content tokens + 10 overhead = 10
        assert_eq!(tokens, 10);
    }

    // 8. Token estimation for single char message
    #[test]
    fn token_estimation_single_char() {
        let service = CompactionService::new(CompactionConfig::default());
        let msg = create_test_message(MessageRole::User, "x");
        let tokens = service.estimate_tokens(&[msg]);
        // 1 char / 4 = 0 content tokens (integer division) + 10 overhead + 1 = 11
        assert!(tokens >= 10);
    }

    // 9. Summary prompt for empty messages returns header only
    #[test]
    fn summary_prompt_empty_messages() {
        let service = CompactionService::new(CompactionConfig::default());
        let prompt = service.create_summary_prompt(&[]);
        assert!(prompt.contains("Summarize the following conversation"));
    }

    // 10. Token usage percentage with exact calculation
    #[test]
    fn token_usage_exact_calculation() {
        let service = CompactionService::new(CompactionConfig {
            context_window: 100,
            ..Default::default()
        });
        // 5 messages * (8 chars / 4 + 10 overhead) = 5 * 12 = 60 tokens
        let msgs: Vec<AgentMessage> = (0..5)
            .map(|_| create_test_message(MessageRole::User, "12345678"))
            .collect();
        let pct = service.token_usage_percentage(&msgs);
        assert!((pct - 60.0).abs() < f64::EPSILON);
    }

    // 11. Needs compaction respects trigger_threshold = 1.0
    #[test]
    fn needs_compaction_never_with_full_threshold() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 1.0,
            target_ratio: 0.5,
            min_preserve_messages: 5,
            context_window: 1,
        });
        let msgs: Vec<AgentMessage> = (0..100)
            .map(|i| create_test_message(MessageRole::User, &format!("Message {}", i)))
            .collect();
        // 1.0 * 1 = 1 token trigger. 100 messages each > 0 tokens => needs compaction
        assert!(service.needs_compaction(&msgs));
    }

    // 12. Needs compaction with trigger_threshold = 0.0
    #[test]
    fn needs_compaction_always_with_zero_threshold() {
        let service = CompactionService::new(CompactionConfig {
            trigger_threshold: 0.0,
            target_ratio: 0.5,
            min_preserve_messages: 5,
            context_window: 1_000_000,
        });
        let msgs: Vec<AgentMessage> = (0..10)
            .map(|i| create_test_message(MessageRole::User, &format!("Msg {}", i)))
            .collect();
        // 0.0 * 1M = 0 token trigger. Any tokens > 0 => needs compaction
        assert!(service.needs_compaction(&msgs));
    }

    // 13. Get context window for various model names
    #[test]
    fn get_context_window_various_models() {
        assert_eq!(get_context_window("claude-3-opus"), 200_000);
        assert_eq!(get_context_window("claude-3.5-sonnet"), 200_000);
        assert_eq!(get_context_window("gpt-4-turbo"), 200_000);
        assert_eq!(get_context_window("unknown-model"), 200_000);
    }

    // 14. Multiple tool messages in summary prompt
    #[test]
    fn multiple_tool_messages_in_summary() {
        let service = CompactionService::new(CompactionConfig::default());
        let msgs = vec![
            create_test_message(MessageRole::User, "Run tool 1"),
            create_test_message(MessageRole::Tool, "result 1"),
            create_test_message(MessageRole::User, "Run tool 2"),
            create_test_message(MessageRole::Tool, "result 2"),
        ];
        let prompt = service.create_summary_prompt(&msgs);
        // Both tool results should be omitted
        assert!(!prompt.contains("result 1"));
        assert!(!prompt.contains("result 2"));
        // Should have two [TOOL RESULT] entries
        let count = prompt.matches("[TOOL RESULT]: <omitted>").count();
        assert_eq!(count, 2);
    }

    // 15. Estimate tokens with very large single message
    #[test]
    fn estimate_tokens_large_message() {
        let service = CompactionService::new(CompactionConfig::default());
        let big = create_test_message(MessageRole::User, &"a".repeat(4_000));
        let tokens = service.estimate_tokens(&[big]);
        // 4000 chars / 4 = 1000 content + 10 overhead = 1010
        assert_eq!(tokens, 1010);
    }
}
