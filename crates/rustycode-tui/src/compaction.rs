//! Token autocompaction for RustyCode TUI
//!
//! Monitors context usage and automatically compacts to prevent hitting limits
//! while preserving important information.

use crate::ui::message::{Message, MessageRole};
use rustycode_providers::predefined;
use std::time::Instant;

const MAX_CONSECUTIVE_FAILURES: u32 = 3;

#[derive(Clone, Debug, Default)]
pub struct AutoCompactState {
    pub compaction_count: u32,
    pub consecutive_failures: u32,
    pub disabled: bool,
}

impl AutoCompactState {
    pub fn on_success(&mut self) {
        self.compaction_count += 1;
        self.consecutive_failures = 0;
    }

    pub fn on_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            self.disabled = true;
        }
    }
}

/// Configuration for context monitoring and compaction
#[derive(Clone, Debug)]
pub struct CompactionConfig {
    /// Maximum tokens allowed in context
    pub max_tokens: usize,
    /// Warning threshold (0.0-1.0, e.g., 0.8 = 80%)
    pub warning_threshold: f64,
    /// Number of recent messages to keep intact
    pub keep_recent_count: usize,
    /// Whether auto-compaction is enabled
    pub auto_compact_enabled: bool,
    /// Compaction strategy (aggressive vs conservative)
    pub strategy: CompactionStrategy,
    /// Current model ID for model-aware context window sizing
    pub model_id: Option<String>,
    /// Circuit breaker state for auto-compaction
    pub auto_compact_state: AutoCompactState,
}

impl CompactionConfig {
    pub fn effective_max_tokens(&self) -> usize {
        self.model_id
            .as_deref()
            .map(predefined::context_window_for_model)
            .unwrap_or(self.max_tokens)
    }
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100_000,
            warning_threshold: 0.8,
            keep_recent_count: 50,
            auto_compact_enabled: true,
            strategy: CompactionStrategy::Balanced,
            model_id: None,
            auto_compact_state: AutoCompactState::default(),
        }
    }
}

/// Compaction strategy
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum CompactionStrategy {
    /// Keep last 20 messages (aggressive)
    Aggressive,
    /// Keep last 50 messages (balanced)
    Balanced,
    /// Keep last 100 messages (conservative)
    Conservative,
}

impl CompactionStrategy {
    /// Get the number of recent messages to keep
    pub fn keep_count(&self) -> usize {
        match self {
            CompactionStrategy::Aggressive => 20,
            CompactionStrategy::Balanced => 50,
            CompactionStrategy::Conservative => 100,
        }
    }
}

/// Context token usage monitor
#[derive(Clone, Debug)]
pub struct ContextMonitor {
    /// Current estimated token count
    pub current_tokens: usize,
    /// Maximum tokens allowed
    pub max_tokens: usize,
    /// Warning threshold (0.0-1.0)
    pub warning_threshold: f64,
    /// Last update time
    pub last_update: Instant,
    /// Whether compaction is needed
    pub needs_compaction: bool,
}

impl ContextMonitor {
    /// Create a new context monitor
    pub fn new(max_tokens: usize, warning_threshold: f64) -> Self {
        Self {
            current_tokens: 0,
            max_tokens,
            warning_threshold,
            last_update: Instant::now(),
            needs_compaction: false,
        }
    }

    /// Count tokens in messages (approximate: 1 token ≈ 4 characters)
    ///
    /// Accounts for all content that consumes context: message text,
    /// thinking blocks, and tool execution inputs/outputs.
    pub fn count_tokens(&self, messages: &[Message]) -> usize {
        let mut total_chars: usize = 0;
        for m in messages {
            // Main message content
            total_chars += m.content.len();
            // Thinking blocks (can be very large for extended thinking models)
            if let Some(ref thinking) = m.thinking {
                total_chars += thinking.len();
            }
            // Tool execution metadata and outputs (often the biggest context consumer)
            if let Some(ref tools) = m.tool_executions {
                for t in tools {
                    total_chars += t.name.len() + t.result_summary.len();
                    if let Some(ref output) = t.detailed_output {
                        total_chars += output.len();
                    }
                }
            }
        }
        // Approximate: 1 token ≈ 4 characters
        (total_chars / 4).max(1)
    }

