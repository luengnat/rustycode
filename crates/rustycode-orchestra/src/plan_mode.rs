//! Plan Mode — Role-based tool access for plan execution
//!
//! Uses role-based tool access instead of global ExecutionPhase states.
//! Each agent role (Planner, Worker, Reviewer, Researcher) has specific tools it can use.
//! Plans can require approval based on risk level and estimated cost.

use crate::agent_identity::AgentRole;
use crate::tool_access_matrix::build_access_matrix;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Plan mode configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanModeConfig {
    pub enabled: bool,
    pub require_approval: bool,
    /// Tools allowed during planning (read-only)
    pub allowed_tools_planning: Vec<String>,
    /// Tools allowed during implementation
    pub allowed_tools_implementation: Vec<String>,
}

impl Default for PlanModeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: true,
            allowed_tools_planning: vec![
                "read".to_string(),
                "read_file".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
                "edit_file".to_string(), // Dry-run only in planning phase
            ],
            allowed_tools_implementation: vec![
                "read".to_string(),
                "read_file".to_string(),
                "edit_file".to_string(),
                "write_file".to_string(),
                "write".to_string(),
                "apply_patch".to_string(),
                "search_replace".to_string(),
                "bash".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
            ],
        }
    }
}

/// Structured plan produced by the agent during planning phase
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub summary: String,
    pub approach: String,
    pub files_to_modify: Vec<FilePlan>,
    pub commands_to_run: Vec<CommandPlan>,
    pub estimated_tokens: TokenEstimate,
    pub estimated_cost_usd: f64,
    pub risks: Vec<Risk>,
    pub success_criteria: Vec<String>,
    pub created_at: String,
}

/// Planned file modification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilePlan {
    pub path: String,
    pub action: FileAction,
    pub reason: String,
}

/// File action type
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Create,
    Modify,
    Delete,
}

impl std::fmt::Display for FileAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Modify => write!(f, "modify"),
            Self::Delete => write!(f, "delete"),
        }
    }
}

/// Planned command execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandPlan {
    pub command: String,
    pub reason: String,
}

/// Token estimate for plan execution
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TokenEstimate {
    pub input: usize,
    pub output: usize,
}

/// Risk identified during planning
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Risk {
    pub level: RiskLevel,
    pub description: String,
}

/// Risk severity level
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

/// Reason a tool was blocked
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolBlockedReason {
    NotAllowedForRole { tool: String, role: AgentRole },
    RequiresApproval,
    ConvoyPlanNotApproved,
    UnknownRole(AgentRole),
}

impl std::fmt::Display for ToolBlockedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAllowedForRole { tool, role } => {
                write!(f, "Tool '{}' not allowed for {:?} role", tool, role)
            }
            Self::RequiresApproval => {
                write!(f, "Plan execution requires approval")
            }
            Self::ConvoyPlanNotApproved => {
                write!(f, "Convoy plan has not been approved")
            }
            Self::UnknownRole(role) => {
                write!(f, "Unknown role: {:?}", role)
            }
        }
    }
}

impl std::error::Error for ToolBlockedReason {}

/// Opaque approval token — granted after user approves a plan
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ApprovalToken {
    pub plan_id: String,
}

/// Plan mode manager — gates execution behind role-based access and approval
#[derive(Clone)]
pub struct PlanMode {
    config: PlanModeConfig,
    role_tool_matrix: HashMap<AgentRole, HashSet<&'static str>>,
}

impl Default for PlanMode {
    fn default() -> Self {
        Self::new(PlanModeConfig::default())
    }
}

impl PlanMode {
    /// Create a new plan mode manager
    pub fn new(config: PlanModeConfig) -> Self {
        Self {
            config,
            role_tool_matrix: build_access_matrix(),
        }
    }

    /// Check if a role can use a specific tool
    pub fn can_use_tool(&self, role: AgentRole, tool: &str) -> Result<(), ToolBlockedReason> {
        // If plan mode is disabled, allow all tools
        if !self.config.enabled {
            return Ok(());
        }

        // Look up role in matrix
        match self.role_tool_matrix.get(&role) {
            Some(allowed_tools) => {
                if allowed_tools.contains(tool) {
                    Ok(())
                } else {
                    Err(ToolBlockedReason::NotAllowedForRole {
                        tool: tool.to_string(),
                        role,
                    })
                }
            }
            None => Err(ToolBlockedReason::UnknownRole(role)),
        }
    }

    /// Assess whether a plan requires approval.
    ///
    /// Returns true if:
    /// - Plan has any High risk
    /// - Estimated cost > $1.00
    ///
    /// Otherwise returns false.
    pub fn assess_approval_required(&self, plan: &Plan) -> bool {
        // Check for high risks
        if plan.risks.iter().any(|r| r.level >= RiskLevel::High) {
            return true;
        }

        // Check for high cost
        if plan.estimated_cost_usd > 1.00 {
            return true;
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
}

/// Plan mode errors
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanModeError {
    NoPlanToApprove,
    PlanNotFound(String),
}

impl std::fmt::Display for PlanModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPlanToApprove => write!(f, "No plan submitted for approval"),
            Self::PlanNotFound(id) => write!(f, "Plan not found: {}", id),
        }
    }
}

