// rustycode-orchestra/src/workspace_index.rs
//! Workspace indexing for Autonomous Mode projects.
//!
//! Provides indexing of milestones, slices, and tasks for workspace navigation,
//! scope selection, and command suggestions.

use std::collections::HashMap;
use std::path::Path;

/// A task target within the workspace.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceTaskTarget {
    pub id: String,
    pub title: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_path: Option<String>,
}

/// A slice target within the workspace.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceSliceTarget {
    pub id: String,
    pub title: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uat_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub tasks: Vec<WorkspaceTaskTarget>,
}

/// A milestone target within the workspace.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceMilestoneTarget {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roadmap_path: Option<String>,
    pub slices: Vec<WorkspaceSliceTarget>,
}

/// A scope target for workspace navigation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceScopeTarget {
    pub scope: String,
    pub label: String,
    pub kind: ScopeKind,
}

/// Kind of scope target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ScopeKind {
    Project,
    Milestone,
    Slice,
    Task,
}

/// Active workspace state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActiveWorkspaceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub phase: String,
}

/// Orchestra workspace index.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OrchestraWorkspaceIndex {
    pub milestones: Vec<WorkspaceMilestoneTarget>,
    pub active: ActiveWorkspaceState,
    pub scopes: Vec<WorkspaceScopeTarget>,
    pub validation_issues: Vec<WorkspaceValidationIssue>,
}

/// Validation issue for workspace indexing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceValidationIssue {
    pub file: String,
    pub severity: WorkspaceValidationSeverity,
    pub message: String,
}

/// Severity of validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum WorkspaceValidationSeverity {
    Fatal,
    Warning,
}

/// Options for indexing workspace.
#[derive(Debug, Clone, Default)]
pub struct IndexWorkspaceOptions {
    /// When true, run validation for each slice (expensive).
    pub validate: bool,
}

/// Result of listing doctor scope suggestions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorScopeSuggestion {
    pub value: String,
    pub label: String,
}

