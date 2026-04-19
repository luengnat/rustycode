use rustycode_orchestra::plan_mode::{PlanMode, PlanModeConfig};
use rustycode_protocol::{AgentRole, ToolCall};
use rustycode_tools::{ConvoyDispatcher, ToolGate};
use std::sync::Arc;

#[tokio::test]
async fn test_plan_mode_role_access_integration() {
    let config = PlanModeConfig {
        enabled: true,
        require_approval: true,
        cost_threshold: 1.0,
    };
    let plan_mode = Arc::new(PlanMode::new(config));
    let dispatcher = ConvoyDispatcher::new(plan_mode.clone());

    // Researcher should be able to read but NOT write
    let read_call = ToolCall {
        call_id: "read-1".into(),
        name: "read".into(),
        arguments: serde_json::json!({}),
    };
    let write_call = ToolCall {
        call_id: "write-1".into(),
        name: "write".into(),
        arguments: serde_json::json!({}),
    };

    assert!(dispatcher.check_allowed(AgentRole::Researcher, "read").is_ok());
    assert!(dispatcher.check_allowed(AgentRole::Researcher, "write").is_err());

    // Worker should be able to write
    assert!(dispatcher.check_allowed(AgentRole::Worker, "write").is_ok());
    
    // Planner should be able to write (plan-only tools usually, but matrix says 'write' is allowed for Planner)
    assert!(dispatcher.check_allowed(AgentRole::Planner, "write").is_ok());
}

#[test]
fn test_plan_mode_approval_logic() {
    use rustycode_protocol::{ConvoyPlan, ConvoyRisk, PlanApproval, RiskLevel};
    use chrono::Utc;

    let config = PlanModeConfig {
        enabled: true,
        require_approval: true,
        cost_threshold: 1.0,
    };
    let pm = PlanMode::new(config);

    let high_risk_plan = ConvoyPlan {
        id: "high-risk".into(),
        summary: "test".into(),
        approach: "test".into(),
        files_to_modify: vec![],
        commands_to_run: vec![],
        risks: vec![ConvoyRisk {
            level: RiskLevel::High,
            description: "dangerous".into(),
            mitigation: "none".into(),
        }],
        estimated_cost_usd: 0.0,
        success_criteria: vec![],
        approval: PlanApproval::default(),
        created_at: Utc::now(),
    };

    assert!(pm.assess_approval_required(&high_risk_plan));

    let low_risk_plan = ConvoyPlan {
        id: "low-risk".into(),
        summary: "test".into(),
        approach: "test".into(),
        files_to_modify: vec![],
        commands_to_run: vec![],
        risks: vec![ConvoyRisk {
            level: RiskLevel::Low,
            description: "safe".into(),
            mitigation: "none".into(),
        }],
        estimated_cost_usd: 0.0,
        success_criteria: vec![],
        approval: PlanApproval::default(),
        created_at: Utc::now(),
    };

    assert!(!pm.assess_approval_required(&low_risk_plan));
}
