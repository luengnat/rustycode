//! Role-based tool access matrix.
//!
//! Defines which tools each agent role can access during execution.

use std::collections::{HashMap, HashSet};
use rustycode_protocol::AgentRole;

/// Build the role-to-tools access matrix
pub fn build_access_matrix() -> HashMap<AgentRole, HashSet<&'static str>> {
    let mut matrix = HashMap::new();

    // Planner: Analyze, research, write plans
    let mut planner_tools = HashSet::new();
    planner_tools.extend(&["read", "read_file", "grep", "glob", "list_dir", "lsp", "web_search", "web_fetch", "write", "edit_file", "bash", "Agent", "TaskCreate", "TaskList", "TaskUpdate"]);
    matrix.insert(AgentRole::Planner, planner_tools);

    // Worker: Execute approved plans
    let mut worker_tools = HashSet::new();
    worker_tools.extend(&["read", "read_file", "grep", "glob", "list_dir", "lsp", "write", "edit_file", "apply_patch", "bash"]);
    matrix.insert(AgentRole::Worker, worker_tools);

    // Reviewer: Verify and test
    let mut reviewer_tools = HashSet::new();
    reviewer_tools.extend(&["read", "read_file", "grep", "glob", "lsp", "web_fetch", "bash"]);
    matrix.insert(AgentRole::Reviewer, reviewer_tools);

    // Researcher: Explore only
    let mut researcher_tools = HashSet::new();
    researcher_tools.extend(&["read", "read_file", "grep", "glob", "lsp", "web_search", "web_fetch", "Agent"]);
    matrix.insert(AgentRole::Researcher, researcher_tools);

    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_has_all_roles() {
        let matrix = build_access_matrix();
        assert!(matrix.contains_key(&AgentRole::Planner));
        assert!(matrix.contains_key(&AgentRole::Worker));
        assert!(matrix.contains_key(&AgentRole::Reviewer));
        assert!(matrix.contains_key(&AgentRole::Researcher));
    }

    #[test]
    fn planner_has_write_and_bash() {
        let matrix = build_access_matrix();
        let planner = &matrix[&AgentRole::Planner];
        assert!(planner.contains("write"));
        assert!(planner.contains("bash"));
    }

    #[test]
    fn reviewer_cannot_write() {
        let matrix = build_access_matrix();
        let reviewer = &matrix[&AgentRole::Reviewer];
        assert!(!reviewer.contains("write"));
        assert!(!reviewer.contains("edit_file"));
    }
}
