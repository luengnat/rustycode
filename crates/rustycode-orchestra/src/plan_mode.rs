//! Plan Mode — Execution gating with read-only planning phase
//!
//! Enforces a planning phase before implementation. The agent analyzes
//! the task using read-only tools, generates a structured plan with
//! risks and costs, and waits for user approval before executing changes.
//!
//! # Phases
//!
//! - **Planning**: Read-only analysis (read, grep, glob, lsp, web_search)
//! - **Implementation**: Full tool access after plan approval

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Execution phases for plan-first workflow
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPhase {
    Planning,
    Implementation,
}

impl std::fmt::Display for ExecutionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "planning"),
            Self::Implementation => write!(f, "implementation"),
        }
    }
}

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
                "edit_file".to_string(),
                "write".to_string(),
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

/// Reason a tool was blocked in the current phase
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolBlockedReason {
    NotAllowedInPhase { tool: String, phase: ExecutionPhase },
    RequiresApproval,
}

impl std::fmt::Display for ToolBlockedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAllowedInPhase { tool, phase } => {
                write!(f, "Tool '{}' not allowed in {} phase", tool, phase)
            }
            Self::RequiresApproval => {
                write!(f, "Implementation requires plan approval")
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

/// Plan mode manager — gates execution behind planning and approval
#[derive(Clone)]
pub struct PlanMode {
    config: PlanModeConfig,
    current_phase: ExecutionPhase,
    approved_plans: HashSet<String>,
    current_plan: Option<Plan>,
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
            current_phase: ExecutionPhase::Planning,
            approved_plans: HashSet::new(),
            current_plan: None,
        }
    }

    /// Check if a tool is allowed in the current execution phase
    pub fn is_tool_allowed(&self, tool: &str) -> Result<(), ToolBlockedReason> {
        let allowed = match self.current_phase {
            ExecutionPhase::Planning => &self.config.allowed_tools_planning,
            ExecutionPhase::Implementation => &self.config.allowed_tools_implementation,
        };

        if allowed.iter().any(|t| t == tool) {
            Ok(())
        } else {
            Err(ToolBlockedReason::NotAllowedInPhase {
                tool: tool.to_string(),
                phase: self.current_phase,
            })
        }
    }

    /// Check if edit_file should run in dry-run mode (planning phase)
    pub fn is_edit_dry_run(&self) -> bool {
        self.current_phase == ExecutionPhase::Planning
    }

    /// Get current execution phase
    pub fn current_phase(&self) -> ExecutionPhase {
        self.current_phase
    }

    /// Get the current plan (if any)
    pub fn current_plan(&self) -> Option<&Plan> {
        self.current_plan.as_ref()
    }

    /// Submit a plan for user approval
    pub fn submit_plan(&mut self, plan: Plan) {
        self.current_plan = Some(plan);
        self.current_phase = ExecutionPhase::Planning;
    }

    /// Approve the current plan and transition to implementation
    pub fn approve(&mut self) -> Result<ApprovalToken, PlanModeError> {
        let plan = self
            .current_plan
            .as_ref()
            .ok_or(PlanModeError::NoPlanToApprove)?;

        let token = ApprovalToken {
            plan_id: plan.id.clone(),
        };

        self.approved_plans.insert(plan.id.clone());
        self.current_phase = ExecutionPhase::Implementation;

        tracing::info!(
            "Plan '{}' approved, transitioning to implementation",
            plan.id
        );
        Ok(token)
    }

    /// Approve a specific plan by ID
    pub fn approve_plan(&mut self, plan_id: &str) -> Result<ApprovalToken, PlanModeError> {
        if !self.approved_plans.contains(plan_id) {
            self.approved_plans.insert(plan_id.to_string());
        }
        self.current_phase = ExecutionPhase::Implementation;
        Ok(ApprovalToken {
            plan_id: plan_id.to_string(),
        })
    }

    /// Reject the current plan, stay in planning phase
    pub fn reject(&mut self) {
        self.current_plan = None;
        self.current_phase = ExecutionPhase::Planning;
    }

    /// Reset back to planning phase (e.g., after implementation completes)
    pub fn reset(&mut self) {
        self.current_phase = ExecutionPhase::Planning;
        self.current_plan = None;
    }

    /// Check if a plan ID has been approved
    pub fn is_approved(&self, plan_id: &str) -> bool {
        self.approved_plans.contains(plan_id)
    }

    /// Get the config
    pub fn config(&self) -> &PlanModeConfig {
        &self.config
    }

    /// Check if plan mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if approval is required
    pub fn requires_approval(&self) -> bool {
        self.config.require_approval
    }
}

