//! Conversation Compaction Service
//!
//! Manages context window compaction by summarizing older messages
//! when token/turn thresholds are exceeded.
//!
//! Inspired by forge_app's Compactor service and goose's progressive compaction:
//! - **Progressive compaction**: Try removing 0%, 10%, 20%, 50%, 100% of tool responses
//! - **Middle-out removal**: Remove tool responses from the middle first, preserving
//!   recent context and early system messages
//! - **Threshold-based triggering**: Compacts when usage exceeds configurable threshold
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::compaction::{Compactor, CompactionConfig};
//!
//! let config = CompactionConfig {
//!     max_tokens: 100_000,
//!     max_turns: 50,
//!     max_messages: 100,
//!     retention_window: 10,
//!     ..Default::default()
//! };
//!
//! let mut compactor = Compactor::new(config);
//!
//! for message in messages {
//!     let action = compactor.add_message(message);
//!     if action.should_compact() {
//!         let result = compactor.compact()?;
//!         println!("Compacted: {} messages, {} tokens saved",
//!             result.messages_removed, result.tokens_saved);
//!     }
//! }
//! ```

use rustycode_protocol::Message;
use std::time::Instant;

/// Default batch size for tool pair summarization
pub const DEFAULT_BATCH_SIZE: usize = 10;

/// Configuration for when compaction should trigger
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Maximum tokens before compaction triggers
    pub max_tokens: usize,
    /// Maximum turns before compaction triggers
    pub max_turns: usize,
    /// Maximum messages before compaction triggers
    pub max_messages: usize,
    /// Number of recent messages to always preserve
    pub retention_window: usize,
    /// Auto-compact threshold as fraction of context (0.0-1.0), e.g. 0.8 = compact at 80%
    pub auto_compact_threshold: f64,
    /// Whether to use progressive compaction (remove tool responses incrementally)
    pub progressive_compaction: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100_000,
            max_turns: 50,
            max_messages: 100,
            retention_window: 10,
            auto_compact_threshold: 0.8,
            progressive_compaction: true,
        }
    }
}

/// A conversation message with metadata for compaction
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
    pub token_count: usize,
    pub timestamp: Instant,
}

impl From<Message> for ConversationMessage {
    fn from(msg: Message) -> Self {
        let content = msg.content.to_text();
        // Rough estimation: 1 token ≈ 4 characters
        let token_count = content.len().div_ceil(4);

        Self {
            role: MessageRole::from(msg.role.as_str()),
            content,
            token_count,
            timestamp: Instant::now(),
        }
    }
}

/// Role of a message in the conversation
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn parse_role(role: &str) -> Self {
        match role {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User, // Default to user for unknown roles
        }
    }
}

impl From<&str> for MessageRole {
    fn from(role: &str) -> Self {
        Self::parse_role(role)
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Number of messages removed
    pub messages_removed: usize,
    /// Number of tokens saved
    pub tokens_saved: usize,
    /// The generated summary of removed messages
    pub summary: String,
    /// Strategy used for compaction
    pub strategy: CompactionStrategy,
}

/// Strategy used for compaction
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompactionStrategy {
    /// Simple front-removal (original strategy)
    FrontRemoval,
    /// Progressive tool response removal with percentage
    ProgressiveToolRemoval(u32),
}

/// Tracks compaction state and performs compaction when needed
#[derive(Debug)]
pub struct Compactor {
    config: CompactionConfig,
    turn_count: usize,
    total_tokens: usize,
    messages: Vec<ConversationMessage>,
    compaction_count: usize,
}

impl Compactor {
    /// Create a new compactor with the given configuration
    pub fn new(config: CompactionConfig) -> Self {
        Self {
            config,
            turn_count: 0,
            total_tokens: 0,
            messages: Vec::new(),
            compaction_count: 0,
        }
    }

    /// Add a message and check if compaction is needed
    pub fn add_message(&mut self, message: ConversationMessage) -> CompactionAction {
        self.total_tokens += message.token_count;
        if message.role == MessageRole::User || message.role == MessageRole::Assistant {
            self.turn_count += 1;
        }
        self.messages.push(message);

        if self.should_compact() {
            CompactionAction::Compact
        } else {
            CompactionAction::None
        }
    }

    /// Add a protocol Message and check if compaction is needed
    pub fn add_message_from_protocol(&mut self, message: Message) -> CompactionAction {
        let conv_message = ConversationMessage::from(message);
        self.add_message(conv_message)
    }

    /// Check if compaction thresholds are exceeded
    pub fn should_compact(&self) -> bool {
        self.total_tokens >= self.config.max_tokens
            || self.turn_count >= self.config.max_turns
            || self.messages.len() >= self.config.max_messages
    }

