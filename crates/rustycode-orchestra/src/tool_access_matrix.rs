//! Role-based tool access matrix
//!
//! This module defines a per-role set of allowed tools. The matrix is
//! consumed by the PlanMode subsystem to determine if a given AgentRole
//! may invoke a particular tool during planning or implementation.
use crate::agent_identity::AgentRole;
use std::collections::{HashMap, HashSet};

/// Build the access matrix for all AgentRole variants.
///
/// The mapping declares, for each role, the set of tool names (string literals)
/// that are permitted for that role. The tool names must match the internal tool
/// registry keys used across the codebase.
pub fn build_access_matrix() -> HashMap<AgentRole, HashSet<&'static str>> {
    // Helper to collect a static list into a HashSet
    fn set_of(list: &[&'static str]) -> HashSet<&'static str> {
        list.iter().copied().collect()
    }

    let mut m: HashMap<AgentRole, HashSet<&'static str>> = HashMap::new();

    // Planner: planning-time read-only tools plus a few editing hooks for dry-run tests
    // (mirrors previous default planning tool set in PlanModeConfig).
    m.insert(
        AgentRole::Planner,
        set_of(&[
            "read",
            "read_file",
            "grep",
            "glob",
            "list_dir",
            "lsp",
            "web_search",
            "web_fetch",
            "edit_file",
        ]),
    );

    // Worker: full implementation-time tools (execution path)
    m.insert(
        AgentRole::Worker,
        set_of(&[
            // Core read tools
            "read",
            "read_file",
            // Editing/applying changes
            "edit_file",
            "write_file",
            "write",
            // Patching/search helpers
            "apply_patch",
            "search_replace",
            // System/tools
            "bash",
            // Discovery helpers
            "grep",
            "glob",
            "list_dir",
            "lsp",
            // Web access
            "web_search",
            "web_fetch",
        ]),
    );

    // Reviewer: limited read/review capabilities
    m.insert(
        AgentRole::Reviewer,
        set_of(&[
            "read",
            "read_file",
            "grep",
            "glob",
            "list_dir",
            "lsp",
            // Lightweight web access for review context
            "web_search",
        ]),
    );

    // Researcher: primarily read-only exploration
    m.insert(
        AgentRole::Researcher,
        set_of(&["read", "read_file", "grep", "glob", "list_dir", "lsp"]),
    );

    m
}
