// rustycode-orchestra/src/state.rs
//! State management for Orchestra v2
//!
//! Provides persistent state that survives context resets

use crate::error::{OrchestraV2Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// State manager for Orchestra v2
pub struct StateManager {
    /// Project root directory
    project_root: PathBuf,
}

impl StateManager {
    /// Create a new state manager
    pub fn new<P: AsRef<Path>>(project_root: P) -> Result<Self> {
        let project_root = project_root.as_ref().to_path_buf();
        let orchestra_dir = project_root.join(".orchestra");

        // Ensure .orchestra directory exists
        fs::create_dir_all(&orchestra_dir).map_err(OrchestraV2Error::Io)?;

        Ok(Self { project_root })
    }

    /// Read current state
    pub fn read_state(&self) -> Result<OrchestraState> {
        let state_path = self.project_root.join(".orchestra/STATE.md");

        if !state_path.exists() {
            return Ok(OrchestraState::default());
        }

        let content = fs::read_to_string(&state_path).map_err(OrchestraV2Error::Io)?;

        // Parse frontmatter
        let (frontmatter, _body) = content
            .split_once("\n---\n")
            .ok_or_else(|| OrchestraV2Error::Parse("Missing frontmatter separator".to_string()))?;

        // Parse metadata from frontmatter
        let state_meta: StateMetadata = serde_yaml::from_str(frontmatter)
            .map_err(|e| OrchestraV2Error::Parse(format!("Invalid state metadata: {}", e)))?;

        // Parse body sections
        let project = Self::parse_project_section(&content)?;
        let execution = Self::parse_execution_section(&content)?;
        let debug = Self::parse_debug_section(&content)?;

        Ok(OrchestraState {
            project,
            milestone: state_meta.milestone,
            execution,
            debug,
        })
    }

    /// Write state to disk atomically
    pub fn write_state(&self, state: &OrchestraState) -> Result<()> {
        let state_path = self.project_root.join(".orchestra/STATE.md");

        let mut content = String::new();

        // Write frontmatter
        content.insert_str(
            0,
            &format!(
                r#"---
updated_at: {}
milestone: {}
version: {}
---
"#,
                Utc::now().to_rfc3339(),
                state.milestone,
                crate::VERSION
            ),
        );

        // Write project section
        content.push_str(&Self::format_project_section(&state.project));

        // Write execution section
        content.push_str(&Self::format_execution_section(&state.execution));

        // Write debug section
        content.push_str(&Self::format_debug_section(&state.debug));

        // Write atomically to prevent corruption on crash
        crate::atomic_write::atomic_write(&state_path, &content)
            .map_err(|e| OrchestraV2Error::InvalidState(format!("Failed to write state: {}", e)))
    }

    /// Parse project section from STATE.md
    fn parse_project_section(content: &str) -> Result<ProjectState> {
        // Extract project section between "## Project" and next "##"
        let project_section = Self::extract_section(content, "Project").unwrap_or_default();

        // Parse name and vision
        let name =
            Self::extract_field(&project_section, "Name").unwrap_or_else(|| "Unknown".to_string());
        let vision = Self::extract_field(&project_section, "Vision").unwrap_or_default();

        Ok(ProjectState {
            name,
            vision,
            requirements: Vec::new(),
            decisions: Vec::new(),
        })
    }

    /// Parse execution section from STATE.md
    fn parse_execution_section(content: &str) -> Result<ExecutionState> {
        let execution_section = Self::extract_section(content, "Execution").unwrap_or_default();

        let active_phase = Self::extract_field(&execution_section, "Active Phase");
        let active_wave =
            Self::extract_field(&execution_section, "Active Wave").and_then(|s| s.parse().ok());
        let active_task = Self::extract_field(&execution_section, "Active Task");

        Ok(ExecutionState {
            active_phase,
            active_wave,
            active_task,
            paused_at: None,
            checkpoints: Vec::new(),
        })
    }

    /// Parse debug section from STATE.md
    fn parse_debug_section(_content: &str) -> Result<DebugState> {
        Ok(DebugState {
            active_sessions: Vec::new(),
            resolved_sessions: Vec::new(),
        })
    }

    /// Format project section
    fn format_project_section(project: &ProjectState) -> String {
        format!(
            r#"
## Project

**Name:** {}
**Vision:** {}

### Requirements
{}

### Decisions
{}
"#,
            project.name,
            project.vision,
            if project.requirements.is_empty() {
                "None".to_string()
            } else {
                project
                    .requirements
                    .iter()
                    .map(|r| format!("- {}", r))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            if project.decisions.is_empty() {
                "None".to_string()
            } else {
                project
                    .decisions
                    .iter()
                    .map(|d| format!("- {}", d))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        )
    }

    /// Format execution section
    fn format_execution_section(execution: &ExecutionState) -> String {
        format!(
            r#"
## Execution

**Active Phase:** {}
**Active Wave:** {}
**Active Task:** {}

### Checkpoints
{}
"#,
            execution.active_phase.as_deref().unwrap_or("None"),
            execution
                .active_wave
                .map(|w| w.to_string())
                .unwrap_or_else(|| "None".to_string()),
            execution.active_task.as_deref().unwrap_or("None"),
            if execution.checkpoints.is_empty() {
                "None".to_string()
            } else {
                execution
                    .checkpoints
                    .iter()
                    .map(|c| format!("- {}", c))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        )
    }

    /// Format debug section
    fn format_debug_section(debug: &DebugState) -> String {
        format!(
            r#"
## Debug

**Active Sessions:** {}
**Resolved Sessions:** {}
"#,
            debug.active_sessions.len(),
            debug.resolved_sessions.len()
        )
    }

    /// Extract a section from markdown content
    fn extract_section(content: &str, section_name: &str) -> Option<String> {
        let pattern = format!("## {}", section_name);
        let start = content.find(&pattern)?;
        let end = content[start + pattern.len()..]
            .find("## ")
            .map(|e| start + pattern.len() + e)
            .unwrap_or(content.len());

        Some(content[start + pattern.len()..end].trim().to_string())
    }

    /// Extract a field value from section content
    fn extract_field(section: &str, field_name: &str) -> Option<String> {
        let pattern = format!("**{}:** ", field_name);
        let start = section.find(&pattern)?;
        let end = section[start + pattern.len()..]
            .find('\n')
            .map(|e| start + pattern.len() + e)
            .unwrap_or(section.len());

        Some(section[start + pattern.len()..end].trim().to_string())
    }
}

/// Orchestra v2 state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestraState {
    /// Project state
    pub project: ProjectState,
    /// Milestone identifier
    pub milestone: String,
    /// Execution state
    pub execution: ExecutionState,
    /// Debug state
    pub debug: DebugState,
}

impl Default for OrchestraState {
    fn default() -> Self {
        Self {
            project: ProjectState::default(),
            milestone: "M001".to_string(),
            execution: ExecutionState::default(),
            debug: DebugState::default(),
        }
    }
}

/// Project state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    /// Project name
    pub name: String,
    /// Project vision
    pub vision: String,
    /// Requirements
    pub requirements: Vec<String>,
    /// Decisions made
    pub decisions: Vec<String>,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self {
            name: "Unknown".to_string(),
            vision: String::new(),
            requirements: Vec::new(),
            decisions: Vec::new(),
        }
    }
}

/// Execution state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionState {
    /// Active phase ID
    pub active_phase: Option<String>,
    /// Active wave index
    pub active_wave: Option<usize>,
    /// Active task ID
    pub active_task: Option<String>,
    /// When work was paused
    pub paused_at: Option<DateTime<Utc>>,
    /// Checkpoints reached
    pub checkpoints: Vec<String>,
}

