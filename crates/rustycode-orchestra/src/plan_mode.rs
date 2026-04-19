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
            current_role: AgentRole::Worker,
            current_plan: None,
        }
    }

    /// Check if an agent with a given role can use a tool
    pub fn can_use_tool(&self, role: AgentRole, tool: &str) -> Result<(), ToolBlockedReason> {
        if !self.config.enabled {
            return Ok(());
        }

        let allowed = self.role_tool_matrix
            .get(&role)
            .ok_or(ToolBlockedReason::UnknownRole(role))?;

        if allowed.contains(tool) {
            Ok(())
        } else {
            Err(ToolBlockedReason::NotAllowedForRole {
                tool: tool.to_string(),
                role,
            })
        }
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
            .as_ref()
            .ok_or(PlanModeError::NoPlanToApprove)?;

        let token = ApprovalToken {
            plan_id: plan.id.clone(),
            approved_at: chrono::Utc::now(),
        };

        self.current_role = AgentRole::Worker;
        Ok(token)
    }

    /// Reset plan mode back to planning phase.
    pub fn reset(&mut self) {
        self.current_role = AgentRole::Planner;
        self.current_plan = None;
    }

    /// Check if a tool is allowed for the current role.
    pub fn is_tool_allowed(&self, tool_name: &str) -> Result<(), ToolBlockedReason> {
        self.can_use_tool(self.current_role, tool_name)
    }
}

/// Approval token returned when a plan is approved
#[derive(Debug, Clone)]
pub struct ApprovalToken {
    plan_id: String,
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
        assert!(pm.can_use_tool(AgentRole::Planner, "read").is_ok());
        assert!(pm.can_use_tool(AgentRole::Planner, "write").is_ok());
    }

    #[test]
    fn can_use_tool_reviewer_cannot_write() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Reviewer, "write").is_err());
    }

    #[test]
    fn can_use_tool_worker_can_write() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Worker, "write").is_ok());
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
            approval: PlanApproval::default(),
            created_at: chrono::Utc::now(),
        };
        assert!(!pm.assess_approval_required(&plan));
    }

    #[test]
    fn set_role_changes_phase() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        assert_eq!(pm.current_role(), AgentRole::Worker);
        pm.set_role(AgentRole::Planner);
        assert_eq!(pm.current_role(), AgentRole::Planner);
    }
}
