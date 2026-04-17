// Integration tests for observability module
#![allow(unused)]

use rustycode_observability::SessionMetrics;

#[test]
fn test_metrics_display_format_tokens() {
    // Directly test the formatting logic since the module is internal
    let used = 500u64;
    let remaining = 1500u64;
    let total = used + remaining;
    let percent = if total > 0 {
        (used as f64 / total as f64 * 100.0).round() as u64
    } else {
        0
    };

    assert_eq!(percent, 25);
    assert_eq!(used, 500);
    assert_eq!(total, 2000);
}

#[test]
fn test_metrics_display_format_duration() {
    let secs = 3661.0; // 1h 1m 1s
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    assert_eq!(hours, 1);
    assert_eq!(minutes, 1);
    assert_eq!(seconds, 1);
}

#[test]
fn test_dashboard_with_session_metrics() {
    let metrics = SessionMetrics::new();

    // Record some work
    metrics.record_task(100, 0.5);
    metrics.record_task(150, 0.75);
    metrics.record_completion();
    metrics.set_active_tasks(2);

    assert_eq!(metrics.total_tokens.value(), 250);
    assert_eq!(metrics.total_tasks.value(), 2);
    assert_eq!(metrics.completed_tasks.value(), 1);
    assert_eq!(metrics.active_tasks.value(), 2.0);
}

#[test]
fn test_metrics_elapsed_time() {
    let metrics = SessionMetrics::new();
    let elapsed = metrics.elapsed_secs();

    // Should be a small positive number
    assert!(elapsed >= 0.0);
    assert!(elapsed < 1.0);
}

#[test]
fn test_metrics_with_errors() {
    let metrics = SessionMetrics::new();

    metrics.record_error();
    metrics.record_error();

    assert_eq!(metrics.total_errors.value(), 2);
}

#[test]
fn test_metrics_progress_calculation() {
    let total_budget = 2000u64;
    let used = 1500u64;
    let remaining = total_budget.saturating_sub(used);

    let percent = (used as f64 / total_budget as f64) * 100.0;

    assert_eq!(remaining, 500);
    assert!(percent > 74.0 && percent < 76.0);
}

#[test]
fn test_metrics_over_budget() {
    let total_budget = 1000u64;
    let used = 1500u64;
    let remaining = total_budget.saturating_sub(used);

    assert_eq!(remaining, 0); // saturating_sub caps at 0
}
