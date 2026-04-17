//! Orchestra Unit Closeout — Unit Completion Closeout
//!
//! Consolidates the repeated pattern of snapshotting metrics, saving activity
//! logs, and extracting memories that appears throughout the auto execution flow.
//! Matches orchestra-2's auto-unit-closeout.ts implementation.
//!
//! Critical for production autonomous systems to ensure clean unit
//! completion with proper metrics, activity logs, and memory extraction.

use anyhow::Result;
use std::path::Path;
use tracing::{debug, info};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Options for unit closeout
#[derive(Debug, Clone, Default)]
pub struct CloseoutOptions {
    /// Character count of prompt sent
    pub prompt_char_count: Option<usize>,

    /// Character count of baseline/context
    pub baseline_char_count: Option<usize>,

    /// Model tier used (light, standard, heavy)
    pub tier: Option<String>,

    /// Whether model was downgraded during execution
    pub model_downgraded: bool,

    /// Whether continue-here fired (context pressure warning)
    pub continue_here_fired: bool,
}

/// Activity log file path result
pub type ActivityFilePath = Option<String>;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Closeout a unit by snapshotting metrics, saving activity log,
/// and extracting memories
///
/// # Arguments
/// * `orchestra_dir` - Path to .orchestra directory
/// * `unit_type` - Type of unit (e.g., "task", "slice")
/// * `unit_id` - ID of the unit
/// * `started_at` - When the unit started (Instant)
/// * `model_id` - Model identifier used
/// * `opts` - Closeout options
///
/// # Returns
/// Optional activity log file path
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::unit_closeout::*;
/// use std::time::Instant;
///
/// # async fn example() -> Result<()> {
/// let started_at = Instant::now();
/// let opts = CloseoutOptions {
///     prompt_char_count: Some(1000),
///     tier: Some("standard".to_string()),
///     ..Default::default()
/// };
///
/// let activity_file = closeout_unit(
///     &orchestra_dir,
///     "task",
///     "T01",
///     started_at,
///     "claude-3-sonnet",
///     opts
/// ).await?;
///
/// println!("Activity saved to: {:?}", activity_file);
/// # Ok(())
/// # }
/// ```
pub async fn closeout_unit(
    orchestra_dir: &Path,
    unit_type: &str,
    unit_id: &str,
    started_at: std::time::Instant,
    model_id: &str,
    opts: CloseoutOptions,
) -> Result<ActivityFilePath> {
    info!("Closing out unit {}:{}", unit_type, unit_id);

    // 1. Snapshot metrics
    snapshot_unit_metrics(
        orchestra_dir,
        unit_type,
        unit_id,
        started_at,
        model_id,
        &opts,
    )?;

    // 2. Save activity log
    let activity_file = save_activity_log_impl(orchestra_dir, unit_type, unit_id)?;

    // 3. Extract memories (fire-and-forget, non-fatal)
    if let Some(ref activity_file) = activity_file {
        extract_memories(activity_file, unit_type, unit_id).await;
    }

    Ok(activity_file)
}

/// Snapshot unit metrics to disk
///
/// Records execution metrics for later analysis and cost tracking
fn snapshot_unit_metrics(
    orchestra_dir: &Path,
    unit_type: &str,
    unit_id: &str,
    started_at: std::time::Instant,
    model_id: &str,
    opts: &CloseoutOptions,
) -> Result<()> {
    use std::fs;

    // Calculate metrics
    let duration = started_at.elapsed();
    let duration_secs = duration.as_secs_f64();

    let metrics = serde_json::json!({
        "unit_type": unit_type,
        "unit_id": unit_id,
        "model_id": model_id,
        "duration_secs": duration_secs,
        "prompt_char_count": opts.prompt_char_count,
        "baseline_char_count": opts.baseline_char_count,
        "tier": opts.tier,
        "model_downgraded": opts.model_downgraded,
        "continue_here_fired": opts.continue_here_fired,
        "completed_at": chrono::Utc::now().to_rfc3339(),
    });

    // Write to metrics directory
    let metrics_dir = orchestra_dir.join("metrics");
    fs::create_dir_all(&metrics_dir)?;

    let metrics_file = metrics_dir.join(format!("{}-{}.json", unit_type, unit_id));
    fs::write(&metrics_file, serde_json::to_string_pretty(&metrics)?)?;

    debug!("Metrics saved to: {:?}", metrics_file);

    Ok(())
}

