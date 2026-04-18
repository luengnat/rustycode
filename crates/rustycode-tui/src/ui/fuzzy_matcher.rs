//! Shared Fuzzy Matching Module
//!
//! This module provides a common fuzzy matching algorithm used across the TUI
//! for various search/filter functionality (file finder, model selector, etc.).
//!
//! ## Features
//!
//! - **Flexible scoring**: Supports different match types (exact, prefix, substring)
//! - **Case-insensitive**: All matching is done case-insensitively
//! - **Highlight support**: Helper for highlighting matched text in UI
//! - **Ranking**: Sort results by relevance score

// Complete implementation - pending integration with search/filter components
#![allow(dead_code)]

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// ============================================================================
// MATCH SCORE
// ============================================================================

/// Match relevance score for ranking search results
///
/// Higher scores indicate better matches. Scores are ordered from worst to best:
/// None < Substring < Prefix < Exact
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MatchScore {
    /// No match
    None = 0,
    /// Substring match (query found anywhere in text)
    Substring = 1,
    /// Prefix match (query at start of text)
    Prefix = 2,
    /// Exact match (query equals text)
    Exact = 3,
}

impl MatchScore {
    /// Check if this score represents any match (non-None)
    #[inline]
    pub fn is_match(self) -> bool {
        self != MatchScore::None
    }

    /// Get a numeric value for this score
    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

// ============================================================================
// FUZZY MATCHER
// ============================================================================

/// Generic fuzzy matcher for searching collections of items
///
/// This matcher provides case-insensitive fuzzy matching with relevance scoring.
/// It can be used to search strings, names, paths, etc.
#[derive(Debug, Clone)]
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Create a new fuzzy matcher
    pub fn new() -> Self {
        Self
    }

    /// Calculate match score for a query against a single text string
    ///
    /// # Arguments
    ///
    /// * `query` - The search query
    /// * `text` - The text to match against
    ///
    /// # Returns
    ///
    /// A `MatchScore` indicating the quality of the match
    pub fn match_score(&self, query: &str, text: &str) -> MatchScore {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        // Empty query matches everything
        if query.is_empty() {
            return MatchScore::Substring;
        }

        // Exact match
        if text_lower == query_lower {
            return MatchScore::Exact;
        }

        // Prefix match
        if text_lower.starts_with(&query_lower) {
            return MatchScore::Prefix;
        }

        // Substring match
        if text_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        MatchScore::None
    }

    /// Calculate match score for a query against multiple text fields
    ///
    /// This is useful when you want to search across multiple properties
    /// (e.g., name and description). Returns the highest score from all fields.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query
    /// * `fields` - Slice of text fields to search
    ///
    /// # Returns
    ///
    /// The highest `MatchScore` from all fields
    pub fn match_score_multi(&self, query: &str, fields: &[&str]) -> MatchScore {
        fields
            .iter()
            .map(|&field| self.match_score(query, field))
            .max()
            .unwrap_or(MatchScore::None)
    }

    /// Filter and index items by query using a scoring function
    ///
    /// # Arguments
    ///
    /// * `query` - The search query
    /// * `items` - Slice of items to filter
    /// * `score_fn` - Function that returns a match score for each item
    ///
    /// # Returns
    ///
    /// A vector of (index, score) tuples sorted by score (descending)
    pub fn filter_and_rank<T, F>(
        &self,
        _query: &str,
        items: &[T],
        score_fn: F,
    ) -> Vec<(usize, MatchScore)>
    where
        F: Fn(&T) -> MatchScore,
    {
        let mut matches: Vec<(usize, MatchScore)> = items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let score = score_fn(item);
                if score.is_match() {
                    Some((idx, score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (descending)
        matches.sort_by_key(|a| std::cmp::Reverse(a.1));

        matches
    }

    /// Highlight matching characters in text for UI display
    ///
    /// Returns a `Line` with matching portions highlighted in yellow/bold.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to highlight
    /// * `query` - The query to highlight (all occurrences)
    ///
    /// # Returns
    ///
    /// A `Line` with highlighted spans
    pub fn highlight_matches(&self, text: &str, query: &str) -> Line<'_> {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        if query.is_empty() {
            return Line::from(text.to_string());
        }

        let mut spans = Vec::new();
        let mut last_idx = 0;

        // Find all matches
        while let Some(idx) = text_lower[last_idx..].find(&query_lower) {
            let absolute_idx = last_idx + idx;

            // Add text before match
            if absolute_idx > last_idx {
                let before = &text[last_idx..absolute_idx];
                spans.push(Span::raw(before.to_string()));
            }

            // Add highlighted match
            let match_end = absolute_idx + query.len();
            let matched = &text[absolute_idx..match_end];
            spans.push(Span::styled(
                matched.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

            last_idx = match_end;
        }

        // Add remaining text
        if last_idx < text.len() {
            let remaining = &text[last_idx..];
            spans.push(Span::raw(remaining.to_string()));
        }

        // If no matches found, return original text
        if spans.is_empty() {
            Line::from(text.to_string())
        } else {
            Line::from(spans)
        }
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_score_exact() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("test", "test"), MatchScore::Exact);
        assert_eq!(matcher.match_score("Test", "test"), MatchScore::Exact); // Case-insensitive
    }

    #[test]
    fn test_match_score_prefix() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("tes", "test"), MatchScore::Prefix);
        assert_eq!(matcher.match_score("Tes", "test"), MatchScore::Prefix);
    }

