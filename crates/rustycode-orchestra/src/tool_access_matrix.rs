//! Role-based tool access matrix.
//!
//! Defines which tools each agent role can access during execution.
//! Tool names MUST match the `fn name()` return values of the actual tool
//! implementations in `rustycode-tools`.

use std::collections::{HashMap, HashSet};
use rustycode_protocol::AgentRole;

/// Build the role-to-tools access matrix
pub fn build_access_matrix() -> HashMap<AgentRole, HashSet<&'static str>> {
    let mut matrix = HashMap::new();

    // Planner: Analyze, research, write plans
    let mut planner_tools = HashSet::new();
    planner_tools.extend(&[
        // File ops
        "read_file", "write_file", "list_dir",
        // Search
        "grep", "glob",
        // Shell
        "bash",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_completion",
        "lsp_document_symbols", "lsp_references", "lsp_full_diagnostics",
        "lsp_code_actions", "lsp_rename", "lsp_formatting",
        "lsp_get_symbols_overview", "lsp_find_symbol", "lsp_workspace_symbols",
        // Web
        "web_search", "web_fetch",
        // Editing
        "edit_file", "multiedit", "apply_patch",
        // Sub-agent
        "task",
        // Task management
        "TaskCreate", "TaskList", "TaskUpdate", "TaskGet",
    ]);
    matrix.insert(AgentRole::Planner, planner_tools);

    // Worker: Execute approved plans
    let mut worker_tools = HashSet::new();
    worker_tools.extend(&[
        // File ops
        "read_file", "write_file", "list_dir",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_completion",
        "lsp_document_symbols", "lsp_references",
        // Editing
        "edit_file", "multiedit", "apply_patch",
        // Shell
        "bash",
    ]);
    matrix.insert(AgentRole::Worker, worker_tools);

    // Reviewer: Verify and test
    let mut reviewer_tools = HashSet::new();
    reviewer_tools.extend(&[
        // File ops (read-only)
        "read_file",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_references",
        "lsp_document_symbols", "lsp_find_symbol",
        // Web
        "web_fetch",
        // Shell (for running tests/verification)
        "bash",
    ]);
    matrix.insert(AgentRole::Reviewer, reviewer_tools);

    // Researcher: Explore only
    let mut researcher_tools = HashSet::new();
    researcher_tools.extend(&[
        // File ops (read-only)
        "read_file",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_references",
        "lsp_document_symbols",
        // Web
        "web_search", "web_fetch",
        // Sub-agent (for spawning explore agents)
        "task",
    ]);
    matrix.insert(AgentRole::Researcher, researcher_tools);

    // Architect: Strategy, structure, and research
    let mut architect_tools = HashSet::new();
    architect_tools.extend(&[
        // File ops
        "read_file", "write_file", "list_dir",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_completion",
        "lsp_document_symbols", "lsp_references", "lsp_full_diagnostics",
        "lsp_code_actions", "lsp_rename", "lsp_formatting",
        "lsp_get_symbols_overview", "lsp_find_symbol", "lsp_workspace_symbols",
        // Web
        "web_search", "web_fetch",
        // Editing
        "edit_file", "multiedit",
        // Sub-agent
        "task",
        // Task management
        "TaskCreate", "TaskList", "TaskUpdate", "TaskGet",
    ]);
    matrix.insert(AgentRole::Architect, architect_tools);

    // Builder: Implementation (similar to Worker)
    let mut builder_tools = HashSet::new();
    builder_tools.extend(&[
        // File ops
        "read_file", "write_file", "list_dir",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_completion",
        "lsp_document_symbols", "lsp_references",
        // Editing
        "edit_file", "multiedit", "apply_patch",
        // Shell
        "bash",
    ]);
    matrix.insert(AgentRole::Builder, builder_tools);

    // Skeptic: Strictly read-only for verification
    let mut skeptic_tools = HashSet::new();
    skeptic_tools.extend(&[
        // File ops (read-only)
        "read_file",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_references",
        "lsp_document_symbols",
        // Web
        "web_fetch",
    ]);
    matrix.insert(AgentRole::Skeptic, skeptic_tools);

    // Judge: Verification via tests/bash
    let mut judge_tools = HashSet::new();
    judge_tools.extend(&[
        // File ops (read-only)
        "read_file",
        // Search
        "grep", "glob",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition", "lsp_references",
        // Shell (for running verification)
        "bash",
    ]);
    matrix.insert(AgentRole::Judge, judge_tools);

    // Scalpel: Surgical fixes (specialized edit)
    let mut scalpel_tools = HashSet::new();
    scalpel_tools.extend(&[
        // File ops (read + targeted edit)
        "read_file",
        // Search
        "grep",
        // LSP
        "lsp_diagnostics", "lsp_hover", "lsp_definition",
        // Editing
        "edit_file", "apply_patch", "multiedit",
    ]);
    matrix.insert(AgentRole::Scalpel, scalpel_tools);

    // Coordinator: High-level orchestration
    let mut coordinator_tools = HashSet::new();
    coordinator_tools.extend(&[
        // Read-only monitoring
        "read_file",
        // Task management
        "TaskCreate", "TaskList", "TaskUpdate", "TaskGet",
        // Sub-agent
        "task",
    ]);
    matrix.insert(AgentRole::Coordinator, coordinator_tools);

    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_has_all_roles() {
        let matrix = build_access_matrix();
        let expected_roles = vec![
            AgentRole::Planner,
            AgentRole::Worker,
            AgentRole::Reviewer,
            AgentRole::Researcher,
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Skeptic,
            AgentRole::Judge,
            AgentRole::Scalpel,
            AgentRole::Coordinator,
        ];
        for role in expected_roles {
            assert!(matrix.contains_key(&role), "Missing role: {:?}", role);
        }
    }

    #[test]
    fn planner_has_write_and_bash() {
        let matrix = build_access_matrix();
        let planner = &matrix[&AgentRole::Planner];
        assert!(planner.contains("write_file"));
        assert!(planner.contains("bash"));
    }

    #[test]
    fn skeptic_cannot_write_or_bash() {
        let matrix = build_access_matrix();
        let skeptic = &matrix[&AgentRole::Skeptic];
        assert!(!skeptic.contains("write_file"));
        assert!(!skeptic.contains("edit_file"));
        assert!(!skeptic.contains("bash"));
    }

    #[test]
    fn reviewer_cannot_write() {
        let matrix = build_access_matrix();
        let reviewer = &matrix[&AgentRole::Reviewer];
        assert!(!reviewer.contains("write_file"));
        assert!(!reviewer.contains("edit_file"));
    }

    #[test]
    fn every_read_tool_is_in_readonly_roles() {
        let matrix = build_access_matrix();
        let readonly_roles = [AgentRole::Skeptic, AgentRole::Researcher];
        for role in &readonly_roles {
            let tools = &matrix[role];
            assert!(tools.contains("read_file"), "{:?} missing 'read_file'", role);
            assert!(tools.contains("grep"), "{:?} missing 'grep'", role);
            assert!(tools.contains("glob"), "{:?} missing 'glob'", role);
        }
    }

    #[test]
    fn no_write_role_has_apply_patch() {
        let matrix = build_access_matrix();
        let write_roles = [AgentRole::Worker, AgentRole::Builder, AgentRole::Scalpel];
        for role in &write_roles {
            let tools = &matrix[role];
            assert!(tools.contains("apply_patch"), "{:?} missing 'apply_patch'", role);
        }
    }

    #[test]
    fn coordinator_has_task_tools_but_not_execution() {
        let matrix = build_access_matrix();
        let coordinator = &matrix[&AgentRole::Coordinator];
        assert!(coordinator.contains("TaskList"));
        assert!(coordinator.contains("TaskUpdate"));
        assert!(coordinator.contains("TaskGet"));
        assert!(!coordinator.contains("bash"));
        assert!(!coordinator.contains("write_file"));
        assert!(!coordinator.contains("edit_file"));
    }

    #[test]
    fn judge_has_bash_for_verification() {
        let matrix = build_access_matrix();
        let judge = &matrix[&AgentRole::Judge];
        assert!(judge.contains("bash"));
        assert!(!judge.contains("write_file"));
        assert!(!judge.contains("edit_file"));
    }

    #[test]
    fn scalpel_has_surgical_tools_only() {
        let matrix = build_access_matrix();
        let scalpel = &matrix[&AgentRole::Scalpel];
        assert!(scalpel.contains("edit_file"));
        assert!(scalpel.contains("apply_patch"));
        assert!(scalpel.contains("read_file"));
        assert!(!scalpel.contains("bash"));
        assert!(!scalpel.contains("glob"));
    }

    #[test]
    fn architect_and_planner_have_task_tool() {
        let matrix = build_access_matrix();
        assert!(matrix[&AgentRole::Architect].contains("task"));
        assert!(matrix[&AgentRole::Planner].contains("task"));
        // Workers don't spawn sub-agents
        assert!(!matrix[&AgentRole::Worker].contains("task"));
    }

    #[test]
    fn matrix_entry_counts_reasonable() {
        let matrix = build_access_matrix();
        // No role should have zero tools
        for (role, tools) in &matrix {
            assert!(!tools.is_empty(), "{:?} has zero tools", role);
        }
        // Coordinator should have fewest tools
        let coord_count = matrix[&AgentRole::Coordinator].len();
        let planner_count = matrix[&AgentRole::Planner].len();
        assert!(coord_count < planner_count, "Coordinator should have fewer tools than Planner");
    }

    // ── New tests: verify tool names match actual registry ──────────────────

    #[test]
    fn all_write_roles_use_write_file_not_write() {
        let matrix = build_access_matrix();
        let write_roles = [
            AgentRole::Planner,
            AgentRole::Worker,
            AgentRole::Architect,
            AgentRole::Builder,
        ];
        for role in &write_roles {
            let tools = &matrix[role];
            // Must use actual tool name "write_file"
            assert!(tools.contains("write_file"), "{:?} missing 'write_file'", role);
            // Must NOT contain the dead alias "write"
            assert!(!tools.contains("write"), "{:?} has dead alias 'write'", role);
        }
    }

    #[test]
    fn no_dead_read_alias() {
        let matrix = build_access_matrix();
        for (role, tools) in &matrix {
            // "read" is dead — actual tool is "read_file"
            if tools.contains("read_file") {
                assert!(!tools.contains("read"), "{:?} has dead alias 'read'", role);
            }
        }
    }

    #[test]
    fn no_dead_agent_alias() {
        let matrix = build_access_matrix();
        for (role, tools) in &matrix {
            // "Agent" is dead — actual tool is "task"
            if tools.contains("task") {
                assert!(!tools.contains("Agent"), "{:?} has dead alias 'Agent'", role);
            }
        }
    }

    #[test]
    fn sub_agent_roles_have_task_not_agent() {
        let matrix = build_access_matrix();
        let task_roles = [AgentRole::Planner, AgentRole::Architect, AgentRole::Researcher, AgentRole::Coordinator];
        for role in &task_roles {
            assert!(matrix[role].contains("task"), "{:?} missing 'task'", role);
            assert!(!matrix[role].contains("Agent"), "{:?} has dead 'Agent'", role);
        }
    }

    #[test]
    fn write_roles_have_multiedit() {
        let matrix = build_access_matrix();
        let edit_roles = [AgentRole::Planner, AgentRole::Worker, AgentRole::Architect, AgentRole::Builder, AgentRole::Scalpel];
        for role in &edit_roles {
            assert!(matrix[role].contains("multiedit"), "{:?} missing 'multiedit'", role);
        }
    }

    #[test]
    fn readonly_roles_have_no_edit_tools() {
        let matrix = build_access_matrix();
        let readonly_roles = [AgentRole::Skeptic];
        for role in &readonly_roles {
            let tools = &matrix[role];
            assert!(!tools.contains("edit_file"), "{:?} should not have edit_file", role);
            assert!(!tools.contains("multiedit"), "{:?} should not have multiedit", role);
            assert!(!tools.contains("apply_patch"), "{:?} should not have apply_patch", role);
            assert!(!tools.contains("write_file"), "{:?} should not have write_file", role);
        }
    }

    /// Verify no tool name in any matrix is a known-dead alias.
    #[test]
    fn no_known_dead_aliases_anywhere() {
        let dead_aliases = ["read", "write", "Agent", "lsp"];
        let matrix = build_access_matrix();
        for (role, tools) in &matrix {
            for alias in &dead_aliases {
                assert!(!tools.contains(alias), "{:?} contains dead alias '{}'", role, alias);
            }
        }
    }
}