    /// Update token count from messages
    pub fn update(&mut self, messages: &[Message]) {
        self.current_tokens = self.count_tokens(messages);
        self.last_update = Instant::now();
        self.needs_compaction = self.usage_percentage() >= self.warning_threshold;
    }

    /// Get current usage as percentage
    pub fn usage_percentage(&self) -> f64 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        self.current_tokens as f64 / self.max_tokens as f64
    }

    /// Check if compaction is needed
    pub fn should_compact(&self) -> bool {
        self.needs_compaction
    }

    /// Get remaining tokens
    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.current_tokens)
    }

    /// Get color code for usage (for UI)
    pub fn usage_color(&self) -> UsageColor {
        let pct = self.usage_percentage();
        if pct < 0.5 {
            UsageColor::Green
        } else if pct < 0.8 {
            UsageColor::Yellow
        } else {
            UsageColor::Red
        }
    }
}

/// Usage color for UI display
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum UsageColor {
    Green,
    Yellow,
    Red,
}

/// Compaction preview information
#[derive(Clone, Debug)]
pub struct CompactionPreview {
    /// Current token count
    pub current_tokens: usize,
    /// Max tokens
    pub max_tokens: usize,
    /// Number of messages to compact
    pub messages_to_compact: usize,
    /// Number of recent messages to keep
    pub recent_to_keep: usize,
    /// Number of tool results to preserve
    pub tool_results_count: usize,
    /// Number of error messages to preserve
    pub error_count: usize,
    /// Estimated tokens saved
    pub estimated_savings: usize,
    /// New token count after compaction
    pub new_token_count: usize,
}

impl CompactionPreview {
    /// Create a compaction preview
    pub fn new(
        current_tokens: usize,
        max_tokens: usize,
        messages: &[Message],
        strategy: CompactionStrategy,
    ) -> Self {
        let keep_count = strategy.keep_count();
        let total_messages = messages.len();

        let messages_to_compact = total_messages.saturating_sub(keep_count);

        let old_messages: Vec<_> = if total_messages > keep_count {
            messages.iter().rev().skip(keep_count).collect()
        } else {
            vec![]
        };

        let tool_results_count = old_messages
            .iter()
            .filter(|m| matches!(m.role, MessageRole::System))
            .count();

        let error_count = old_messages
            .iter()
            .filter(|m| m.content.to_lowercase().contains("error"))
            .count();

        // Estimate savings: assume compaction reduces old messages by 70%
        let old_tokens = messages_to_compact * 100; // rough estimate
        let estimated_savings = (old_tokens as f64 * 0.7) as usize;
        let new_token_count = current_tokens.saturating_sub(estimated_savings);

        Self {
            current_tokens,
            max_tokens,
            messages_to_compact,
            recent_to_keep: keep_count,
            tool_results_count,
            error_count,
            estimated_savings,
            new_token_count,
        }
    }

    /// Format as display text
    pub fn format(&self) -> String {
        let fmt = |n: usize| -> String {
            if n >= 1_000_000 {
                format!("{:.1}M", n as f64 / 1_000_000.0)
            } else if n >= 1_000 {
                format!("{:.0}k", n as f64 / 1_000.0)
            } else {
                n.to_string()
            }
        };
        let pct = (self.current_tokens as f64 / self.max_tokens as f64) * 100.0;
        let new_pct = (self.new_token_count as f64 / self.max_tokens as f64) * 100.0;
        format!(
            "⚠ Context at {:.0}% ({}/{})\n\nCompaction plan:\n  Keep last {} messages intact\n  Summarize {} older messages\n  Preserve {} tool results, {} errors\n\nEstimated: {} → {} ({:.0}%)\n\n[Enter to compact] [Esc to cancel]",
            pct,
            fmt(self.current_tokens),
            fmt(self.max_tokens),
            self.recent_to_keep,
            self.messages_to_compact,
            self.tool_results_count,
            self.error_count,
            fmt(self.current_tokens),
            fmt(self.new_token_count),
            new_pct
        )
    }
}