    /// Perform compaction: remove old messages and generate a summary
    pub fn compact(&mut self) -> Option<CompactionResult> {
        if self.messages.len() <= self.config.retention_window {
            return None;
        }

        // Try progressive compaction first if enabled
        if self.config.progressive_compaction {
            if let Some(result) = self.compact_progressive() {
                return Some(result);
            }
        }

        // Fall back to simple front-removal compaction
        self.compact_simple()
    }

    /// Progressive compaction (inspired by goose).
    ///
    /// Tries removing tool responses at increasing percentages:
    /// 0% (just summarize) -> 10% -> 20% -> 50% -> 100%.
    /// Tool responses are removed from the middle outward to preserve
    /// early context and recent messages.
    fn compact_progressive(&mut self) -> Option<CompactionResult> {
        let percentages = [0, 10, 20, 50, 100];
        let protected = self.config.retention_window;

        // Find tool response indices in the removable range
        let removable_end = self.messages.len().saturating_sub(protected);
        let tool_indices: Vec<usize> = self.messages[..removable_end]
            .iter()
            .enumerate()
            .filter(|(_, m)| m.role == MessageRole::Tool)
            .map(|(i, _)| i)
            .collect();

        // If no tool responses, fall back to simple compaction
        if tool_indices.is_empty() {
            return None;
        }

        for &remove_percent in &percentages {
            let indices_to_remove = Self::middle_out_indices(&tool_indices, remove_percent);
            if indices_to_remove.is_empty() && remove_percent == 0 {
                // At 0%, just generate a summary without removing messages
                continue;
            }

            // Remove selected indices (in reverse order to preserve positions)
            let mut removed_messages = Vec::new();
            let mut tokens_removed = 0usize;
            for &idx in indices_to_remove.iter().rev() {
                if idx < self.messages.len() {
                    let msg = self.messages.remove(idx);
                    tokens_removed += msg.token_count;
                    removed_messages.push(msg);
                }
            }

            if !removed_messages.is_empty() {
                let summary = self.generate_summary(&removed_messages);
                self.total_tokens = self.total_tokens.saturating_sub(tokens_removed);
                self.recount_turns();
                self.compaction_count += 1;

                return Some(CompactionResult {
                    messages_removed: removed_messages.len(),
                    tokens_saved: tokens_removed,
                    summary,
                    strategy: CompactionStrategy::ProgressiveToolRemoval(remove_percent),
                });
            }
        }

        None
    }

    /// Simple front-removal compaction (original strategy).
    fn compact_simple(&mut self) -> Option<CompactionResult> {
        let split_point = self.messages.len() - self.config.retention_window;
        let removed: Vec<_> = self.messages.drain(..split_point).collect();

        let tokens_removed: usize = removed.iter().map(|m| m.token_count).sum();
        let messages_removed = removed.len();

        let summary = self.generate_summary(&removed);

        self.total_tokens -= tokens_removed;
        self.recount_turns();
        self.compaction_count += 1;

        Some(CompactionResult {
            messages_removed,
            tokens_saved: tokens_removed,
            summary,
            strategy: CompactionStrategy::FrontRemoval,
        })
    }

    /// Check if token usage exceeds the auto-compact threshold.
    ///
    /// Returns true when current usage exceeds `auto_compact_threshold` of `max_tokens`.
    pub fn should_auto_compact(&self, context_limit: usize) -> bool {
        if context_limit == 0 {
            return false;
        }
        let ratio = self.total_tokens as f64 / context_limit as f64;
        let threshold = self.config.auto_compact_threshold;
        threshold > 0.0 && threshold <= 1.0 && ratio > threshold
    }

    /// Compute how many tool call responses to summarize for a given context limit.
    ///
    /// Returns the number of tool responses that can be summarized to save space.
    pub fn tool_summarization_cutoff(&self, context_limit: usize) -> usize {
        let threshold = self.config.auto_compact_threshold;
        let effective_limit = (context_limit as f64 * threshold) as usize;
        (3 * effective_limit / 20_000).clamp(10, 500)
    }