/// Debug state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebugState {
    /// Active debug sessions
    pub active_sessions: Vec<DebugSession>,
    /// Resolved debug sessions
    pub resolved_sessions: Vec<DebugSession>,
}

/// Debug session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    /// Session ID
    pub id: String,
    /// Issue description
    pub issue: String,
    /// When session was created
    pub created_at: DateTime<Utc>,
}

/// State metadata from frontmatter
#[derive(Debug, Serialize, Deserialize)]
struct StateMetadata {
    updated_at: String,
    milestone: String,
    version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- OrchestraState ---

    #[test]
    fn orchestra_state_default() {
        let s = OrchestraState::default();
        assert_eq!(s.milestone, "M001");
        assert_eq!(s.project.name, "Unknown");
        assert!(s.project.vision.is_empty());
        assert!(s.execution.active_phase.is_none());
        assert!(s.debug.active_sessions.is_empty());
    }

    #[test]
    fn orchestra_state_serde_roundtrip() {
        let s = OrchestraState::default();
        let json = serde_json::to_string(&s).unwrap();
        let decoded: OrchestraState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.milestone, s.milestone);
        assert_eq!(decoded.project.name, s.project.name);
    }

    // --- ProjectState ---

    #[test]
    fn project_state_default() {
        let p = ProjectState::default();
        assert_eq!(p.name, "Unknown");
        assert!(p.vision.is_empty());
        assert!(p.requirements.is_empty());
        assert!(p.decisions.is_empty());
    }

    #[test]
    fn project_state_serde_roundtrip() {
        let p = ProjectState {
            name: "TestProject".into(),
            vision: "Build something".into(),
            requirements: vec!["Req1".into(), "Req2".into()],
            decisions: vec!["Use Rust".into()],
        };
        let json = serde_json::to_string(&p).unwrap();
        let decoded: ProjectState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "TestProject");
        assert_eq!(decoded.requirements.len(), 2);
        assert_eq!(decoded.decisions.len(), 1);
    }

    // --- ExecutionState ---

    #[test]
    fn execution_state_default() {
        let e = ExecutionState::default();
        assert!(e.active_phase.is_none());
        assert!(e.active_wave.is_none());
        assert!(e.active_task.is_none());
        assert!(e.paused_at.is_none());
        assert!(e.checkpoints.is_empty());
    }

    #[test]
    fn execution_state_serde_roundtrip() {
        let e = ExecutionState {
            active_phase: Some("P01".into()),
            active_wave: Some(2),
            active_task: Some("T03".into()),
            paused_at: None,
            checkpoints: vec!["cp1".into()],
        };
        let json = serde_json::to_string(&e).unwrap();
        let decoded: ExecutionState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.active_phase, Some("P01".into()));
        assert_eq!(decoded.active_wave, Some(2));
        assert_eq!(decoded.checkpoints.len(), 1);
    }

    // --- DebugState ---

    #[test]
    fn debug_state_default() {
        let d = DebugState::default();
        assert!(d.active_sessions.is_empty());
        assert!(d.resolved_sessions.is_empty());
    }

    #[test]
    fn debug_state_serde_roundtrip() {
        let d = DebugState {
            active_sessions: vec![DebugSession {
                id: "ds-1".into(),
                issue: "bug".into(),
                created_at: Utc::now(),
            }],
            resolved_sessions: vec![],
        };
        let json = serde_json::to_string(&d).unwrap();
        let decoded: DebugState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.active_sessions.len(), 1);
        assert_eq!(decoded.active_sessions[0].id, "ds-1");
    }

    // --- DebugSession ---

    #[test]
    fn debug_session_fields() {
        let ds = DebugSession {
            id: "abc".into(),
            issue: "crash on start".into(),
            created_at: Utc::now(),
        };
        assert_eq!(ds.id, "abc");
        assert_eq!(ds.issue, "crash on start");
    }

    // --- extract_section ---

    #[test]
    fn extract_section_found() {
        let md = "## Project\n\n**Name:** Foo\n\n## Execution\n\nstuff";
        let section = StateManager::extract_section(md, "Project").unwrap();
        assert!(section.contains("Foo"));
    }

    #[test]
    fn extract_section_not_found() {
        let md = "## Other\nstuff";
        assert!(StateManager::extract_section(md, "Project").is_none());
    }

    #[test]
    fn extract_section_last() {
        let md = "## Project\n\n**Name:** Foo";
        let section = StateManager::extract_section(md, "Project").unwrap();
        assert!(section.contains("Foo"));
    }

    // --- extract_field ---

    #[test]
    fn extract_field_found() {
        let section = "**Name:** MyProject\n**Vision:** Cool";
        assert_eq!(
            StateManager::extract_field(section, "Name"),
            Some("MyProject".into())
        );
        assert_eq!(
            StateManager::extract_field(section, "Vision"),
            Some("Cool".into())
        );
    }

    #[test]
    fn extract_field_not_found() {
        let section = "no fields here";
        assert!(StateManager::extract_field(section, "Name").is_none());
    }

    // --- format_project_section ---

    #[test]
    fn format_project_empty() {
        let p = ProjectState::default();
        let s = StateManager::format_project_section(&p);
        assert!(s.contains("Unknown"));
        assert!(s.contains("None"));
    }

    #[test]
    fn format_project_with_requirements() {
        let p = ProjectState {
            name: "Test".into(),
            vision: "V".into(),
            requirements: vec!["R1".into()],
            decisions: vec!["D1".into()],
        };
        let s = StateManager::format_project_section(&p);
        assert!(s.contains("- R1"));
        assert!(s.contains("- D1"));
    }

    // --- format_execution_section ---

    #[test]
    fn format_execution_defaults() {
        let e = ExecutionState::default();
        let s = StateManager::format_execution_section(&e);
        assert!(s.contains("None"));
    }

    #[test]
    fn format_execution_with_phase() {
        let e = ExecutionState {
            active_phase: Some("P02".into()),
            active_wave: Some(3),
            active_task: None,
            paused_at: None,
            checkpoints: vec!["cp1".into()],
        };
        let s = StateManager::format_execution_section(&e);
        assert!(s.contains("P02"));
        assert!(s.contains("3"));
        assert!(s.contains("- cp1"));
    }

    // --- format_debug_section ---

    #[test]
    fn format_debug_empty() {
        let d = DebugState::default();
        let s = StateManager::format_debug_section(&d);
        assert!(s.contains("0"));
    }

    // --- StateManager with tempdir ---

    #[test]
    fn state_manager_creates_orchestra_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let _sm = StateManager::new(tmp.path()).unwrap();
        assert!(tmp.path().join(".orchestra").exists());
    }

    #[test]
    fn read_state_default_when_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let sm = StateManager::new(tmp.path()).unwrap();
        let state = sm.read_state().unwrap();
        assert_eq!(state.milestone, "M001");
        assert_eq!(state.project.name, "Unknown");
    }

    #[test]
    fn write_and_read_state_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let sm = StateManager::new(tmp.path()).unwrap();

        let state = OrchestraState {
            project: ProjectState {
                name: "TestProj".into(),
                vision: "Build it".into(),
                requirements: vec![],
                decisions: vec![],
            },
            milestone: "M002".into(),
            execution: ExecutionState {
                active_phase: Some("P01".into()),
                active_wave: Some(1),
                active_task: None,
                paused_at: None,
                checkpoints: vec![],
            },
            debug: DebugState::default(),
        };

        sm.write_state(&state).unwrap();
        let loaded = sm.read_state().unwrap();
        assert_eq!(loaded.project.name, "TestProj");
        assert_eq!(loaded.milestone, "M002");
        assert_eq!(loaded.execution.active_phase, Some("P01".into()));
    }
}
