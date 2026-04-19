use rustycode_orchestra::plan_mode::{PlanMode, PlanModeConfig};
use rustycode_protocol::{
    AgentRole, ConvoyPlan, ConvoyRisk, PlanApproval, RiskLevel, permission_role::ToolBlockedReason,
};
use chrono::Utc;

#[test]
fn test_planner_research_allowed_without_approval() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    pm.set_role(AgentRole::Planner);

    // Planner should be able to run bash even if there's no plan or an unapproved high-risk plan
    assert!(pm.can_use_tool(AgentRole::Planner, "bash").is_ok());

    let plan = create_high_risk_plan();
    pm.submit_plan(plan);

    // Still allowed as Planner (research context)
    assert!(pm.can_use_tool(AgentRole::Planner, "bash").is_ok());
}

#[test]
fn test_worker_blocked_on_high_risk_unapproved_plan() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    let plan = create_high_risk_plan();
    pm.submit_plan(plan);

    // Switch to Worker
    pm.set_role(AgentRole::Worker);

    // Non-sensitive tool allowed
    assert!(pm.can_use_tool(AgentRole::Worker, "read_file").is_ok());

    // Sensitive tool BLOCKED
    let result = pm.can_use_tool(AgentRole::Worker, "bash");
    assert!(matches!(result, Err(ToolBlockedReason::ConvoyPlanNotApproved)));
}

#[test]
fn test_worker_allowed_after_approval() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    let plan = create_high_risk_plan();
    pm.submit_plan(plan);
    pm.set_role(AgentRole::Worker);

    // Blocked initially
    assert!(pm.can_use_tool(AgentRole::Worker, "bash").is_err());

    // Approve
    pm.approve().expect("Should approve");

    // Now allowed
    assert!(pm.can_use_tool(AgentRole::Worker, "bash").is_ok());
}

#[test]
fn test_worker_allowed_on_low_risk_without_approval() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    let plan = create_low_risk_plan();
    pm.submit_plan(plan);
    pm.set_role(AgentRole::Worker);

    // Low risk plan does not require approval by default config (threshold is High)
    assert!(pm.can_use_tool(AgentRole::Worker, "bash").is_ok());
}

#[test]
fn test_reviewer_role_strictly_gated() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    pm.set_role(AgentRole::Reviewer);

    // Allowed to read
    assert!(pm.can_use_tool(AgentRole::Reviewer, "read_file").is_ok());

    // NOT allowed to write (even if plan is approved/low risk, it's a role boundary)
    assert!(pm.can_use_tool(AgentRole::Reviewer, "write_file").is_err());
}

#[test]
fn test_skeptic_role_strictly_gated() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    pm.set_role(AgentRole::Skeptic);

    assert!(pm.can_use_tool(AgentRole::Skeptic, "read_file").is_ok());
    assert!(pm.can_use_tool(AgentRole::Skeptic, "bash").is_err());
    assert!(pm.can_use_tool(AgentRole::Skeptic, "write_file").is_err());
}

#[test]
fn test_judge_role_access() {
    let mut pm = PlanMode::new(PlanModeConfig::default());
    pm.set_role(AgentRole::Judge);

    // Judge can verify (bash) but not write code
    assert!(pm.can_use_tool(AgentRole::Judge, "read_file").is_ok());
    assert!(pm.can_use_tool(AgentRole::Judge, "bash").is_ok());
    assert!(pm.can_use_tool(AgentRole::Judge, "write_file").is_err());
}

#[test]
fn test_task_tool_gated_correctly() {
    let pm = PlanMode::new(PlanModeConfig::default());

    // Roles that should have "task" (sub-agent spawning)
    assert!(pm.can_use_tool(AgentRole::Planner, "task").is_ok());
    assert!(pm.can_use_tool(AgentRole::Architect, "task").is_ok());
    assert!(pm.can_use_tool(AgentRole::Researcher, "task").is_ok());
    assert!(pm.can_use_tool(AgentRole::Coordinator, "task").is_ok());

    // Roles that should NOT have "task"
    assert!(pm.can_use_tool(AgentRole::Worker, "task").is_err());
    assert!(pm.can_use_tool(AgentRole::Builder, "task").is_err());
    assert!(pm.can_use_tool(AgentRole::Skeptic, "task").is_err());
    assert!(pm.can_use_tool(AgentRole::Judge, "task").is_err());
}

#[test]
fn test_dead_aliases_rejected() {
    let pm = PlanMode::new(PlanModeConfig::default());

    // These old aliases should NOT be accepted by the access matrix
    let dead_aliases = ["read", "write", "Agent", "lsp"];
    for alias in &dead_aliases {
        // Even Planner (most permissive) should reject dead aliases
        assert!(
            pm.can_use_tool(AgentRole::Planner, alias).is_err(),
            "Dead alias '{}' should not be in access matrix",
            alias
        );
    }
}

// Helpers

fn create_high_risk_plan() -> ConvoyPlan {
    ConvoyPlan {
        id: "high-risk-1".into(),
        summary: "Delete everything".into(),
        approach: "rm -rf /".into(),
        files_to_modify: vec![],
        commands_to_run: vec![],
        risks: vec![ConvoyRisk {
            level: RiskLevel::High,
            description: "Very dangerous".into(),
            mitigation: "None".into(),
        }],
        estimated_cost_usd: 0.0,
        success_criteria: vec![],
        approval: PlanApproval::default(),
        created_at: Utc::now(),
    }
}

fn create_low_risk_plan() -> ConvoyPlan {
    ConvoyPlan {
        id: "low-risk-1".into(),
        summary: "Read README".into(),
        approach: "cat README.md".into(),
        files_to_modify: vec![],
        commands_to_run: vec![],
        risks: vec![ConvoyRisk {
            level: RiskLevel::Low,
            description: "Safe".into(),
            mitigation: "N/A".into(),
        }],
        estimated_cost_usd: 0.0,
        success_criteria: vec![],
        approval: PlanApproval::default(),
        created_at: Utc::now(),
    }
}