impl std::error::Error for PlanModeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_planning_tools() {
        let config = PlanModeConfig::default();
        assert!(config.enabled);
        assert!(config.require_approval);
        assert!(config.allowed_tools_planning.contains(&"read".to_string()));
        assert!(config.allowed_tools_planning.contains(&"grep".to_string()));
        assert!(config
            .allowed_tools_planning
            .contains(&"edit_file".to_string()));
    }

    #[test]
    fn default_config_has_implementation_tools() {
        let config = PlanModeConfig::default();
        assert!(config
            .allowed_tools_implementation
            .contains(&"write".to_string()));
        assert!(config
            .allowed_tools_implementation
            .contains(&"bash".to_string()));
    }

    #[test]
    fn can_use_tool_planner_read() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Planner, "read").is_ok());
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
    fn can_use_tool_disabled_mode_allows_all() {
        let config = PlanModeConfig {
            enabled: false,
            ..PlanModeConfig::default()
        };
        let pm = PlanMode::new(config);
        // When disabled, any tool is allowed
        assert!(pm.can_use_tool(AgentRole::Reviewer, "write").is_ok());
    }

    #[test]
    fn assess_approval_required_high_risk() {
        let pm = PlanMode::new(PlanModeConfig::default());
        let plan = Plan {
            id: "plan-1".to_string(),
            summary: "Test".to_string(),
            approach: "".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            estimated_tokens: TokenEstimate::default(),
            estimated_cost_usd: 0.50,
            risks: vec![Risk {
                level: RiskLevel::High,
                description: "High risk change".to_string(),
            }],
            success_criteria: vec![],
            created_at: "2026-04-14".to_string(),
        };
        assert!(pm.assess_approval_required(&plan));
    }

    #[test]
    fn assess_approval_required_high_cost() {
        let pm = PlanMode::new(PlanModeConfig::default());
        let plan = Plan {
            id: "plan-2".to_string(),
            summary: "Test".to_string(),
            approach: "".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            estimated_tokens: TokenEstimate::default(),
            estimated_cost_usd: 1.50,
            risks: vec![Risk {
                level: RiskLevel::Low,
                description: "Low risk".to_string(),
            }],
            success_criteria: vec![],
            created_at: "2026-04-14".to_string(),
        };
        assert!(pm.assess_approval_required(&plan));
    }

    #[test]
    fn assess_approval_required_low_risk_low_cost() {
        let pm = PlanMode::new(PlanModeConfig::default());
        let plan = Plan {
            id: "plan-3".to_string(),
            summary: "Test".to_string(),
            approach: "".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            estimated_tokens: TokenEstimate::default(),
            estimated_cost_usd: 0.05,
            risks: vec![Risk {
                level: RiskLevel::Low,
                description: "Low risk".to_string(),
            }],
            success_criteria: vec![],
            created_at: "2026-04-14".to_string(),
        };
        assert!(!pm.assess_approval_required(&plan));
    }

    #[test]
    fn file_action_display() {
        assert_eq!(FileAction::Create.to_string(), "create");
        assert_eq!(FileAction::Modify.to_string(), "modify");
        assert_eq!(FileAction::Delete.to_string(), "delete");
    }

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
    }

    #[test]
    fn plan_serialization() {
        let plan = Plan {
            id: "plan-serde".to_string(),
            summary: "Test".to_string(),
            approach: "Approach".to_string(),
            files_to_modify: vec![FilePlan {
                path: "src/main.rs".to_string(),
                action: FileAction::Modify,
                reason: "Add error handling".to_string(),
            }],
            commands_to_run: vec![CommandPlan {
                command: "cargo test".to_string(),
                reason: "Verify changes".to_string(),
            }],
            estimated_tokens: TokenEstimate {
                input: 500,
                output: 1000,
            },
            estimated_cost_usd: 0.04,
            risks: vec![Risk {
                level: RiskLevel::High,
                description: "Changes main entry point".to_string(),
            }],
            success_criteria: vec!["Tests pass".to_string()],
            created_at: "2026-04-14".to_string(),
        };

        let json = serde_json::to_string(&plan).unwrap();
        let parsed: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "plan-serde");
        assert_eq!(parsed.files_to_modify.len(), 1);
        assert_eq!(parsed.risks[0].level, RiskLevel::High);
    }

    #[test]
    fn plan_mode_error_display() {
        assert_eq!(
            PlanModeError::NoPlanToApprove.to_string(),
            "No plan submitted for approval"
        );
        assert_eq!(
            PlanModeError::PlanNotFound("x".to_string()).to_string(),
            "Plan not found: x"
        );
    }

    #[test]
    fn disabled_plan_mode() {
        let config = PlanModeConfig {
            enabled: false,
            ..PlanModeConfig::default()
        };
        let pm = PlanMode::new(config);
        assert!(!pm.is_enabled());
    }

    #[test]
    fn no_approval_required() {
        let config = PlanModeConfig {
            require_approval: false,
            ..PlanModeConfig::default()
        };
        let pm = PlanMode::new(config);
        assert!(!pm.requires_approval());
    }
}