    /// Middle-out index selection.
    ///
    /// Given a list of indices and a removal percentage, returns indices
    /// to remove by expanding outward from the middle. This preserves
    /// early context (system messages, first user message) and recent
    /// context (last few messages).
    fn middle_out_indices(indices: &[usize], remove_percent: u32) -> Vec<usize> {
        if indices.is_empty() || remove_percent == 0 {
            return Vec::new();
        }

        let num_to_remove = ((indices.len() * remove_percent as usize) / 100).max(1);
        let middle = indices.len() / 2;
        let mut result = Vec::with_capacity(num_to_remove);

        for i in 0..num_to_remove {
            if i % 2 == 0 {
                let offset = i / 2;
                if middle > offset {
                    result.push(indices[middle - offset - 1]);
                }
            } else {
                let offset = i / 2;
                if middle + offset < indices.len() {
                    result.push(indices[middle + offset]);
                }
            }
        }

        result
    }

    /// Recount turns from current messages.
    fn recount_turns(&mut self) {
        self.turn_count = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User || m.role == MessageRole::Assistant)
            .count();
    }

    /// Get current token count
    pub fn token_count(&self) -> usize {
        self.total_tokens
    }

    /// Get current turn count
    pub fn turn_count(&self) -> usize {
        self.turn_count
    }

    /// Get number of messages
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get compaction count
    pub fn compaction_count(&self) -> usize {
        self.compaction_count
    }

    /// Identify tool call IDs eligible for summarization.
    ///
    /// Returns tool call IDs that exceed the cutoff and should be summarized
    /// in batches. Protects the most recent `protect_last_n` tool calls from
    /// summarization to preserve current-turn context.
    ///
    /// Inspired by goose's `tool_ids_to_summarize` pattern.
    pub fn tool_ids_to_summarize(
        &self,
        cutoff: usize,
        protect_last_n: usize,
        batch_size: usize,
    ) -> Vec<String> {
        let mut tool_call_ids: Vec<String> = Vec::new();

        for (i, msg) in self.messages.iter().enumerate() {
            if msg.role != MessageRole::Tool {
                continue;
            }
            // Use index-based pseudo-ID for tracking
            tool_call_ids.push(format!("tool_{}", i));
        }

        // Never summarize the last N tool calls (current turn)
        let eligible = tool_call_ids.len().saturating_sub(protect_last_n);
        if eligible <= cutoff + batch_size {
            return Vec::new();
        }

        tool_call_ids.into_iter().take(batch_size).collect()
    }

    /// Get a reference to current messages
    pub fn messages(&self) -> &[ConversationMessage] {
        &self.messages
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.turn_count = 0;
        self.total_tokens = 0;
        self.messages.clear();
        self.compaction_count = 0;
    }

    fn generate_summary(&self, removed: &[ConversationMessage]) -> String {
        let mut parts = Vec::new();
        let mut current_role = None;
        let mut current_content = String::new();

        for msg in removed {
            if current_role.as_ref() != Some(&msg.role) {
                if !current_content.is_empty() {
                    parts.push(format!("{:?}", current_role.unwrap()));
                    parts.push(current_content.clone());
                }
                current_role = Some(msg.role.clone());
                current_content = msg.content.clone();
            } else {
                current_content.push('\n');
                current_content.push_str(&msg.content);
            }
        }

        if !current_content.is_empty() {
            parts.push(format!("{:?}", current_role.unwrap()));
            parts.push(current_content);
        }

        // Truncate summary if too long
        let summary = parts.join("\n");
        if summary.len() > 2000 {
            let truncated = match summary.is_char_boundary(2000) {
                true => &summary[..2000],
                false => {
                    let mut b = 2000;
                    while b > 0 && !summary.is_char_boundary(b) {
                        b -= 1;
                    }
                    &summary[..b]
                }
            };
            format!(
                "[Compacted {} messages, {} tokens] {}",
                removed.len(),
                removed.iter().map(|m| m.token_count).sum::<usize>(),
                truncated
            )
        } else {
            summary
        }
    }
}

/// Action to take after adding a message
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompactionAction {
    /// No action needed
    None,
    /// Compaction should be performed
    Compact,
}