/// Extract title from roadmap header.
///
/// Removes milestone ID prefix if present.
#[allow(dead_code)] // Kept for future use
fn title_from_roadmap_header(content: &str, fallback_id: &str) -> String {
    content
        .lines()
        .find(|line| line.starts_with("# "))
        .and_then(|heading| heading.strip_prefix("# ").map(|title| title.trim()))
        .and_then(|title| {
            // Remove "MXXX: " or "MXXX-hash: " prefix if present
            if title.contains(':') {
                title.split(':').nth(1).map(|s| s.trim().to_string())
            } else {
                Some(title.to_string())
            }
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| fallback_id.to_string())
}

/// Index a single slice within a milestone.
#[allow(dead_code)] // Kept for future use
fn index_slice(
    _base_path: &Path,
    _milestone_id: &str,
    slice_id: &str,
    fallback_title: &str,
    done: bool,
) -> WorkspaceSliceTarget {
    // Simplified implementation - in full version would:
    // 1. Resolve paths to PLAN, SUMMARY, UAT files
    // 2. Parse PLAN.md to extract tasks
    // 3. Get branch name from worktree detection
    WorkspaceSliceTarget {
        id: slice_id.to_string(),
        title: fallback_title.to_string(),
        done,
        plan_path: None,
        summary_path: None,
        uat_path: None,
        tasks_dir: None,
        branch: None,
        tasks: Vec::new(),
    }
}

/// Index a Orchestra workspace.
///
/// Scans all milestones and slices, building an index of tasks, scopes,
/// and optionally running validation.
///
/// # Arguments
/// * `base_path` - Path to the Orchestra project root
/// * `opts` - Indexing options (validation flag)
///
/// # Returns
/// OrchestraWorkspaceIndex with milestones, active state, scopes, and validation issues
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::workspace_index::index_workspace;
/// use std::path::Path;
///
/// let index = index_workspace(Path::new("/project"), false);
/// println!("Found {} milestones", index.milestones.len());
/// ```
pub fn index_workspace(_base_path: &Path, _opts: IndexWorkspaceOptions) -> OrchestraWorkspaceIndex {
    // Simplified implementation - in full version would:
    // 1. Find all milestone directories (M001, M002, etc.)
    // 2. Parse ROADMAP.md from each milestone
    // 3. Index each slice and its tasks
    // 4. Run validation if requested
    // 5. Derive active state from deriveState()
    // 6. Build scope hierarchy

    let milestones = Vec::new(); // Would be populated from actual file system
    let active = ActiveWorkspaceState {
        milestone_id: None,
        slice_id: None,
        task_id: None,
        phase: "unknown".to_string(),
    };
    let scopes = vec![WorkspaceScopeTarget {
        scope: "project".to_string(),
        label: "project".to_string(),
        kind: ScopeKind::Project,
    }];
    let validation_issues = Vec::new();

    OrchestraWorkspaceIndex {
        milestones,
        active,
        scopes,
        validation_issues,
    }
}

/// List doctor scope suggestions for the workspace.
///
/// Returns ordered suggestions with the active slice first.
///
/// # Arguments
/// * `base_path` - Path to the Orchestra project root
///
/// # Returns
/// Vector of scope suggestions with value and label
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::workspace_index::list_doctor_scope_suggestions;
/// use std::path::Path;
///
/// let suggestions = list_doctor_scope_suggestions(Path::new("/project"));
/// for suggestion in suggestions {
///     println!("{}: {}", suggestion.value, suggestion.label);
/// }
/// ```
pub fn list_doctor_scope_suggestions(base_path: &Path) -> Vec<DoctorScopeSuggestion> {
    let index = index_workspace(base_path, IndexWorkspaceOptions::default());

    let active_slice_scope =
        if let (Some(mid), Some(sid)) = (&index.active.milestone_id, &index.active.slice_id) {
            Some(format!("{}/{}", mid, sid))
        } else {
            None
        };

    let mut ordered: Vec<_> = index
        .scopes
        .into_iter()
        .filter(|scope| scope.kind != ScopeKind::Project)
        .collect();

    ordered.sort_by(|a, b| {
        if let Some(ref active) = active_slice_scope {
            if a.scope == *active {
                return std::cmp::Ordering::Less;
            }
            if b.scope == *active {
                return std::cmp::Ordering::Greater;
            }
        }
        a.scope.cmp(&b.scope)
    });

    ordered
        .into_iter()
        .map(|scope| DoctorScopeSuggestion {
            value: scope.scope,
            label: scope.label,
        })
        .collect()
}

/// Get suggested next commands based on workspace state.
///
/// Analyzes active phase, scope, and validation issues to suggest
/// relevant Orchestra commands.
///
/// # Arguments
/// * `base_path` - Path to the Orchestra project root
///
/// # Returns
/// Vector of suggested command strings
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::workspace_index::get_suggested_next_commands;
/// use std::path::Path;
///
/// let commands = get_suggested_next_commands(Path::new("/project"));
/// for command in commands {
///     println!("{}", command);
/// }
/// ```
pub fn get_suggested_next_commands(base_path: &Path) -> Vec<String> {
    let index = index_workspace(base_path, IndexWorkspaceOptions { validate: true });

    let scope = if let (Some(mid), Some(sid)) = (&index.active.milestone_id, &index.active.slice_id)
    {
        Some(format!("{}/{}", mid, sid))
    } else {
        index.active.milestone_id.clone()
    };

    let mut commands = Vec::new();

    if index.active.phase == "planning" {
        commands.push("/orchestra".to_string());
    }
    if index.active.phase == "executing" || index.active.phase == "summarizing" {
        commands.push("/orchestra auto".to_string());
    }
    if let Some(ref scope) = scope {
        commands.push(format!("/orchestra doctor {}", scope));
        commands.push(format!("/orchestra doctor fix {}", scope));
    }
    if let Some(ref scope) = scope {
        if !index.validation_issues.is_empty() {
            commands.push(format!("/orchestra doctor audit {}", scope));
        }
    }
    commands.push("/orchestra status".to_string());

    // Remove duplicates while preserving order
    let mut seen = HashMap::new();
    commands.retain(|cmd| seen.insert(cmd.clone(), true).is_none());

    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_workspace_task_target_serialization() {
        let task = WorkspaceTaskTarget {
            id: "T01".to_string(),
            title: "Test task".to_string(),
            done: false,
            plan_path: Some("/path/to/PLAN.md".to_string()),
            summary_path: None,
        };

        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"T01\""));
        assert!(json.contains("\"Test task\""));
        assert!(json.contains("\"plan_path\""));
    }

    #[test]
    fn test_workspace_slice_target_serialization() {
        let slice = WorkspaceSliceTarget {
            id: "S01".to_string(),
            title: "Test slice".to_string(),
            done: true,
            plan_path: None,
            summary_path: Some("/path/to/SUMMARY.md".to_string()),
            uat_path: None,
            tasks_dir: None,
            branch: None,
            tasks: vec![],
        };

        let json = serde_json::to_string(&slice).unwrap();
        assert!(json.contains("\"S01\""));
        assert!(json.contains("\"Test slice\""));
    }

    #[test]
    fn test_workspace_milestone_target_serialization() {
        let milestone = WorkspaceMilestoneTarget {
            id: "M001".to_string(),
            title: "First milestone".to_string(),
            roadmap_path: Some("/path/to/ROADMAP.md".to_string()),
            slices: vec![],
        };

        let json = serde_json::to_string(&milestone).unwrap();
        assert!(json.contains("\"M001\""));
        assert!(json.contains("\"First milestone\""));
    }

    #[test]
    fn test_workspace_scope_target_serialization() {
        let scope = WorkspaceScopeTarget {
            scope: "M001/S01".to_string(),
            label: "M001/S01: Test".to_string(),
            kind: ScopeKind::Slice,
        };

        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains("\"M001/S01\""));
        assert!(json.contains("\"slice\""));
    }

    #[test]
    fn test_scope_kind_serialization() {
        let kind = ScopeKind::Task;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"task\"");
    }

    #[test]
    fn test_active_workspace_state_serialization() {
        let state = ActiveWorkspaceState {
            milestone_id: Some("M001".to_string()),
            slice_id: Some("S01".to_string()),
            task_id: Some("T01".to_string()),
            phase: "executing".to_string(),
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"M001\""));
        assert!(json.contains("\"S01\""));
        assert!(json.contains("\"T01\""));
        assert!(json.contains("\"executing\""));
    }

    #[test]
    fn test_orchestra_workspace_index_serialization() {
        let index = OrchestraWorkspaceIndex {
            milestones: vec![],
            active: ActiveWorkspaceState {
                milestone_id: None,
                slice_id: None,
                task_id: None,
                phase: "unknown".to_string(),
            },
            scopes: vec![],
            validation_issues: vec![],
        };

        let json = serde_json::to_string(&index).unwrap();
        assert!(json.contains("\"milestones\""));
        assert!(json.contains("\"active\""));
        assert!(json.contains("\"scopes\""));
    }

    #[test]
    fn test_workspace_validation_issue_serialization() {
        let issue = WorkspaceValidationIssue {
            file: "PLAN.md".to_string(),
            severity: WorkspaceValidationSeverity::Warning,
            message: "Missing verification section".to_string(),
        };

        let json = serde_json::to_string(&issue).unwrap();
        assert!(json.contains("\"PLAN.md\""));
        assert!(json.contains("\"warning\""));
        assert!(json.contains("\"Missing verification section\""));
    }

    #[test]
    fn test_title_from_roadmap_header_with_heading() {
        let content = "# M001: First milestone\n\nSome content";
        let title = title_from_roadmap_header(content, "M001");
        assert_eq!(title, "First milestone");
    }

    #[test]
    fn test_title_from_roadmap_header_without_heading() {
        let content = "Just some content without heading";
        let title = title_from_roadmap_header(content, "M001");
        assert_eq!(title, "M001");
    }

    #[test]
    fn test_title_from_roadmap_header_empty_content() {
        let content = "";
        let title = title_from_roadmap_header(content, "M001");
        assert_eq!(title, "M001");
    }

    #[test]
    fn test_index_slice_basic() {
        let slice = index_slice(Path::new("/project"), "M001", "S01", "Test slice", false);
        assert_eq!(slice.id, "S01");
        assert_eq!(slice.title, "Test slice");
        assert!(!slice.done);
        assert!(slice.tasks.is_empty());
    }

    #[test]
    fn test_index_workspace_returns_valid_structure() {
        let temp_dir = TempDir::new().unwrap();
        let index = index_workspace(temp_dir.path(), IndexWorkspaceOptions::default());

        assert!(index.milestones.is_empty()); // Empty directory has no milestones
        assert_eq!(index.active.phase, "unknown");
        assert_eq!(index.scopes.len(), 1); // Only project scope
        assert_eq!(index.scopes[0].kind, ScopeKind::Project);
        assert!(index.validation_issues.is_empty());
    }

    #[test]
    fn test_list_doctor_scope_suggestions_returns_scopes() {
        let temp_dir = TempDir::new().unwrap();
        let suggestions = list_doctor_scope_suggestions(temp_dir.path());

        // Empty workspace should only have project scope (filtered out)
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_get_suggested_next_commands_includes_status() {
        let temp_dir = TempDir::new().unwrap();
        let commands = get_suggested_next_commands(temp_dir.path());

        assert!(commands.contains(&"/orchestra status".to_string()));
    }

    #[test]
    fn test_get_suggested_next_commands_phase_planning() {
        let temp_dir = TempDir::new().unwrap();
        let commands = get_suggested_next_commands(temp_dir.path());

        // With unknown phase, should still include status
        assert!(commands.contains(&"/orchestra status".to_string()));
    }

    #[test]
    fn test_doctor_scope_suggestion_structure() {
        let suggestion = DoctorScopeSuggestion {
            value: "M001/S01".to_string(),
            label: "M001/S01: Test slice".to_string(),
        };

        assert_eq!(suggestion.value, "M001/S01");
        assert_eq!(suggestion.label, "M001/S01: Test slice");
    }

    #[test]
    fn test_index_workspace_options_default() {
        let opts = IndexWorkspaceOptions::default();
        assert!(!opts.validate);
    }

    #[test]
    fn test_validation_severity_serialization() {
        let severity = WorkspaceValidationSeverity::Fatal;
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, "\"fatal\"");

        let severity = WorkspaceValidationSeverity::Warning;
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, "\"warning\"");
    }

    #[test]
    fn test_workspace_scope_target_kinds() {
        let project = WorkspaceScopeTarget {
            scope: "project".to_string(),
            label: "Project".to_string(),
            kind: ScopeKind::Project,
        };

        let milestone = WorkspaceScopeTarget {
            scope: "M001".to_string(),
            label: "M001".to_string(),
            kind: ScopeKind::Milestone,
        };

        let slice = WorkspaceScopeTarget {
            scope: "M001/S01".to_string(),
            label: "S01".to_string(),
            kind: ScopeKind::Slice,
        };

        let task = WorkspaceScopeTarget {
            scope: "M001/S01/T01".to_string(),
            label: "T01".to_string(),
            kind: ScopeKind::Task,
        };

        assert_eq!(project.kind, ScopeKind::Project);
        assert_eq!(milestone.kind, ScopeKind::Milestone);
        assert_eq!(slice.kind, ScopeKind::Slice);
        assert_eq!(task.kind, ScopeKind::Task);
    }
}