/// Compact messages to reduce token count
pub fn compact_context(messages: Vec<Message>, strategy: CompactionStrategy) -> Vec<Message> {
    let keep_count = strategy.keep_count();

    if messages.len() <= keep_count {
        return messages;
    }

    let mut result = Vec::new();

    // 1. Keep last N messages intact (in reverse for collection)
    let recent: Vec<_> = messages.iter().rev().take(keep_count).cloned().collect();

    // 2. Get older messages
    let old: Vec<&Message> = messages.iter().rev().skip(keep_count).collect();

    // 3. Summarize older messages if any
    if !old.is_empty() {
        let summary = summarize_messages(&old);
        result.push(summary);

        // 4. Preserve tool results (system messages that contain tool outputs)
        let tools: Vec<Message> = old
            .iter()
            .filter(|m| matches!(m.role, MessageRole::System))
            .map(|m| (*m).clone())
            .collect();
        result.extend(tools);

        // 5. Keep error messages (messages with "error" in content, case-insensitive)
        let errors: Vec<Message> = old
            .iter()
            .filter(|m| m.content.to_lowercase().contains("error"))
            .map(|m| (*m).clone())
            .collect();
        result.extend(errors);
    }

    // 6. Add recent messages back in correct order
    result.extend(recent.into_iter().rev());

    result
}

/// Summarize old messages into key points
fn summarize_messages(messages: &[&Message]) -> Message {
    let key_points = extract_key_points(messages);

    let content = if key_points.is_empty() {
        format!("Summary of {} messages (older context)", messages.len())
    } else {
        format!(
            "Summary of {} messages:\n{}",
            messages.len(),
            key_points.join("\n• ")
        )
    };

    Message::new(MessageRole::System, content)
}