impl CompactionAction {
    /// Check if compaction should be performed
    pub fn should_compact(&self) -> bool {
        matches!(self, CompactionAction::Compact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::Message;

    fn make_message(role: MessageRole, content: &str, tokens: usize) -> ConversationMessage {
        ConversationMessage {
            role,
            content: content.to_string(),
            token_count: tokens,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_no_compaction_under_threshold() {
        let config = CompactionConfig {
            max_messages: 20,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        for i in 0..10 {
            let action =
                compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
            assert_eq!(action, CompactionAction::None);
        }
    }

    #[test]
    fn test_compaction_triggers_on_message_count() {
        let config = CompactionConfig {
            max_messages: 10,
            retention_window: 3,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        for i in 0..10 {
            compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
        }

        assert!(compactor.should_compact());
    }

    #[test]
    fn test_compact_retains_recent_messages() {
        let config = CompactionConfig {
            max_messages: 10,
            retention_window: 3,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        for i in 0..10 {
            compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
        }

        let result = compactor.compact().unwrap();
        assert_eq!(result.messages_removed, 7);
        assert_eq!(compactor.message_count(), 3);
    }

    #[test]
    fn test_compaction_reduces_token_count() {
        let config = CompactionConfig {
            max_messages: 10,
            retention_window: 3,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        for i in 0..10 {
            compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
        }

        let _ = compactor.compact();
        assert_eq!(compactor.token_count(), 300); // 3 messages * 100 tokens each
    }

    #[test]
    fn test_no_compact_when_few_messages() {
        let config = CompactionConfig {
            max_messages: 100,
            retention_window: 10,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        for i in 0..5 {
            compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
        }

        assert!(compactor.compact().is_none());
    }

    #[test]
    fn test_turn_counting() {
        let config = CompactionConfig {
            max_turns: 5,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // Add user messages (should count as turns)
        for _i in 0..3 {
            compactor.add_message(make_message(MessageRole::User, "user", 100));
        }
        assert_eq!(compactor.turn_count(), 3);

        // Add tool messages (should not count as turns)
        compactor.add_message(make_message(MessageRole::Tool, "tool", 50));
        assert_eq!(compactor.turn_count(), 3);

        // Add assistant messages (should count as turns)
        compactor.add_message(make_message(MessageRole::Assistant, "assistant", 100));
        assert_eq!(compactor.turn_count(), 4);
    }

    #[test]
    fn test_compaction_on_turn_threshold() {
        let config = CompactionConfig {
            max_turns: 3,
            retention_window: 2,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // Add 4 user messages (4 turns)
        for _i in 0..4 {
            compactor.add_message(make_message(MessageRole::User, "user", 100));
        }

        assert!(compactor.should_compact());
    }

    #[test]
    fn test_compaction_on_token_threshold() {
        let config = CompactionConfig {
            max_tokens: 500,
            retention_window: 2,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // Add messages totaling 600 tokens
        for _i in 0..6 {
            compactor.add_message(make_message(MessageRole::User, "user", 100));
        }

        assert!(compactor.should_compact());
    }

    #[test]
    fn test_protocol_message_conversion() {
        let proto_msg = Message::user("Hello, world!");
        let conv_msg = ConversationMessage::from(proto_msg);

        assert_eq!(conv_msg.role, MessageRole::User);
        assert_eq!(conv_msg.content, "Hello, world!");
        assert!(conv_msg.token_count > 0);
    }

    #[test]
    fn test_compaction_action_should_compact() {
        assert!(!CompactionAction::None.should_compact());
        assert!(CompactionAction::Compact.should_compact());
    }

    #[test]
    fn test_reset() {
        let mut compactor = Compactor::new(CompactionConfig::default());

        compactor.add_message(make_message(MessageRole::User, "test", 100));
        assert_eq!(compactor.message_count(), 1);

        compactor.reset();
        assert_eq!(compactor.message_count(), 0);
        assert_eq!(compactor.token_count(), 0);
        assert_eq!(compactor.turn_count(), 0);
    }

    // ── Progressive Compaction Tests ──────────────────────────────────────

    #[test]
    fn test_progressive_compaction_removes_tool_responses() {
        let config = CompactionConfig {
            max_messages: 10,
            max_tokens: 1_000_000, // Don't trigger on tokens
            max_turns: 1_000_000,  // Don't trigger on turns
            retention_window: 2,
            progressive_compaction: true,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // Add conversation with tool responses
        compactor.add_message(make_message(MessageRole::User, "user msg", 50));
        compactor.add_message(make_message(MessageRole::Tool, "tool result 1", 200));
        compactor.add_message(make_message(MessageRole::Tool, "tool result 2", 200));
        compactor.add_message(make_message(MessageRole::Tool, "tool result 3", 200));
        compactor.add_message(make_message(MessageRole::Assistant, "response", 50));
        compactor.add_message(make_message(MessageRole::User, "follow-up", 50));
        compactor.add_message(make_message(MessageRole::Tool, "tool result 4", 200));
        compactor.add_message(make_message(MessageRole::Assistant, "final", 50));

        // Force compaction by exceeding max_messages
        compactor.add_message(make_message(MessageRole::User, "overflow", 50));
        compactor.add_message(make_message(MessageRole::User, "overflow2", 50));

        assert!(compactor.should_compact());
        let result = compactor.compact();

        assert!(result.is_some());
        let r = result.unwrap();
        // Should have removed some tool responses
        assert!(r.messages_removed > 0);
        assert!(r.tokens_saved > 0);
        // Should use progressive strategy
        assert!(matches!(
            r.strategy,
            CompactionStrategy::ProgressiveToolRemoval(_)
        ));
    }

    #[test]
    fn test_middle_out_indices() {
        let indices: Vec<usize> = vec![2, 5, 8, 12, 15, 20, 25];

        // Remove 50% = 3 items from middle
        let result = Compactor::middle_out_indices(&indices, 50);
        assert!(result.len() >= 3);
        // All returned values should be valid indices from the input
        for idx in &result {
            assert!(indices.contains(idx));
        }
    }

    #[test]
    fn test_middle_out_indices_empty() {
        let indices: Vec<usize> = vec![];
        let result = Compactor::middle_out_indices(&indices, 50);
        assert!(result.is_empty());
    }

    #[test]
    fn test_middle_out_preserves_edges() {
        let indices: Vec<usize> = vec![0, 10, 20, 30, 40, 50, 60, 70, 80, 90];

        // Remove 20% = 2 items from middle
        let result = Compactor::middle_out_indices(&indices, 20);

        // Middle-out should prefer middle indices
        // Should not remove the very first or very last
        assert!(!result.contains(&0));
        assert!(!result.contains(&90));
    }

    #[test]
    fn test_should_auto_compact_threshold() {
        let config = CompactionConfig {
            max_tokens: 1000,
            auto_compact_threshold: 0.8,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // Add 500 tokens (50% of 1000) - should NOT auto-compact
        compactor.add_message(make_message(MessageRole::User, "msg", 500));
        assert!(!compactor.should_auto_compact(1000));

        // Add 400 more tokens (total 900 = 90% of 1000) - SHOULD auto-compact
        compactor.add_message(make_message(MessageRole::User, "msg2", 400));
        assert!(compactor.should_auto_compact(1000));
    }

    #[test]
    fn test_compaction_strategy_default_has_progressive() {
        let config = CompactionConfig::default();
        assert!(config.progressive_compaction);
        assert_eq!(config.auto_compact_threshold, 0.8);
    }

    #[test]
    fn test_tool_summarization_cutoff() {
        let config = CompactionConfig::default();
        let compactor = Compactor::new(config);

        let cutoff = compactor.tool_summarization_cutoff(100_000);
        assert!(cutoff >= 10);
        assert!(cutoff <= 500);
    }

    #[test]
    fn test_tool_ids_to_summarize_basic() {
        let config = CompactionConfig {
            max_messages: 100,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // Add user msg + 16 tool msgs + user msg
        compactor.add_message(make_message(MessageRole::User, "start", 10));
        for i in 0..16 {
            compactor.add_message(make_message(MessageRole::Tool, &format!("tool_{}", i), 100));
        }
        compactor.add_message(make_message(MessageRole::User, "end", 10));

        // cutoff=5: 16 eligible, 16 > 5+10 → batch of 10
        let result = compactor.tool_ids_to_summarize(5, 0, DEFAULT_BATCH_SIZE);
        assert_eq!(result.len(), DEFAULT_BATCH_SIZE);
    }

    #[test]
    fn test_tool_ids_to_summarize_protects_current_turn() {
        let config = CompactionConfig {
            max_messages: 100,
            ..Default::default()
        };
        let mut compactor = Compactor::new(config);

        // 20 tool messages
        compactor.add_message(make_message(MessageRole::User, "start", 10));
        for i in 0..20 {
            compactor.add_message(make_message(MessageRole::Tool, &format!("tool_{}", i), 100));
        }

        // Protect last 8: 12 eligible, 12 <= 2+10 → nothing
        let result = compactor.tool_ids_to_summarize(2, 8, DEFAULT_BATCH_SIZE);
        assert!(result.is_empty(), "Should not summarize when protected");

        // Protect last 7: 13 eligible, 13 > 2+10 → batch
        let result = compactor.tool_ids_to_summarize(2, 7, DEFAULT_BATCH_SIZE);
        assert_eq!(result.len(), DEFAULT_BATCH_SIZE);
    }

    #[test]
    fn test_tool_ids_to_summarize_no_tools() {
        let config = CompactionConfig::default();
        let mut compactor = Compactor::new(config);

        compactor.add_message(make_message(MessageRole::User, "hello", 10));
        compactor.add_message(make_message(MessageRole::Assistant, "hi", 10));

        let result = compactor.tool_ids_to_summarize(5, 0, DEFAULT_BATCH_SIZE);
        assert!(result.is_empty());
    }
}
