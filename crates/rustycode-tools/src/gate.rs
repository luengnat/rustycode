use rustycode_protocol::{AgentRole, permission_role::ToolBlockedReason};

/// Trait for gating tool access based on agent role and plan state.
/// This allows low-level tool executors to check permissions without
/// depending on high-level orchestration logic.
pub trait ToolGate: Send + Sync + std::fmt::Debug {
    /// Check if a tool can be used by the given role.
    fn check_access(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason>;
}
