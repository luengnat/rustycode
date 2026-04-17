//! Orchestra State Derivation
//!
//! Reconstructs the complete Orchestra project state by parsing files on disk.
//! This is the **source of truth** — `STATE.md` is just a cached snapshot.
//!
//! # State Hierarchy
//!
//! Orchestra organizes work in a three-level hierarchy:
//!
//! ```
//! Milestones (M01, M02, ...)
//!   └─ Slices (S01, S02, ...)
//!       └─ Tasks (T01, T02, ...)
//! ```
//!
//! # Derivation Algorithm
//!
//! 1. Scan `.orchestra/milestones/*/` for ROADMAP.md files
//! 2. Parse each milestone to find incomplete slices
//! 3. For each incomplete slice, parse PLAN.md for tasks
//! 4. Return the **first incomplete task** at the deepest level
//!
//! # Finding the Active Task
//!
//! The algorithm prioritizes depth over breadth:
//! - Complete M01/S01/T01 before M01/S01/T02
//! - Complete M01/S01 before M01/S02
//! - Complete M01 before M02
//!
//! # Caching
//!
//! Derived state is cached to `STATE.md` for fast reads without
//! file parsing. Call `write_state_cache()` to update the cache.
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::state_derivation::StateDeriver;
//!
//! let deriver = StateDeriver::new(project_root);
//! let state = deriver.derive()?;
//!
//! match state.active_task {
//!     Some(task) => println!("Executing: {}", task.id),
//!     None => println!("All tasks complete!"),
//! }
//! ```
//!
//! # Error Handling
//!
//! State derivation is **fault-tolerant**:
//! - Missing milestones are skipped
//! - Malformed ROADMAP.md/PLAN.md files log warnings
//! - Empty projects return `Ok` with no active task
//!
//! Only critical errors (permissions, disk full) return `Err`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;
use walkdir::WalkDir;

// Import Phase from phases module
use crate::phases::Phase;

/// Orchestra state derived from files on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestraState {
    /// Active milestone ID (e.g., "M01")
    pub active_milestone: Option<MilestoneRef>,
    /// Active slice ID (e.g., "S01")
    pub active_slice: Option<SliceRef>,
    /// Active task ID (e.g., "T01")
    pub active_task: Option<TaskRef>,
    /// All milestones
    pub milestones: Vec<MilestoneState>,
    /// Current phase (like orchestra-2)
    #[serde(default = "default_phase")]
    pub phase: Phase,
}

/// Default phase is executing
fn default_phase() -> Phase {
    Phase::Execute
}

/// Milestone reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneRef {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
}

/// Slice reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceRef {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
}

/// Task reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRef {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub done: bool,
}

/// Milestone state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneState {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub complete: bool,
    pub slices: Vec<SliceState>,
}

/// Slice state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceState {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub done: bool,
    pub tasks: Vec<TaskState>,
}

/// Task state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub done: bool,
    pub has_plan: bool,
    pub has_summary: bool,
}

/// Roadmap structure (from ROADMAP.md)
#[derive(Debug, Clone)]
struct Roadmap {
    slices: Vec<RoadmapSlice>,
}

/// Slice in roadmap
#[derive(Debug, Clone)]
struct RoadmapSlice {
    id: String,
    #[allow(dead_code)] // Kept for future use
    title: String, // Reserved for future display purposes
    done: bool,
}

/// Plan structure (from PLAN.md)
#[derive(Debug, Clone)]
struct SlicePlan {
    tasks: Vec<PlanTask>,
}

/// Task in plan
#[derive(Debug, Clone)]
struct PlanTask {
    id: String,
    title: String,
    done: bool,
}

/// State deriver
pub struct StateDeriver {
    project_root: PathBuf,
}

impl StateDeriver {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Derive state from files on disk
    pub fn derive_state(&self) -> Result<OrchestraState> {
        let milestones_dir = self.project_root.join(".orchestra/milestones");

        if !milestones_dir.exists() {
            return Ok(OrchestraState {
                active_milestone: None,
                active_slice: None,
                active_task: None,
                milestones: Vec::new(),
                phase: Phase::Research,
            });
        }

        // Find all milestones
        let mut milestones = Vec::new();
        for entry in WalkDir::new(&milestones_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                if let Some(milestone_state) = self.load_milestone(path)? {
                    milestones.push(milestone_state);
                }
            }
        }

        // Sort by ID
        milestones.sort_by(|a, b| a.id.cmp(&b.id));

