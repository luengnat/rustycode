// ── Budget Enforcement Functions ───────────────────────────────────────────────

/// Enforce budget constraints on a collection of items.
///
/// # Type Parameters
///
/// * `T` - Item type (must provide token count via callback)
///
/// # Example
///
/// ```
/// use rustycode_core::context::{enforce_budget, TokenCounter};
///
/// let items = vec!["short", "medium text", "a very long piece of text that uses many tokens"];
/// let budget = 50;
///
/// let selected = enforce_budget(&items, budget, |item| TokenCounter::estimate_tokens(item))?;
/// assert!(selected.len() <= items.len());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn enforce_budget<T, F>(items: &[T], budget: usize, token_fn: F) -> anyhow::Result<Vec<&T>>
where
    F: Fn(&T) -> usize,
{
    let mut result = Vec::new();
    let mut used: usize = 0;

    for item in items {
        let tokens = token_fn(item);
        let new_total = used.saturating_add(tokens);

        if new_total <= budget {
            result.push(item);
            used = new_total;
        } else {
            // Budget exhausted, stop adding items
            break;
        }
    }

    Ok(result)
}

/// Enforce budget with priority ordering (higher priority items first).
///
/// Items should be pre-sorted by priority (highest first). This function
/// will include as many high-priority items as possible before moving
/// to lower-priority ones.
///
/// # Type Parameters
///
/// * `T` - Item type (must provide token count via callback)
///
/// # Example
///
/// ```
/// use rustycode_core::context::{enforce_budget_prioritized, TokenCounter};
///
/// let items = vec![
///     ("low priority", "some text"),
///     ("high priority", "important text"),
///     ("medium priority", "other text"),
/// ];
///
/// // Pre-sort by priority (higher first)
/// let mut sorted = items.clone();
/// sorted.sort_by(|a, b| b.0.cmp(&a.0));
///
/// let budget = 50;
/// let selected = enforce_budget_prioritized(&sorted, budget, |item| {
///     TokenCounter::estimate_tokens(item.1)
/// })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn enforce_budget_prioritized<T, F>(
    items: &[T],
    budget: usize,
    token_fn: F,
) -> anyhow::Result<Vec<&T>>
where
    F: Fn(&T) -> usize,
{
    let mut result = Vec::new();
    let mut used: usize = 0;

    for item in items {
        let tokens = token_fn(item);
        let new_total = used.saturating_add(tokens);

        if new_total <= budget {
            result.push(item);
            used = new_total;
        }
        // Skip items that don't fit (lower priority items get dropped)
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enforce_budget() {
        let items = vec!["short", "medium text", "very long text here"];
        let budget = 20; // Approximate

        let selected = enforce_budget(&items, budget, |s| s.len()).unwrap();
        assert!(!selected.is_empty());
        assert!(selected.len() <= items.len());
    }

    #[test]
    fn test_enforce_budget_empty() {
        let items: Vec<&str> = vec![];
        let budget = 100;

        let selected = enforce_budget(&items, budget, |s| s.len()).unwrap();
        assert!(selected.is_empty());
    }

    #[test]
    fn test_enforce_budget_zero_budget() {
        let items = vec!["test"];
        let budget = 0;

        let selected = enforce_budget(&items, budget, |s| s.len()).unwrap();
        assert!(selected.is_empty());
    }

    #[test]
    fn test_enforce_budget_prioritized() {
        let items = vec![
            ("low", "long text content here"),
            ("high", "short"),
            ("medium", "text"),
        ];

        // Pre-sort by priority (high first)
        let mut sorted = items.clone();
        sorted.sort_by(|a, b| b.0.cmp(a.0));

        let budget = 10;
        let selected = enforce_budget_prioritized(&sorted, budget, |item| item.1.len()).unwrap();

        // Should include high-priority items first
        assert!(!selected.is_empty());
    }
}
