use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::team::RiskLevel;

/// Plan for a single file modification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePlan {
    pub path: String,
    pub description: String,
}

/// Plan for a single command execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandPlan {
    pub command: String,
    pub description: String,
}

/// Risk associated with a convoy plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConvoyRisk {
    pub level: RiskLevel,
    pub description: String,
    pub mitigation: String,
}

/// Approval status for a convoy plan.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PlanApproval {
    pub approved: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// Execution plan for a convoy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConvoyPlan {
    /// Unique plan identifier.
    pub id: String,
    /// High-level summary of the feature/change.
    pub summary: String,
    /// The technical approach/strategy.
    pub approach: String,
    /// List of files expected to be modified.
    pub files_to_modify: Vec<FilePlan>,
    /// List of commands expected to be run.
    pub commands_to_run: Vec<CommandPlan>,
    /// Risks identified during planning.
    pub risks: Vec<ConvoyRisk>,
    /// Estimated cost in USD.
    pub estimated_cost_usd: f64,
    /// Success criteria defined during planning.
    pub success_criteria: Vec<String>,
    /// Approval status and tracking.
    pub approval: PlanApproval,
    /// When this plan was created.
    pub created_at: DateTime<Utc>,
}