        debug!("Total milestones loaded: {}", milestones.len());
        for m in &milestones {
            debug!(
                "  Milestone {} complete={} slices={}",
                m.id,
                m.complete,
                m.slices.len()
            );
        }

        // Find active milestone (first incomplete)
        let active_milestone = milestones.iter().find(|m| !m.complete).map(|m| {
            debug!("Found active milestone: {}", m.id);
            MilestoneRef {
                id: m.id.clone(),
                title: m.title.clone(),
                path: m.path.clone(),
            }
        });

        debug!(
            "Active milestone: {:?}",
            active_milestone.as_ref().map(|m| &m.id)
        );

        // Find active slice
        let active_slice = if let Some(ref am) = active_milestone {
            milestones
                .iter()
                .find(|m| m.id == am.id)
                .and_then(|m| m.slices.iter().find(|s| !s.done))
                .map(|s| SliceRef {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    path: s.path.clone(),
                })
        } else {
            None
        };

        // Find active task
        let active_task = if let Some(ref aslice) = active_slice {
            debug!("Looking for active task in slice: {}", aslice.id);
            let result = milestones
                .iter()
                .find(|m| {
                    m.id == active_milestone
                        .as_ref()
                        .map(|am| am.id.clone())
                        .unwrap_or_default()
                })
                .and_then(|m| {
                    debug!("Found milestone: {}", m.id);
                    debug!(
                        "  Slices: {:?}",
                        m.slices
                            .iter()
                            .map(|s| (&s.id, s.done, s.tasks.len()))
                            .collect::<Vec<_>>()
                    );
                    m.slices.iter().find(|s| s.id == aslice.id)
                })
                .and_then(|s| {
                    debug!("Found slice: {} with {} tasks", s.id, s.tasks.len());
                    debug!(
                        "  Tasks: {:?}",
                        s.tasks.iter().map(|t| (&t.id, t.done)).collect::<Vec<_>>()
                    );
                    s.tasks.iter().find(|t| !t.done)
                })
                .map(|t| {
                    debug!("Found active task: {}", t.id);
                    TaskRef {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        path: t.path.clone(),
                        done: t.done,
                    }
                });
            result
        } else {
            debug!("No active slice, so no active task");
            None
        };

        // Determine phase (like orchestra-2)
        let phase = if milestones.is_empty() {
            Phase::Research // No milestones yet
        } else if let Some(ref task) = active_task {
            // Check if slice has PLAN.md
            let plan_path = if let Some(ref slice) = active_slice {
                slice.path.join("PLAN.md")
            } else {
                PathBuf::new()
            };

            if !plan_path.exists() {
                Phase::Plan
            } else if task.done {
                // Task is done, check if all tasks in slice are done
                let all_done = if let Some(ref am) = active_milestone {
                    milestones
                        .iter()
                        .find(|m| m.id == am.id)
                        .and_then(|m| {
                            m.slices.iter().find(|s| {
                                s.id == active_slice
                                    .as_ref()
                                    .map(|s| s.id.clone())
                                    .unwrap_or_default()
                            })
                        })
                        .map(|s| s.tasks.iter().all(|t| t.done))
                        .unwrap_or(false)
                } else {
                    false
                };

                if all_done {
                    Phase::Complete // All tasks in slice done
                } else {
                    Phase::Execute
                }
            } else {
                Phase::Execute
            }
        } else if active_slice.is_some() {
            // Has slice but no active task
            // Check if slice has PLAN.md
            let plan_path = active_slice
                .as_ref()
                .map(|s| s.path.join("PLAN.md"))
                .unwrap_or(PathBuf::new());
            if !plan_path.exists() {
                Phase::Plan // Slice needs planning
            } else {
                // Slice has plan but no tasks or all tasks done
                Phase::Complete
            }
        } else if active_milestone.is_some() {
            // Has milestone but no active slice
            Phase::Validate
        } else {
            Phase::Validate // All done
        };