/// Plan mode errors
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanModeError {
    NoPlanToApprove,
    PlanNotFound(String),
    AlreadyInPhase(ExecutionPhase),
}

impl std::fmt::Display for PlanModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPlanToApprove => write!(f, "No plan submitted for approval"),
            Self::PlanNotFound(id) => write!(f, "Plan not found: {}", id),
            Self::AlreadyInPhase(phase) => write!(f, "Already in {} phase", phase),
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
    fn planning_phase_allows_read_tools() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.is_tool_allowed("read").is_ok());
        assert!(pm.is_tool_allowed("grep").is_ok());
        assert!(pm.is_tool_allowed("glob").is_ok());
    }

    #[test]
    fn planning_phase_blocks_write_tools() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.is_tool_allowed("write").is_err());
        assert!(pm.is_tool_allowed("bash").is_err());
    }

    #[test]
    fn implementation_phase_allows_all_tools() {
        let pm = PlanMode::new(PlanModeConfig::default());
        let mut pm = pm;
        pm.current_phase = ExecutionPhase::Implementation;
        assert!(pm.is_tool_allowed("write").is_ok());
        assert!(pm.is_tool_allowed("bash").is_ok());
        assert!(pm.is_tool_allowed("edit_file").is_ok());
    }

    #[test]
    fn edit_dry_run_in_planning() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.is_edit_dry_run());
    }

    #[test]
    fn edit_not_dry_run_in_implementation() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        pm.current_phase = ExecutionPhase::Implementation;
        assert!(!pm.is_edit_dry_run());
    }

    #[test]
    fn submit_and_approve_plan() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let plan = Plan {
            id: "plan-1".to_string(),
            summary: "Test plan".to_string(),
            approach: "Do the thing".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            estimated_tokens: TokenEstimate::default(),
            estimated_cost_usd: 0.05,
            risks: vec![],
            success_criteria: vec!["It works".to_string()],
            created_at: "2026-04-14".to_string(),
        };

        pm.submit_plan(plan);
        assert_eq!(pm.current_phase(), ExecutionPhase::Planning);
        assert!(pm.current_plan().is_some());

        let token = pm.approve().unwrap();
        assert_eq!(token.plan_id, "plan-1");
        assert_eq!(pm.current_phase(), ExecutionPhase::Implementation);
        assert!(pm.is_approved("plan-1"));
    }

    #[test]
    fn approve_without_plan_fails() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        assert_eq!(pm.approve(), Err(PlanModeError::NoPlanToApprove));
    }

    #[test]
    fn reject_clears_plan() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let plan = Plan {
            id: "plan-2".to_string(),
            summary: "Rejected plan".to_string(),
            approach: "".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            estimated_tokens: TokenEstimate::default(),
            estimated_cost_usd: 0.0,
            risks: vec![],
            success_criteria: vec![],
            created_at: "2026-04-14".to_string(),
        };

        pm.submit_plan(plan);
        assert!(pm.current_plan().is_some());
        pm.reject();
        assert!(pm.current_plan().is_none());
        assert_eq!(pm.current_phase(), ExecutionPhase::Planning);
    }

    #[test]
    fn reset_goes_back_to_planning() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        pm.current_phase = ExecutionPhase::Implementation;
        pm.reset();
        assert_eq!(pm.current_phase(), ExecutionPhase::Planning);
        assert!(pm.current_plan().is_none());
    }

    #[test]
    fn tool_blocked_reason_display() {
        let reason = ToolBlockedReason::NotAllowedInPhase {
            tool: "write".to_string(),
            phase: ExecutionPhase::Planning,
        };
        assert_eq!(
            reason.to_string(),
            "Tool 'write' not allowed in planning phase"
        );

        let reason = ToolBlockedReason::RequiresApproval;
        assert_eq!(reason.to_string(), "Implementation requires plan approval");
    }

    #[test]
    fn execution_phase_display() {
        assert_eq!(ExecutionPhase::Planning.to_string(), "planning");
        assert_eq!(ExecutionPhase::Implementation.to_string(), "implementation");
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
    fn approve_plan_by_id() {
        let mut pm = PlanMode::new(PlanModeConfig::default());
        let token = pm.approve_plan("external-plan-42").unwrap();
        assert_eq!(token.plan_id, "external-plan-42");
        assert_eq!(pm.current_phase(), ExecutionPhase::Implementation);
        assert!(pm.is_approved("external-plan-42"));
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
