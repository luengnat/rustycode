use serde::{Deserialize, Serialize};
use std::fmt;
use crate::agent_protocol::AgentRole;

/// Permissions roles for high-level agent tasking and tool access gating.
///
/// **Deprecated in favor of `AgentRole`.** `PermissionRole` overlaps with
/// `AgentRole` but has fewer variants. New code should use `AgentRole` directly.
/// This type is kept for backwards compatibility with serialized data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionRole {
    /// Autonomous task executor. Can read and write.
    Worker,
    /// Plan-only agent. Can read and write plans, but not application code.
    Planner,
    /// Verification-only agent. Can read and run tests, but not write code.
    Reviewer,
    /// Research agent. Read-only exploration.
    Researcher,
    /// Strategic agent. Can define architecture and review plans.
    Architect,
    /// Critical reviewer. Only allowed to read and verify.
    Skeptic,
    /// Final decider. Allows terminal verification and high-level approval.
    Judge,
}

impl PermissionRole {
    /// Whether this role is intended to modify application code.
    pub fn can_write_code(&self) -> bool {
        matches!(self, Self::Worker | Self::Architect)
    }

    /// Whether this role is intended to create/modify plans.
    pub fn can_manage_plans(&self) -> bool {
        matches!(self, Self::Worker | Self::Planner | Self::Architect)
    }
}

impl fmt::Display for PermissionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Worker => write!(f, "worker"),
            Self::Planner => write!(f, "planner"),
            Self::Reviewer => write!(f, "reviewer"),
            Self::Researcher => write!(f, "researcher"),
            Self::Architect => write!(f, "architect"),
            Self::Skeptic => write!(f, "skeptic"),
            Self::Judge => write!(f, "judge"),
        }
    }
}


/// Reason why a tool invocation was blocked by the permission system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolBlockedReason {
    /// The agent's role does not have permission to use this tool.
    NotAllowedForRole { tool: String, role: AgentRole },
    /// Plan approval is required before this tool can be used.
    RequiresApproval,
    /// The associated convoy plan has not been approved yet.
    ConvoyPlanNotApproved,
    /// The agent role is unknown to the permission system.
    UnknownRole(AgentRole),
}

impl fmt::Display for ToolBlockedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAllowedForRole { tool, role } => {
                write!(f, "Tool '{}' not allowed for {:?} role", tool, role)
            }
            Self::RequiresApproval => {
                write!(f, "Plan approval required before tool access")
            }
            Self::ConvoyPlanNotApproved => {
                write!(f, "Convoy plan not yet approved")
            }
            Self::UnknownRole(role) => {
                write!(f, "Unknown agent role: {:?}", role)
            }
        }
    }
}

impl std::error::Error for ToolBlockedReason {}