        Ok(OrchestraState {
            active_milestone,
            active_slice,
            active_task,
            milestones,
            phase,
        })
    }

    /// Load milestone state from disk
    fn load_milestone(&self, milestone_path: &Path) -> Result<Option<MilestoneState>> {
        let id = milestone_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Read ROADMAP.md
        let roadmap_path = milestone_path.join("ROADMAP.md");
        let roadmap = if roadmap_path.exists() {
            self.parse_roadmap(&roadmap_path)?
        } else {
            Roadmap { slices: Vec::new() }
        };

        // Check if milestone is complete
        let complete = !roadmap.slices.is_empty() && roadmap.slices.iter().all(|s| s.done);
        debug!(
            "Milestone {} complete: {} ({} slices)",
            id,
            complete,
            roadmap.slices.len()
        );
        for s in &roadmap.slices {
            debug!("  Slice {} done={}", s.id, s.done);
        }

        // Load slices
        let mut slices = Vec::new();
        for roadmap_slice in &roadmap.slices {
            // Try both paths: milestone_path/S01 and milestone_path/slices/S01
            let direct_path = milestone_path.join(&roadmap_slice.id);
            let slices_subdir_path = milestone_path.join("slices").join(&roadmap_slice.id);

            let slice_path = if slices_subdir_path.exists() {
                slices_subdir_path
            } else {
                direct_path
            };

            if let Some(slice_state) = self.load_slice(&slice_path, &roadmap_slice.id)? {
                slices.push(slice_state);
            }
        }

        // If no roadmap, look for slices directly
        if roadmap.slices.is_empty() {
            for entry in WalkDir::new(milestone_path)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_dir() {
                    let slice_id = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    // Skip common non-slice directories
                    if slice_id == "slices" || slice_id == "tasks" || slice_id.starts_with('.') {
                        continue;
                    }

                    if let Some(slice_state) = self.load_slice(path, &slice_id)? {
                        slices.push(slice_state);
                    }
                }
            }
        }

        // Sort slices by ID
        slices.sort_by(|a, b| a.id.cmp(&b.id));

        let title = format!("Milestone {}", id);

        Ok(Some(MilestoneState {
            id,
            title,
            path: milestone_path.to_path_buf(),
            complete,
            slices,
        }))
    }

    /// Load slice state from disk
    fn load_slice(&self, slice_path: &Path, slice_id: &str) -> Result<Option<SliceState>> {
        // Read PLAN.md
        let plan_path = slice_path.join("PLAN.md");
        debug!("Loading slice from: {:?}", slice_path);
        debug!("Looking for PLAN.md at: {:?}", plan_path);
        debug!("PLAN.md exists: {}", plan_path.exists());
        let plan = if plan_path.exists() {
            self.parse_plan(&plan_path)?
        } else {
            SlicePlan { tasks: Vec::new() }
        };

        // Check if slice is complete in ROADMAP.md (not just in PLAN.md)
        // A slice is only truly done if it's marked as done in ROADMAP.md
        let milestone_path = slice_path
            .parent()
            .and_then(|parent| {
                if parent.file_name().and_then(|n| n.to_str()) == Some("slices") {
                    parent.parent()
                } else {
                    Some(parent)
                }
            })
            .unwrap_or(slice_path);
        let roadmap_path = milestone_path.join("ROADMAP.md");
        let roadmap_done = if roadmap_path.exists() {
            let roadmap_content = std::fs::read_to_string(&roadmap_path)?;
            roadmap_content.contains(&format!("- [x] {}:", slice_id))
        } else {
            false
        };

        // Check if all tasks in the plan are done
        let all_tasks_done = !plan.tasks.is_empty() && plan.tasks.iter().all(|t| t.done);

        // Slice is done if marked in ROADMAP OR if all tasks are done
        let done = roadmap_done || all_tasks_done;

        // Load tasks
        let mut tasks = Vec::new();
        for plan_task in &plan.tasks {
            let task_path = slice_path.join("tasks").join(&plan_task.id);
            let has_plan = task_path.join(format!("{}-PLAN.md", plan_task.id)).exists();
            let has_summary = task_path
                .join(format!("{}-SUMMARY.md", plan_task.id))
                .exists();

            debug!(
                "Loading task: {} has_plan={} has_summary={}",
                plan_task.id, has_plan, has_summary
            );
            tasks.push(TaskState {
                id: plan_task.id.clone(),
                title: plan_task.title.clone(),
                path: task_path,
                done: plan_task.done,
                has_plan,
                has_summary,
            });
        }

        debug!("Total tasks loaded: {}", tasks.len());

        // If no plan, look for tasks directly
        if plan.tasks.is_empty() {
            let tasks_dir = slice_path.join("tasks");
            if tasks_dir.exists() {
                for entry in WalkDir::new(&tasks_dir)
                    .min_depth(1)
                    .max_depth(1)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let path = entry.path();
                    if path.is_dir() {
                        let task_id = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let has_plan = path.join(format!("{}-PLAN.md", task_id)).exists();
                        let has_summary = path.join(format!("{}-SUMMARY.md", task_id)).exists();

                        tasks.push(TaskState {
                            id: task_id.clone(),
                            title: format!("Task {}", task_id),
                            path: path.to_path_buf(),
                            done: has_summary,
                            has_plan,
                            has_summary,
                        });
                    }
                }
            }
        }

        // Sort tasks by ID
        tasks.sort_by(|a, b| a.id.cmp(&b.id));

        let title = format!("Slice {}", slice_id);

        Ok(Some(SliceState {
            id: slice_id.to_string(),
            title,
            path: slice_path.to_path_buf(),
            done,
            tasks,
        }))
    }

    /// Parse ROADMAP.md
    fn parse_roadmap(&self, path: &Path) -> Result<Roadmap> {
        let content =
            std::fs::read_to_string(path).context(format!("Failed to read roadmap: {:?}", path))?;

        let mut slices = Vec::new();

        debug!("Parsing roadmap: {:?}", path);
        for line in content.lines() {
            // Look for "- [x] S01: Title" or "- [ ] S01: Title"
            if let Some(rest) = line.strip_prefix("- [") {
                let done = rest.starts_with('x');
                // Trim the space before the bracket
                let rest = rest.trim_start();
                if let Some(slice_line) =
                    rest.strip_prefix("x] ").or_else(|| rest.strip_prefix("] "))
                {
                    let parts: Vec<&str> = slice_line.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let id = parts[0].trim().to_string();
                        let title = parts[1].trim().to_string();
                        debug!("  Found slice: {} ({}) done={}", id, title, done);
                        slices.push(RoadmapSlice { id, title, done });
                    }
                }
            }
        }

        debug!("  Total slices parsed: {}", slices.len());
        Ok(Roadmap { slices })
    }

    /// Parse PLAN.md
    fn parse_plan(&self, path: &Path) -> Result<SlicePlan> {
        let content =
            std::fs::read_to_string(path).context(format!("Failed to read plan: {:?}", path))?;

        debug!("Parsing plan: {:?}", path);
        debug!("Plan content:\n{}", content);
        let mut tasks = Vec::new();

        for line in content.lines() {
            // Format 1: "- [x] T01: Title" or "- [ ] T01: Title"
            if let Some(rest) = line.strip_prefix("- [") {
                let rest = rest.trim_start();
                let done = rest.starts_with('x');
                // Strip the 'x' if done, or skip to content
                let rest = if done {
                    rest.strip_prefix('x').unwrap_or(rest)
                } else {
                    rest
                };

                // Try format 1: "] T01: Title" or "] **T01: Title**" (note: we already stripped 'x' if done)
                if let Some(task_line) = rest.strip_prefix("] ") {
                    // Strip markdown bold markers if present
                    let task_line_cleaned = task_line.replace("**", "");
                    let parts: Vec<&str> = task_line_cleaned.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let id = parts[0].trim().to_string();
                        let title = parts[1].trim().to_string();
                        debug!("  Found task (format 1): {} ({}) done={}", id, title, done);
                        tasks.push(PlanTask { id, title, done });
                        continue;
                    }
                }

                // Format 2: "- [T01](./tasks/T01-PLAN.md): Title" or "- [x][T01](./tasks/T01-PLAN.md): Title"
                // Look for "[T01](" pattern
                if let Some(link_start) = rest.find('[') {
                    let after_bracket = &rest[link_start + 1..]; // Skip '['
                    if let Some(link_end) = after_bracket.find(']') {
                        let id = &after_bracket[..link_end]; // "T01"
                                                             // Skip past "](" to find the end of the URL, then extract title after "): "
                        let rest_after_link = &after_bracket[link_end..];
                        if let Some(url_end) = rest_after_link.find("): ") {
                            let title = rest_after_link[url_end + 3..].trim().to_string();
                            debug!("  Found task (format 2): {} ({}) done={}", id, title, done);
                            tasks.push(PlanTask {
                                id: id.to_string(),
                                title,
                                done,
                            });
                            continue;
                        }
                    }
                }
            }
        }

        Ok(SlicePlan { tasks })
    }

    /// Write STATE.md cache
    pub fn write_state_cache(&self, state: &OrchestraState) -> Result<()> {
        let state_path = self.project_root.join(".orchestra/STATE.md");

        let mut content = String::from("# Orchestra State\n\n");

        if let Some(ref am) = state.active_milestone {
            content.push_str(&format!("**Active Milestone:** {}: {}\n", am.id, am.title));
        }

        if let Some(ref aslice) = state.active_slice {
            content.push_str(&format!(
                "**Active Slice:** {}: {}\n",
                aslice.id, aslice.title
            ));
        }

        if let Some(ref atask) = state.active_task {
            content.push_str(&format!("**Active Task:** {}: {}\n", atask.id, atask.title));
            content.push_str(&format!("**Next Action:** Execute {}\n", atask.id));
        }

        content.push_str(&format!(
            "\n**Last Updated:** {}\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));

        std::fs::write(&state_path, content)
            .context(format!("Failed to write STATE.md: {:?}", state_path))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_state_derivation() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create milestone structure
        let milestone_dir = project_root.join(".orchestra/milestones/M01");
        std::fs::create_dir_all(milestone_dir.join("slices/S01/tasks")).unwrap();

        // Create ROADMAP.md
        let roadmap = r#"# Milestone M01

## Slices
- [ ] S01: First slice
- [ ] S02: Second slice
"#;
        std::fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        // Create PLAN.md
        let plan = r#"# Slice S01

## Tasks
- [ ] T01: First task
- [ ] T02: Second task
"#;
        std::fs::write(milestone_dir.join("slices/S01/PLAN.md"), plan).unwrap();

        // Derive state
        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().unwrap();

        // Debug: print what we found
        println!("Milestones: {:?}", state.milestones.len());
        if !state.milestones.is_empty() {
            let m = &state.milestones[0];
            println!("  Milestone {}: {} slices", m.id, m.slices.len());
            if !m.slices.is_empty() {
                let s = &m.slices[0];
                println!(
                    "    Slice {}: {} tasks, done={}",
                    s.id,
                    s.tasks.len(),
                    s.done
                );
            }
        }

        // Verify
        assert_eq!(
            state.active_milestone.as_ref().map(|m| m.id.as_str()),
            Some("M01")
        );
        assert_eq!(
            state.active_slice.as_ref().map(|s| s.id.as_str()),
            Some("S01")
        );
        assert_eq!(
            state.active_task.as_ref().map(|t| t.id.as_str()),
            Some("T01")
        );
    }

    // --- Serde roundtrip tests ---

    #[test]
    fn orchestra_state_serde_roundtrip() {
        let state = OrchestraState {
            active_milestone: Some(MilestoneRef {
                id: "M01".into(),
                title: "Milestone M01".into(),
                path: PathBuf::from("/tmp/M01"),
            }),
            active_slice: None,
            active_task: None,
            milestones: vec![],
            phase: Phase::Research,
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: OrchestraState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.active_milestone.as_ref().unwrap().id, "M01");
        assert_eq!(decoded.phase, Phase::Research);
    }

    #[test]
    fn orchestra_state_empty_serde() {
        let state = OrchestraState {
            active_milestone: None,
            active_slice: None,
            active_task: None,
            milestones: vec![],
            phase: Phase::Validate,
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: OrchestraState = serde_json::from_str(&json).unwrap();
        assert!(decoded.active_milestone.is_none());
        assert!(decoded.milestones.is_empty());
    }

    #[test]
    fn milestone_ref_serde() {
        let mr = MilestoneRef {
            id: "M02".into(),
            title: "Second".into(),
            path: PathBuf::from("/proj/.orchestra/milestones/M02"),
        };
        let json = serde_json::to_string(&mr).unwrap();
        let decoded: MilestoneRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "M02");
        assert_eq!(decoded.title, "Second");
    }

    #[test]
    fn slice_ref_serde() {
        let sr = SliceRef {
            id: "S01".into(),
            title: "Core".into(),
            path: PathBuf::from("/proj/.orchestra/milestones/M01/slices/S01"),
        };
        let json = serde_json::to_string(&sr).unwrap();
        let decoded: SliceRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "S01");
    }

    #[test]
    fn task_ref_serde() {
        let tr = TaskRef {
            id: "T01".into(),
            title: "Setup".into(),
            path: PathBuf::from("/proj/.orchestra/milestones/M01/slices/S01/tasks/T01"),
            done: false,
        };
        let json = serde_json::to_string(&tr).unwrap();
        let decoded: TaskRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "T01");
        assert!(!decoded.done);
    }

    #[test]
    fn task_ref_done_serde() {
        let tr = TaskRef {
            id: "T03".into(),
            title: "Done task".into(),
            path: PathBuf::from("/proj/T03"),
            done: true,
        };
        let json = serde_json::to_string(&tr).unwrap();
        let decoded: TaskRef = serde_json::from_str(&json).unwrap();
        assert!(decoded.done);
    }

    #[test]
    fn milestone_state_serde() {
        let ms = MilestoneState {
            id: "M01".into(),
            title: "Milestone M01".into(),
            path: PathBuf::from("/proj/M01"),
            complete: false,
            slices: vec![SliceState {
                id: "S01".into(),
                title: "Slice S01".into(),
                path: PathBuf::from("/proj/M01/S01"),
                done: false,
                tasks: vec![TaskState {
                    id: "T01".into(),
                    title: "Task T01".into(),
                    path: PathBuf::from("/proj/M01/S01/tasks/T01"),
                    done: false,
                    has_plan: true,
                    has_summary: false,
                }],
            }],
        };
        let json = serde_json::to_string(&ms).unwrap();
        let decoded: MilestoneState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "M01");
        assert!(!decoded.complete);
        assert_eq!(decoded.slices.len(), 1);
        assert_eq!(decoded.slices[0].tasks.len(), 1);
        assert!(decoded.slices[0].tasks[0].has_plan);
    }

    #[test]
    fn slice_state_serde() {
        let ss = SliceState {
            id: "S02".into(),
            title: "Slice S02".into(),
            path: PathBuf::from("/proj/S02"),
            done: true,
            tasks: vec![],
        };
        let json = serde_json::to_string(&ss).unwrap();
        let decoded: SliceState = serde_json::from_str(&json).unwrap();
        assert!(decoded.done);
        assert!(decoded.tasks.is_empty());
    }

    #[test]
    fn task_state_serde() {
        let ts = TaskState {
            id: "T02".into(),
            title: "Write tests".into(),
            path: PathBuf::from("/proj/T02"),
            done: true,
            has_plan: true,
            has_summary: true,
        };
        let json = serde_json::to_string(&ts).unwrap();
        let decoded: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "T02");
        assert!(decoded.has_plan);
        assert!(decoded.has_summary);
    }

    // --- StateDeriver construction ---

    #[test]
    fn state_deriver_new() {
        let deriver = StateDeriver::new(PathBuf::from("/tmp/nonexistent"));
        let _ = deriver;
    }

    #[test]
    fn derive_state_empty_project() {
        let temp_dir = TempDir::new().unwrap();
        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        let state = deriver.derive_state().unwrap();
        assert!(state.active_milestone.is_none());
        assert!(state.active_slice.is_none());
        assert!(state.active_task.is_none());
        assert!(state.milestones.is_empty());
    }

    #[test]
    fn derive_state_no_milestones_dir() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra");
        std::fs::create_dir_all(&orchestra_dir).unwrap();
        // No milestones subdirectory
        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        let state = deriver.derive_state().unwrap();
        assert!(state.milestones.is_empty());
    }

    #[test]
    fn derive_state_complete_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let milestone_dir = temp_dir.path().join(".orchestra/milestones/M01");
        std::fs::create_dir_all(milestone_dir.join("slices/S01/tasks")).unwrap();

        let roadmap = "- [x] S01: Done slice\n";
        std::fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        let plan = "- [x] T01: Done task\n";
        std::fs::write(milestone_dir.join("slices/S01/PLAN.md"), plan).unwrap();

        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        let state = deriver.derive_state().unwrap();

        assert_eq!(state.milestones.len(), 1);
        assert!(state.milestones[0].complete);
        // No active milestone since M01 is complete
        assert!(state.active_milestone.is_none());
    }

    #[test]
    fn write_state_cache_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".orchestra")).unwrap();

        let state = OrchestraState {
            active_milestone: Some(MilestoneRef {
                id: "M01".into(),
                title: "Milestone M01".into(),
                path: PathBuf::from("/tmp"),
            }),
            active_slice: None,
            active_task: None,
            milestones: vec![],
            phase: Phase::Execute,
        };

        let deriver = StateDeriver::new(temp_dir.path().to_path_buf());
        deriver.write_state_cache(&state).unwrap();

        let content = std::fs::read_to_string(temp_dir.path().join(".orchestra/STATE.md")).unwrap();
        assert!(content.contains("Active Milestone:** M01:"));
        assert!(content.contains("Milestone M01"));
    }

    #[test]
    fn orchestra_state_default_phase_is_execute() {
        let json =
            r#"{"active_milestone":null,"active_slice":null,"active_task":null,"milestones":[]}"#;
        let decoded: OrchestraState = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.phase, Phase::Execute);
    }
}
