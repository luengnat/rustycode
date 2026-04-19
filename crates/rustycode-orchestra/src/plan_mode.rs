//! Plan Mode — Role-based tool access for plan execution
//!
//! Uses role-based tool access instead of global ExecutionPhase states.
//! Each agent role (Planner, Worker, Reviewer, Researcher) has specific tools it can use.
//! Plans can require approval based on risk level and estimated cost.

use crate::tool_access_matrix::build_access_matrix;
use rustycode_protocol::{
    AgentRole, ConvoyPlan, permission_role::ToolBlockedReason,
    RiskLevel,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Plan mode configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanModeConfig {
    pub enabled: bool,
    pub require_approval: bool,
    pub cost_threshold: f64,
}

impl Default for PlanModeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: true,
            cost_threshold: 1.00, // $1.00 default threshold
        }
    }
}

/// Criteria that trigger a requirement for manual approval
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ApprovalTrigger {
    /// Plan has risks at or above this level
    HighRisk(RiskLevel),
    /// Estimated cost exceeds this threshold
    HighCost { estimated_usd: f64, threshold: f64 },
    /// Plan modifies files outside the project root (not yet implemented)
    ExternalChanges { examples: Vec<String> },
}

/// Plan mode manager — gates execution behind role-based access and approval
#[derive(Clone, Debug)]
pub struct PlanMode {
    config: PlanModeConfig,
    role_tool_matrix: HashMap<AgentRole, HashSet<&'static str>>,
    approval_triggers: Vec<ApprovalTrigger>,
    current_role: AgentRole,
    current_plan: Option<ConvoyPlan>,
}

impl Default for PlanMode {
    fn default() -> Self {
        Self::new(PlanModeConfig::default())
    }
}

impl PlanMode {
    /// Create a new plan mode manager
    pub fn new(config: PlanModeConfig) -> Self {
        let approval_triggers = vec![
            ApprovalTrigger::HighRisk(RiskLevel::High),
            ApprovalTrigger::HighCost {
                estimated_usd: 0.0, // Placeholder
                threshold: config.cost_threshold,
            },
        ];

        Self {
            config,
            role_tool_matrix: build_access_matrix(),
            approval_triggers,
            current_role: AgentRole::Planner,
            current_plan: None,
        }
    }

    /// Check if an agent with a given role can use a tool
    pub fn can_use_tool(&self, role: AgentRole, tool: &str) -> Result<(), ToolBlockedReason> {
        if !self.config.enabled {
            return Ok(());
        }

        // 1. Check basic role-based access
        let allowed = self.role_tool_matrix
            .get(&role)
            .ok_or(ToolBlockedReason::UnknownRole(role))?;

        if !allowed.contains(tool) {
            return Err(ToolBlockedReason::NotAllowedForRole {
                tool: tool.to_string(),
                role,
            });
        }

        // 2. Enforcement: If sensitive tool and plan approval is required but missing, block implementation roles
        if self.is_sensitive_tool(tool) {
            if let Some(plan) = &self.current_plan {
                // If the plan requires approval and hasn't been approved yet
                if self.assess_approval_required(plan) && !plan.approval.approved {
                    // Block sensitive tools for implementation roles (Worker, Builder, etc.)
                    // Note: Planner is exempted so they can finish drafting the plan/artifacts.
                    if matches!(
                        role,
                        AgentRole::Worker | AgentRole::Builder | AgentRole::Architect | AgentRole::Scalpel
                    ) {
                        return Err(ToolBlockedReason::ConvoyPlanNotApproved);
                    }
                }
            }
        }

        Ok(())
    }

    /// Assess whether a plan requires approval based on risk and cost.
    pub fn assess_approval_required(&self, plan: &ConvoyPlan) -> bool {
        if !self.config.require_approval {
            return false;
        }

        for trigger in &self.approval_triggers {
            match trigger {
                ApprovalTrigger::HighRisk(level) => {
                    if plan.risks.iter().any(|r| r.level >= *level) {
                        return true;
                    }
                }
                ApprovalTrigger::HighCost { threshold: _, .. } => {
                    // For now, ConvoyPlan doesn't have cost, so we skip this check
                    // or use a placeholder if cost were available.
                    // if plan.estimated_cost_usd > *threshold { return true; }
                }
                ApprovalTrigger::ExternalChanges { .. } => {
                    // Not yet implemented
                }
            }
        }
        false
    }

    /// Get the config
    pub fn config(&self) -> &PlanModeConfig {
        &self.config
    }

    /// Check if plan mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if approval is required globally
    pub fn requires_approval(&self) -> bool {
        self.config.require_approval
    }

    /// Get the current role.
    pub fn current_role(&self) -> AgentRole {
        self.current_role
    }

