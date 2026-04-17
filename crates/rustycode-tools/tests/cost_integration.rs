//! Integration tests for end-to-end cost tracking through the tool execution pipeline.
//!
//! Verifies:
//! - API calls are recorded to CostTracker
//! - Costs persist to SQLite via Storage::save_api_call
//! - Budget enforcement blocks overspend
//! - Cost accumulation is accurate
//! - Session cost summary is queryable

use rustycode_storage::{ApiCallRecord, Storage};
use std::sync::Arc;

fn test_db(prefix: &str) -> Arc<Storage> {
    let db_path = std::env::temp_dir().join(format!("{}-{}.db", prefix, std::process::id()));
    let _ = std::fs::remove_file(&db_path);
    let storage = Storage::open(&db_path).expect("failed to open test db");
    Arc::new(storage)
}

fn create_test_session(storage: &Storage) -> String {
    let session = rustycode_protocol::Session::builder()
        .task("test cost")
        .build();
    let id = session.id.to_string();
    storage.insert_session(&session).expect("insert session");
    id
}

#[test]
fn test_save_and_list_api_calls() {
    let storage = test_db("cost-save");
    let session_id = create_test_session(&storage);

    let rec = ApiCallRecord {
        id: 0,
        session_id: session_id.clone(),
        model: "claude-sonnet-4-6".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.015,
        tool_name: Some("edit_file".to_string()),
        provider: Some("anthropic".to_string()),
        called_at: chrono::Utc::now().to_rfc3339(),
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cache_savings_usd: 0.0,
    };

    storage.save_api_call(&rec).expect("save api call");

    let calls = storage.list_api_calls(&session_id).expect("list api calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].model, "claude-sonnet-4-6");
    assert_eq!(calls[0].input_tokens, 1000);
    assert_eq!(calls[0].output_tokens, 500);
    assert!((calls[0].cost_usd - 0.015).abs() < f64::EPSILON);
    assert_eq!(calls[0].tool_name.as_deref(), Some("edit_file"));
}

#[test]
fn test_cost_accumulation() {
    let storage = test_db("cost-accum");
    let session_id = create_test_session(&storage);

    // Record 3 API calls
    for i in 0..3 {
        let rec = ApiCallRecord {
            id: 0,
            session_id: session_id.clone(),
            model: "claude-sonnet-4-6".to_string(),
            input_tokens: 500 * (i + 1),
            output_tokens: 200 * (i + 1),
            cost_usd: 0.01 * (i as f64 + 1.0),
            tool_name: Some(format!("tool_{}", i)),
            provider: None,
            called_at: chrono::Utc::now().to_rfc3339(),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_savings_usd: 0.0,
        };
        storage.save_api_call(&rec).expect("save api call");
    }

    let calls = storage.list_api_calls(&session_id).expect("list");
    assert_eq!(calls.len(), 3);

    let total_cost: f64 = calls.iter().map(|c| c.cost_usd).sum();
    assert!(
        (total_cost - 0.06).abs() < 1e-10,
        "total cost should be 0.06, got {}",
        total_cost
    );

    let total_input: i64 = calls.iter().map(|c| c.input_tokens).sum();
    assert_eq!(total_input, 3000);
}

#[test]
fn test_session_cost_query() {
    let storage = test_db("cost-query");
    let session_id = create_test_session(&storage);

    // Record calls
    for i in 0..5 {
        let rec = ApiCallRecord {
            id: 0,
            session_id: session_id.clone(),
            model: if i % 2 == 0 {
                "claude-sonnet-4-6"
            } else {
                "claude-opus-4-6"
            }
            .to_string(),
            input_tokens: 1000,
            output_tokens: 500,
            cost_usd: 0.02,
            tool_name: Some("edit_file".to_string()),
            provider: Some("anthropic".to_string()),
            called_at: chrono::Utc::now().to_rfc3339(),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_savings_usd: 0.0,
        };
        storage.save_api_call(&rec).expect("save");
    }

    // Query total cost
    let total = storage.session_cost(&session_id).expect("query cost");
    assert!(
        (total - 0.10).abs() < 1e-10,
        "total should be 0.10, got {}",
        total
    );
}

#[test]
fn test_session_isolation() {
    let storage = test_db("cost-iso");
    let s1 = create_test_session(&storage);
    let s2 = create_test_session(&storage);

    // Record for session 1
    let rec1 = ApiCallRecord {
        id: 0,
        session_id: s1.clone(),
        model: "claude-sonnet-4-6".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.05,
        tool_name: None,
        provider: None,
        called_at: chrono::Utc::now().to_rfc3339(),
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cache_savings_usd: 0.0,
    };
    storage.save_api_call(&rec1).expect("save s1");

    // Record for session 2
    let rec2 = ApiCallRecord {
        id: 0,
        session_id: s2.clone(),
        model: "claude-opus-4-6".to_string(),
        input_tokens: 2000,
        output_tokens: 1000,
        cost_usd: 0.10,
        tool_name: None,
        provider: None,
        called_at: chrono::Utc::now().to_rfc3339(),
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cache_savings_usd: 0.0,
    };
    storage.save_api_call(&rec2).expect("save s2");

    // Verify isolation
    let s1_cost = storage.session_cost(&s1).expect("cost s1");
    let s2_cost = storage.session_cost(&s2).expect("cost s2");
    assert!((s1_cost - 0.05).abs() < 1e-10);
    assert!((s2_cost - 0.10).abs() < 1e-10);

    let s1_calls = storage.list_api_calls(&s1).expect("calls s1");
    assert_eq!(s1_calls.len(), 1);
    assert_eq!(s1_calls[0].model, "claude-sonnet-4-6");
}

#[test]
fn test_cost_tracker_in_memory() {
    use rustycode_llm::cost_tracker::CostTracker;

    let mut tracker = CostTracker::new(None);

    tracker
        .record_call(rustycode_llm::cost_tracker::ApiCall {
            model: "claude-sonnet-4-6".to_string(),
            input_tokens: 1000,
            output_tokens: 500,
            cost_usd: 0.015,
            timestamp: chrono::Utc::now(),
            tool_name: Some("edit_file".to_string()),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_savings_usd: 0.0,
        })
        .expect("should be under budget");

    let summary = tracker.session_summary();
    assert_eq!(summary.calls_count, 1);
    assert!((summary.total_cost - 0.015).abs() < 1e-10);
    assert_eq!(summary.total_input_tokens, 1000);
    assert_eq!(summary.total_output_tokens, 500);
}

#[test]
fn test_budget_enforcement() {
    use rustycode_llm::cost_tracker::CostTracker;

    let mut tracker = CostTracker::new(Some(0.05)); // $0.05 budget

    // Under budget
    let under = tracker.record_call(rustycode_llm::cost_tracker::ApiCall {
        model: "claude-sonnet-4-6".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.03,
        timestamp: chrono::Utc::now(),
        tool_name: None,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cache_savings_usd: 0.0,
    });
    assert!(under.is_ok(), "should be under budget");

    // Over budget
    let over = tracker.record_call(rustycode_llm::cost_tracker::ApiCall {
        model: "claude-sonnet-4-6".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.03,
        timestamp: chrono::Utc::now(),
        tool_name: None,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cache_savings_usd: 0.0,
    });
    assert!(over.is_err(), "should exceed budget");

    let summary = tracker.session_summary();
    assert!((summary.total_cost - 0.06).abs() < 1e-10);

    let budget = tracker.check_budget();
    assert!(budget.is_exceeded);
    assert!(budget.percent_used > 100.0);
}