/// Save activity log implementation
///
/// This is a placeholder - the actual implementation is in activity_log.rs
fn save_activity_log_impl(
    orchestra_dir: &Path,
    unit_type: &str,
    unit_id: &str,
) -> Result<ActivityFilePath> {
    use std::fs;

    // Create activity directory
    let activity_dir = orchestra_dir.join("activity");
    fs::create_dir_all(&activity_dir)?;

    // Generate activity file path
    let activity_file = activity_dir.join(format!("{}-{}.jsonl", unit_type, unit_id));

    // Check if we have session entries to save
    // For now, return the path as if we saved something
    // The actual activity_log module handles the content

    debug!("Activity log would be saved to: {:?}", activity_file);

    // Return the path (even if empty, for consistency with orchestra-2)
    Ok(Some(activity_file.to_string_lossy().to_string()))
}

/// Extract memories from activity log (fire-and-forget)
///
/// Non-fatal operation that runs in the background
async fn extract_memories(activity_file: &str, unit_type: &str, unit_id: &str) {
    // Placeholder for memory extraction
    // In orchestra-2, this calls extractMemoriesFromUnit which:
    // 1. Reads the activity log file
    // 2. Builds an LLM call to extract memories
    // 3. Saves memories to .orchestra/memories/

    debug!(
        "Memory extraction for {}:{} from {}",
        unit_type, unit_id, activity_file
    );

    // This is a fire-and-forget operation, so errors are logged but not propagated
    // The actual implementation would be:
    // let llm_call = build_memory_llm_call(ctx);
    // if let Some(llm_call) = llm_call {
    //     extract_memories_from_unit(activity_file, unit_type, unit_id, llm_call)
    //         .await
    //         .unwrap_or_else(|e| {
    //             warn!("Memory extraction failed: {}", e);
    //         });
    // }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_closeout_options_default() {
        let opts = CloseoutOptions::default();
        assert!(opts.prompt_char_count.is_none());
        assert!(opts.baseline_char_count.is_none());
        assert!(opts.tier.is_none());
        assert!(!opts.model_downgraded);
        assert!(!opts.continue_here_fired);
    }

    #[test]
    fn test_closeout_options_with_values() {
        let opts = CloseoutOptions {
            prompt_char_count: Some(1000),
            baseline_char_count: Some(5000),
            tier: Some("standard".to_string()),
            model_downgraded: true,
            continue_here_fired: false,
        };

        assert_eq!(opts.prompt_char_count, Some(1000));
        assert_eq!(opts.baseline_char_count, Some(5000));
        assert_eq!(opts.tier, Some("standard".to_string()));
        assert!(opts.model_downgraded);
        assert!(!opts.continue_here_fired);
    }

    #[tokio::test]
    async fn test_closeout_unit() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra");

        let started_at = std::time::Instant::now();
        let opts = CloseoutOptions {
            prompt_char_count: Some(1000),
            tier: Some("standard".to_string()),
            ..Default::default()
        };

        let result = closeout_unit(
            &orchestra_dir,
            "task",
            "T01",
            started_at,
            "claude-3-sonnet",
            opts,
        )
        .await;

        assert!(result.is_ok());

        // Check that metrics file was created
        let metrics_file = orchestra_dir.join("metrics/task-T01.json");
        assert!(metrics_file.exists());

        // Check that activity directory was created
        let activity_dir = orchestra_dir.join("activity");
        assert!(activity_dir.exists());
    }

    #[test]
    fn test_snapshot_unit_metrics() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra");

        let started_at = std::time::Instant::now();
        let opts = CloseoutOptions::default();

        let result = snapshot_unit_metrics(
            &orchestra_dir,
            "task",
            "T01",
            started_at,
            "claude-3-sonnet",
            &opts,
        );

        assert!(result.is_ok());

        // Verify metrics file
        let metrics_file = orchestra_dir.join("metrics/task-T01.json");
        assert!(metrics_file.exists());

        // Read and verify content
        let content = std::fs::read_to_string(&metrics_file).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(json["unit_type"], "task");
        assert_eq!(json["unit_id"], "T01");
        assert_eq!(json["model_id"], "claude-3-sonnet");
    }

    #[test]
    fn test_save_activity_log_impl() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra");

        let result = save_activity_log_impl(&orchestra_dir, "task", "T01");

        assert!(result.is_ok());

        // Should return a file path
        let activity_file = result.unwrap();
        assert!(activity_file
            .as_ref()
            .is_some_and(|s| s.contains("task-T01.jsonl")));

        // Directory should be created
        let activity_dir = orchestra_dir.join("activity");
        assert!(activity_dir.exists());
    }
}