    /// Set the active role.
    pub fn set_role(&mut self, role: AgentRole) {
        self.current_role = role;
    }

    /// Get the current plan (if any).
    pub fn current_plan(&self) -> Option<&ConvoyPlan> {
        self.current_plan.as_ref()
    }

    /// Submit a plan and switch to planner role for review.
    pub fn submit_plan(&mut self, plan: ConvoyPlan) {
        self.current_plan = Some(plan);
        self.current_role = AgentRole::Planner;
    }

    /// Get the current phase as a string (for TUI compatibility).
    /// Returns "planning" if the current role is Planner, "implementation" otherwise.
    pub fn current_phase(&self) -> &'static str {
        match self.current_role {
            AgentRole::Planner => "planning",
            _ => "implementation",
        }
    }

    /// Approve the current plan and switch to worker role.
    pub fn approve(&mut self) -> Result<ApprovalToken, PlanModeError> {
        let plan = self
            .current_plan
            .as_mut()
            .ok_or(PlanModeError::NoPlanToApprove)?;

        // Set approval state
        plan.approval.approved = true;
        plan.approval.approved_at = Some(chrono::Utc::now());
        plan.approval.approved_by = Some("User".to_string());

        let token = ApprovalToken {
            plan_id: plan.id.clone(),
            approved_at: plan.approval.approved_at.unwrap(),
        };

        self.current_role = AgentRole::Worker;
        Ok(token)
    }

    /// Reset plan mode back to planning phase.
    pub fn reset(&mut self) {
        self.current_role = AgentRole::Planner;
        self.current_plan = None;
    }

    /// Approve a specific plan by ID and switch to worker role.
    pub fn approve_plan(&mut self, plan_id: &str) -> Result<ApprovalToken, PlanModeError> {
        if let Some(plan) = &mut self.current_plan {
            if plan.id == plan_id {
                plan.approval.approved = true;
                plan.approval.approved_at = Some(chrono::Utc::now());
            }
        }

        self.current_role = AgentRole::Worker;
        Ok(ApprovalToken {
            plan_id: plan_id.to_string(),
            approved_at: chrono::Utc::now(),
        })
    }

    /// Reject the current plan and stay in planner role.
    pub fn reject(&mut self) {
        self.current_plan = None;
        self.current_role = AgentRole::Planner;
    }

    /// Check if a tool is allowed for the current role.
    pub fn is_tool_allowed(&self, tool_name: &str) -> Result<(), ToolBlockedReason> {
        self.can_use_tool(self.current_role, tool_name)
    }

    /// Helper to identify tools that require an approved plan for execution roles.
    pub fn is_sensitive_tool(&self, tool: &str) -> bool {
        matches!(
            tool,
            "bash" | "write_file" | "edit_file" | "multiedit" | "apply_patch" | "git_commit" | "text_editor_20250728"
        )
    }
}

/// Approval token returned when a plan is approved
#[derive(Debug, Clone)]
pub struct ApprovalToken {
    plan_id: String,
    #[allow(dead_code)]
    approved_at: chrono::DateTime<chrono::Utc>,
}

impl ApprovalToken {
    /// Get the plan ID
    pub fn plan_id(&self) -> &str {
        &self.plan_id
    }
}

/// Errors that can occur during plan mode operations
#[derive(Debug, thiserror::Error)]
pub enum PlanModeError {
    /// No plan has been submitted for approval
    #[error("no plan to approve")]
    NoPlanToApprove,
    /// Plan has already been approved
    #[error("plan already approved")]
    AlreadyApproved,
}

// We keep PlanModeProvider but simplify it if needed
pub trait PlanModeProvider: Send + Sync {
    fn can_use_tool(&self, role: AgentRole, tool: &str) -> Result<(), ToolBlockedReason>;
}

impl PlanModeProvider for PlanMode {
    fn can_use_tool(&self, role: AgentRole, tool: &str) -> Result<(), ToolBlockedReason> {
        self.can_use_tool(role, tool)
    }
}

impl rustycode_tools::ToolGate for PlanMode {
    fn check_access(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason> {
        self.can_use_tool(role, tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::{ConvoyRisk, PlanApproval};

    #[test]
    fn can_use_tool_planner_read() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Planner, "read_file").is_ok());
        assert!(pm.can_use_tool(AgentRole::Planner, "write_file").is_ok());
    }