/// Extract key points from messages
fn extract_key_points(messages: &[&Message]) -> Vec<String> {
    let mut points = Vec::new();

    for msg in messages {
        // Skip system messages and empty content
        if matches!(msg.role, MessageRole::System) || msg.content.trim().is_empty() {
            continue;
        }

        // Extract user requests and AI responses
        let content_preview = if msg.content.len() > 100 {
            format!("{}...", msg.content.chars().take(94).collect::<String>())
        } else {
            msg.content.clone()
        };

        let role_prefix = match msg.role {
            MessageRole::User => "User asked",
            MessageRole::Assistant => "AI responded",
            MessageRole::System => continue,
        };

        points.push(format!("{}: {}", role_prefix, content_preview));

        // Limit to 10 key points
        if points.len() >= 10 {
            break;
        }
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_message(role: MessageRole, content: &str) -> Message {
        Message::new(role, content.to_string())
    }

    #[test]
    fn test_context_monitor_new() {
        let monitor = ContextMonitor::new(100_000, 0.8);
        assert_eq!(monitor.max_tokens, 100_000);
        assert_eq!(monitor.warning_threshold, 0.8);
        assert_eq!(monitor.current_tokens, 0);
        assert!(!monitor.needs_compaction);
    }

    #[test]
    fn test_context_monitor_count_tokens() {
        let monitor = ContextMonitor::new(100_000, 0.8);
        let messages = vec![
            create_test_message(MessageRole::User, "Hello world"),
            create_test_message(MessageRole::Assistant, "Hi there!"),
        ];

        let count = monitor.count_tokens(&messages);
        assert!(count > 0);
        assert_eq!(count, (20 / 4)); // "Hello world" (11) + "Hi there!" (9) = 20 chars
    }

    #[test]
    fn test_context_monitor_update() {
        let mut monitor = ContextMonitor::new(1000, 0.8);
        let messages = vec![create_test_message(MessageRole::User, "Test message")];

        monitor.update(&messages);
        assert!(monitor.current_tokens > 0);
        assert!(!monitor.needs_compaction); // Should be under threshold
    }

    #[test]
    fn test_context_monitor_usage_percentage() {
        let mut monitor = ContextMonitor::new(1000, 0.5);
        let messages = vec![
            create_test_message(MessageRole::User, &"x".repeat(400)), // ~100 tokens
        ];

        monitor.update(&messages);
        let pct = monitor.usage_percentage();
        assert!(pct > 0.0);
        assert!(pct <= 1.0);
    }

    #[test]
    fn test_context_monitor_should_compact() {
        let mut monitor = ContextMonitor::new(1000, 0.5);
        let messages = vec![
            create_test_message(MessageRole::User, &"x".repeat(600)), // ~150 tokens = 15%
        ];

        monitor.update(&messages);
        assert!(!monitor.should_compact());

        // Add more to exceed threshold
        let messages = vec![
            create_test_message(MessageRole::User, &"x".repeat(3000)), // ~750 tokens = 75%
        ];
        monitor.update(&messages);
        assert!(monitor.should_compact());
    }

    #[test]
    fn test_usage_color() {
        let mut monitor = ContextMonitor::new(1000, 0.8);

        // Green: < 50% (less than 500 tokens)
        let messages = vec![
            create_test_message(MessageRole::User, &"x".repeat(200)), // 50 tokens = 5%
        ];
        monitor.update(&messages);
        assert_eq!(monitor.usage_color(), UsageColor::Green);

        // Yellow: 50-80% (500-799 tokens)
        let messages = vec![
            create_test_message(MessageRole::User, &"x".repeat(2400)), // 600 tokens = 60%
        ];
        monitor.update(&messages);
        assert_eq!(monitor.usage_color(), UsageColor::Yellow);

        // Red: > 80% (800+ tokens)
        let messages = vec![
            create_test_message(MessageRole::User, &"x".repeat(3400)), // 850 tokens = 85%
        ];
        monitor.update(&messages);
        assert_eq!(monitor.usage_color(), UsageColor::Red);
    }

    #[test]
    fn test_compaction_strategy_keep_count() {
        assert_eq!(CompactionStrategy::Aggressive.keep_count(), 20);
        assert_eq!(CompactionStrategy::Balanced.keep_count(), 50);
        assert_eq!(CompactionStrategy::Conservative.keep_count(), 100);
    }

    #[test]
    fn test_compact_context_no_compaction_needed() {
        let messages = vec![
            create_test_message(MessageRole::User, "Message 1"),
            create_test_message(MessageRole::Assistant, "Response 1"),
        ];

        let result = compact_context(messages, CompactionStrategy::Balanced);
        assert_eq!(result.len(), 2); // No change
    }

    #[test]
    fn test_compact_context_with_compaction() {
        let messages: Vec<Message> = (0..60)
            .map(|i| {
                if i % 2 == 0 {
                    create_test_message(MessageRole::User, &format!("User message {}", i))
                } else {
                    create_test_message(
                        MessageRole::Assistant,
                        &format!("Assistant response {}", i),
                    )
                }
            })
            .collect();

        let result = compact_context(messages, CompactionStrategy::Balanced);
        // Should keep last 50 + 1 summary
        assert!(result.len() <= 51);
        assert_eq!(result[0].role, MessageRole::System); // First should be summary
    }

    #[test]
    fn test_summarize_messages() {
        let messages = [
            create_test_message(MessageRole::User, "How do I create a file?"),
            create_test_message(MessageRole::Assistant, "You can use the write_file tool"),
            create_test_message(MessageRole::User, "What about reading?"),
            create_test_message(MessageRole::Assistant, "Use the read_file tool"),
        ];

        let message_refs: Vec<&Message> = messages.iter().collect();
        let summary = summarize_messages(&message_refs);

        assert_eq!(summary.role, MessageRole::System);
        assert!(summary.content.contains("Summary of 4 messages"));
    }

    #[test]
    fn test_extract_key_points() {
        let messages = [
            create_test_message(MessageRole::User, "How do I create a file?"),
            create_test_message(MessageRole::Assistant, "You can use the write_file tool"),
        ];

        let message_refs: Vec<&Message> = messages.iter().collect();
        let points = extract_key_points(&message_refs);

        assert_eq!(points.len(), 2);
        assert!(points[0].contains("User asked"));
        assert!(points[1].contains("AI responded"));
    }

    #[test]
    fn test_compaction_preview() {
        let messages: Vec<Message> = (0..100)
            .map(|i| create_test_message(MessageRole::User, &format!("Message {}", i)))
            .collect();

        let preview =
            CompactionPreview::new(80_000, 100_000, &messages, CompactionStrategy::Balanced);

        assert_eq!(preview.current_tokens, 80_000);
        assert_eq!(preview.max_tokens, 100_000);
        assert_eq!(preview.messages_to_compact, 50); // 100 - 50
        assert_eq!(preview.recent_to_keep, 50);
        assert!(preview.estimated_savings > 0);
        assert!(preview.new_token_count < preview.current_tokens);
    }

    #[test]
    fn test_compaction_config_default() {
        let config = CompactionConfig::default();
        assert_eq!(config.max_tokens, 100_000);
        assert_eq!(config.warning_threshold, 0.8);
        assert_eq!(config.keep_recent_count, 50);
        assert!(config.auto_compact_enabled);
        assert_eq!(config.strategy, CompactionStrategy::Balanced);
    }
}