    #[test]
    fn test_match_score_substring() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("es", "test"), MatchScore::Substring);
        assert_eq!(matcher.match_score("st", "test"), MatchScore::Substring);
    }

    #[test]
    fn test_match_score_none() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("xyz", "test"), MatchScore::None);
    }

    #[test]
    fn test_match_score_empty_query() {
        let matcher = FuzzyMatcher::new();
        // Empty query matches everything (returns Substring as default)
        assert_eq!(matcher.match_score("", "test"), MatchScore::Substring);
    }

    #[test]
    fn test_match_score_multi() {
        let matcher = FuzzyMatcher::new();
        let fields = vec!["test", "example", "demo"];

        // Should match "test" exactly
        assert_eq!(
            matcher.match_score_multi("test", &fields),
            MatchScore::Exact
        );

        // Should match "test" as prefix
        assert_eq!(
            matcher.match_score_multi("tes", &fields),
            MatchScore::Prefix
        );

        // Should match "example" as substring
        assert_eq!(
            matcher.match_score_multi("xa", &fields),
            MatchScore::Substring
        );

        // No match in any field
        assert_eq!(matcher.match_score_multi("xyz", &fields), MatchScore::None);
    }

    #[test]
    fn test_filter_and_rank() {
        let matcher = FuzzyMatcher::new();
        let items = vec!["test", "testing", "example", "contest"];

        let results =
            matcher.filter_and_rank("test", &items, |item| matcher.match_score("test", item));

        // Should return all matches sorted by score
        assert!(results.len() >= 2);

        // First result should be exact match ("test")
        let (first_idx, first_score) = &results[0];
        assert_eq!(*first_score, MatchScore::Exact);
        assert_eq!(items[*first_idx], "test");
    }

    #[test]
    fn test_highlight_matches() {
        let matcher = FuzzyMatcher::new();
        let line = matcher.highlight_matches("test example", "es");

        // Should create spans with highlighted "es" portions
        let spans = line.spans;
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_matches_empty_query() {
        let matcher = FuzzyMatcher::new();
        let line = matcher.highlight_matches("test", "");

        // Should return original text unchanged
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_match_score_is_match() {
        assert!(MatchScore::Exact.is_match());
        assert!(MatchScore::Prefix.is_match());
        assert!(MatchScore::Substring.is_match());
        assert!(!MatchScore::None.is_match());
    }

    #[test]
    fn test_match_score_as_u8() {
        assert_eq!(MatchScore::None.as_u8(), 0);
        assert_eq!(MatchScore::Substring.as_u8(), 1);
        assert_eq!(MatchScore::Prefix.as_u8(), 2);
        assert_eq!(MatchScore::Exact.as_u8(), 3);
    }

    #[test]
    fn test_match_score_ordering() {
        assert!(MatchScore::Exact > MatchScore::Prefix);
        assert!(MatchScore::Prefix > MatchScore::Substring);
        assert!(MatchScore::Substring > MatchScore::None);
    }
}