    #[test]
    fn can_use_tool_reviewer_cannot_write() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Reviewer, "write_file").is_err());
    }

    #[test]
    fn can_use_tool_worker_can_write() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Worker, "write_file").is_ok());
    }

    #[test]
    fn assess_approval_required_high_risk() {
        let pm = PlanMode::new(PlanModeConfig::default());
        let plan = ConvoyPlan {
            id: "plan-1".to_string(),
            summary: "Test".to_string(),
            approach: "".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![ConvoyRisk {
                level: RiskLevel::High,
                description: "High risk change".to_string(),
                mitigation: String::new(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec![],
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        assert!(pm.assess_approval_required(&plan));
    }

    #[test]
    fn assess_approval_required_low_risk() {
        let pm = PlanMode::new(PlanModeConfig::default());
        let plan = ConvoyPlan {
            id: "plan-2".to_string(),
            summary: "Test".to_string(),
            approach: "".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![ConvoyRisk {
                level: RiskLevel::Low,
                description: "Low risk".to_string(),
                mitigation: String::new(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec![],
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        assert!(!pm.assess_approval_required(&plan));
    }

    #[test]
    fn set_role_changes_phase() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        assert_eq!(pm.current_role(), AgentRole::Planner);
        pm.set_role(AgentRole::Worker);
        assert_eq!(pm.current_role(), AgentRole::Worker);
    }

    #[test]
    fn is_sensitive_tool_identifies_mutating_tools() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.is_sensitive_tool("bash"));
        assert!(pm.is_sensitive_tool("write_file"));
        assert!(pm.is_sensitive_tool("edit_file"));
        assert!(pm.is_sensitive_tool("apply_patch"));
        assert!(pm.is_sensitive_tool("multiedit"));
        assert!(pm.is_sensitive_tool("git_commit"));
        assert!(!pm.is_sensitive_tool("read_file"));
        assert!(!pm.is_sensitive_tool("grep"));
        assert!(!pm.is_sensitive_tool("glob"));
    }

    #[test]
    fn unapproved_high_risk_plan_blocks_worker_bash() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let plan = ConvoyPlan {
            id: "plan-blocked".to_string(),
            summary: "Risky".to_string(),
            approach: String::new(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![ConvoyRisk {
                level: RiskLevel::High,
                description: "danger".to_string(),
                mitigation: String::new(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec![],
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        pm.submit_plan(plan);
        // Worker should be blocked on sensitive tools when plan is unapproved
        assert!(pm.can_use_tool(AgentRole::Worker, "bash").is_err());
        assert!(pm.can_use_tool(AgentRole::Worker, "write_file").is_err());
        // Planner should still be allowed
        assert!(pm.can_use_tool(AgentRole::Planner, "bash").is_ok());
        // Non-sensitive tools still allowed
        assert!(pm.can_use_tool(AgentRole::Worker, "read_file").is_ok());
    }

    #[test]
    fn approved_plan_allows_worker_sensitive_tools() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let plan = ConvoyPlan {
            id: "plan-ok".to_string(),
            summary: "Safe".to_string(),
            approach: String::new(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![ConvoyRisk {
                level: RiskLevel::Low,
                description: "safe".to_string(),
                mitigation: String::new(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec![],
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        pm.submit_plan(plan);
        pm.approve().unwrap();
        // Worker should now be allowed on sensitive tools
        assert!(pm.can_use_tool(AgentRole::Worker, "bash").is_ok());
        assert!(pm.can_use_tool(AgentRole::Worker, "write_file").is_ok());
    }

    #[test]
    fn disabled_plan_mode_allows_all_tools() {
        let config = PlanModeConfig {
            enabled: false,
            require_approval: false,
            cost_threshold: 0.0,
        };
        let pm = PlanMode::new(config);
        assert!(pm.can_use_tool(AgentRole::Researcher, "bash").is_ok());
        assert!(pm.can_use_tool(AgentRole::Researcher, "write_file").is_ok());
    }

    #[test]
    fn reject_clears_plan_and_stays_planner() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let plan = ConvoyPlan {
            id: "plan-reject".to_string(),
            summary: "Bad".to_string(),
            approach: String::new(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![],
            estimated_cost_usd: 0.0,
            success_criteria: vec![],
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        pm.submit_plan(plan);
        assert!(pm.current_plan().is_some());
        pm.reject();
        assert!(pm.current_plan().is_none());
        assert_eq!(pm.current_role(), AgentRole::Planner);
    }

    #[test]
    fn reset_clears_everything() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let plan = ConvoyPlan {
            id: "plan-reset".to_string(),
            summary: "Reset".to_string(),
            approach: String::new(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![],
            estimated_cost_usd: 0.0,
            success_criteria: vec![],
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        pm.submit_plan(plan);
        pm.approve().unwrap();
        assert_eq!(pm.current_role(), AgentRole::Worker);
        pm.reset();
        assert_eq!(pm.current_role(), AgentRole::Planner);
        assert!(pm.current_plan().is_none());
    }
}
