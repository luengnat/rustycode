//! Orchestra Universal Config Tools — Tool registry for AI coding tools.
//!
//! Known AI coding tools with their config directory locations.
//! Based on research of Oh My Pi's discovery system and direct config
//! file inspection of each tool.
//!
//! Matches orchestra-2's tools.ts implementation.

/// Information about an AI coding tool
#[derive(Debug, Clone, PartialEq)]
pub struct ToolInfo {
    /// Unique identifier for the tool
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// User-level config directory (e.g., ~/.claude)
    pub user_dir: Option<&'static str>,
    /// Project-level config directory (e.g., .claude/)
    pub project_dir: Option<&'static str>,
}

/// Registry of known AI coding tools
pub const TOOLS: &[ToolInfo] = &[
    ToolInfo {
        id: "claude",
        name: "Claude Code",
        user_dir: Some(".claude"),
        project_dir: Some(".claude"),
    },
    ToolInfo {
        id: "cursor",
        name: "Cursor",
        user_dir: Some(".cursor"),
        project_dir: Some(".cursor"),
    },
    ToolInfo {
        id: "windsurf",
        name: "Windsurf",
        user_dir: Some(".codeium/windsurf"),
        project_dir: Some(".windsurf"),
    },
    ToolInfo {
        id: "gemini",
        name: "Gemini CLI",
        user_dir: Some(".gemini"),
        project_dir: Some(".gemini"),
    },
    ToolInfo {
        id: "codex",
        name: "OpenAI Codex",
        user_dir: Some(".codex"),
        project_dir: Some(".codex"),
    },
    ToolInfo {
        id: "cline",
        name: "Cline",
        user_dir: None,
        project_dir: None, // Uses root-level .clinerules (handled specially)
    },
    ToolInfo {
        id: "github-copilot",
        name: "GitHub Copilot",
        user_dir: None,
        project_dir: Some(".github"),
    },
    ToolInfo {
        id: "vscode",
        name: "VS Code",
        user_dir: None,
        project_dir: Some(".vscode"),
    },
];

/// Find tool info by ID
///
/// # Arguments
/// * `id` - Tool identifier (e.g., "claude", "cursor")
///
/// # Returns
/// Tool info or `None` if not found
///
/// # Examples
/// ```
/// use rustycode_orchestra::universal_config_tools::get_tool;
///
/// let claude = get_tool("claude");
/// assert!(claude.is_some());
/// assert_eq!(claude.unwrap().name, "Claude Code");
///
/// let unknown = get_tool("unknown");
/// assert!(unknown.is_none());
/// ```
pub fn get_tool(id: &str) -> Option<&'static ToolInfo> {
    TOOLS.iter().find(|tool| tool.id == id)
}

/// Get all tools that have project-level config directories
///
/// # Returns
/// Iterator over tools with project directories
///
/// # Examples
/// ```
/// use rustycode_orchestra::universal_config_tools::get_tools_with_project_config;
///
/// let tools: Vec<_> = get_tools_with_project_config().collect();
/// assert!(!tools.is_empty());
/// ```
pub fn get_tools_with_project_config() -> impl Iterator<Item = &'static ToolInfo> {
    TOOLS.iter().filter(|tool| tool.project_dir.is_some())
}

/// Get all tools that have user-level config directories
///
/// # Returns
/// Iterator over tools with user directories
///
/// # Examples
/// ```
/// use rustycode_orchestra::universal_config_tools::get_tools_with_user_config;
///
/// let tools: Vec<_> = get_tools_with_user_config().collect();
/// assert!(!tools.is_empty());
/// ```
pub fn get_tools_with_user_config() -> impl Iterator<Item = &'static ToolInfo> {
    TOOLS.iter().filter(|tool| tool.user_dir.is_some())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tool_existing() {
        let claude = get_tool("claude");
        assert!(claude.is_some());
        assert_eq!(claude.unwrap().name, "Claude Code");
    }

    #[test]
    fn test_get_tool_not_found() {
        let unknown = get_tool("unknown-tool");
        assert!(unknown.is_none());
    }

    #[test]
    fn test_get_tool_cursor() {
        let cursor = get_tool("cursor");
        assert!(cursor.is_some());
        assert_eq!(cursor.unwrap().id, "cursor");
        assert_eq!(cursor.unwrap().user_dir, Some(".cursor"));
        assert_eq!(cursor.unwrap().project_dir, Some(".cursor"));
    }

    #[test]
    fn test_get_tools_with_project_config() {
        let tools: Vec<_> = get_tools_with_project_config().collect();
        assert!(!tools.is_empty());

        // Claude should be in the list
        assert!(tools.iter().any(|t| t.id == "claude"));

        // Cline should NOT be in the list (uses .clinerules at root)
        assert!(!tools.iter().any(|t| t.id == "cline"));
    }

    #[test]
    fn test_get_tools_with_user_config() {
        let tools: Vec<_> = get_tools_with_user_config().collect();
        assert!(!tools.is_empty());

        // Claude should be in the list
        assert!(tools.iter().any(|t| t.id == "claude"));

        // VS Code should NOT be in the list (no user config)
        assert!(!tools.iter().any(|t| t.id == "vscode"));
    }

    #[test]
    fn test_cline_special_case() {
        let cline = get_tool("cline");
        assert!(cline.is_some());
        assert_eq!(cline.unwrap().user_dir, None);
        assert_eq!(cline.unwrap().project_dir, None);
    }

    #[test]
    fn test_all_tools_have_unique_ids() {
        let mut ids = std::collections::HashSet::new();
        for tool in TOOLS {
            assert!(ids.insert(tool.id), "Duplicate tool ID: {}", tool.id);
        }
    }

    #[test]
    fn test_tool_count() {
        assert_eq!(TOOLS.len(), 8);
    }

    #[test]
    fn test_windsurf_nested_config() {
        let windsurf = get_tool("windsurf");
        assert!(windsurf.is_some());
        assert_eq!(windsurf.unwrap().user_dir, Some(".codeium/windsurf"));
        assert_eq!(windsurf.unwrap().project_dir, Some(".windsurf"));
    }

    #[test]
    fn test_github_copilot_project_only() {
        let copilot = get_tool("github-copilot");
        assert!(copilot.is_some());
        assert_eq!(copilot.unwrap().user_dir, None);
        assert_eq!(copilot.unwrap().project_dir, Some(".github"));
    }
}
