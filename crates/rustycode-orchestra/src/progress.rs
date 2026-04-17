//! Progress tracking for Orchestra v2
//!
//! Provides comprehensive progress tracking across milestones, phases, and tasks.
//! Calculates completion percentages and generates detailed progress reports.

use crate::error::{OrchestraV2Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Progress tracker for Orchestra v2
pub struct ProgressTracker {
    project_root: std::path::PathBuf,
    /// Cached milestone data
    milestones: HashMap<String, MilestoneProgress>,
}

impl ProgressTracker {
    /// Create a new progress tracker for a project
    pub fn new(project_root: std::path::PathBuf) -> Self {
        Self {
            project_root,
            milestones: HashMap::new(),
        }
    }

    /// Generate progress report for the project
    pub fn generate_report(&mut self) -> Result<ProgressReport> {
        // Load milestone data
        self.load_milestones()?;

        let total_phases: usize = self.milestones.values().map(|m| m.total_phases).sum();
        let completed_phases: usize = self.milestones.values().map(|m| m.completed_phases).sum();

        let progress_percent = if total_phases > 0 {
            (completed_phases as f32 / total_phases as f32) * 100.0
        } else {
            0.0
        };

        // Find current and next phases
        let (current_phase, next_phase) = self.find_current_and_next_phases()?;

        Ok(ProgressReport {
            total_phases,
            completed_phases,
            progress_percent,
            current_phase,
            next_phase,
        })
    }

    /// Get detailed status for a specific milestone
    pub fn milestone_status(&mut self, milestone_id: &str) -> Result<MilestoneProgress> {
        self.load_milestones()?;

        self.milestones.get(milestone_id).cloned().ok_or_else(|| {
            OrchestraV2Error::InvalidState(format!("Milestone {} not found", milestone_id))
        })
    }

    /// Load milestones from disk
    fn load_milestones(&mut self) -> Result<()> {
        let milestones_dir = self.project_root.join(".orchestra").join("milestones");

        if !milestones_dir.exists() {
            return Ok(());
        }

        // Read each milestone directory
        for entry in std::fs::read_dir(milestones_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let milestone_id = entry.file_name().to_string_lossy().to_string();

                // Load slices
                let slices_dir = entry.path().join("slices");
                if slices_dir.exists() {
                    let mut total_phases = 0;
                    let mut completed_phases = 0;
                    let mut phases = Vec::new();

                    for slice_entry in std::fs::read_dir(&slices_dir)? {
                        let slice_entry = slice_entry?;
                        if slice_entry.file_type()?.is_dir() {
                            let slice_id = slice_entry.file_name().to_string_lossy().to_string();
                            let plan_path = slice_entry.path().join("PLAN.md");

                            if plan_path.exists() {
                                total_phases += 1;

                                // Check if slice is completed
                                let is_completed = self.is_slice_completed(&plan_path)?;
                                if is_completed {
                                    completed_phases += 1;
                                }

                                phases.push(PhaseStatus {
                                    id: slice_id.clone(),
                                    title: self
                                        .extract_slice_title(&plan_path)
                                        .unwrap_or_else(|_| slice_id.clone()),
                                    completed: is_completed,
                                });
                            }
                        }
                    }

                    self.milestones.insert(
                        milestone_id.clone(),
                        MilestoneProgress {
                            id: milestone_id.clone(),
                            title: self
                                .extract_milestone_title(&entry.path())
                                .unwrap_or_else(|_| milestone_id.clone()),
                            total_phases,
                            completed_phases,
                            phases,
                        },
                    );
                }
            }
        }

        Ok(())
    }

    /// Check if a slice is completed
    fn is_slice_completed(&self, plan_path: &Path) -> Result<bool> {
        let content = std::fs::read_to_string(plan_path)?;

        // Check if all tasks are marked as done
        let in_tasks_section = content
            .lines()
            .skip_while(|line| !line.starts_with("## Tasks"))
            .skip(1)
            .take_while(|line| !line.starts_with("#") && !line.is_empty());

        let all_done = in_tasks_section
            .filter(|line| line.trim().starts_with("- ["))
            .all(|line| line.trim().starts_with("- [x]"));

        Ok(all_done)
    }

    /// Extract slice title from PLAN.md
    fn extract_slice_title(&self, plan_path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(plan_path)?;

        for line in content.lines() {
            if line.starts_with("# ") {
                return Ok(line.trim_start_matches("# ").trim().to_string());
            }
        }

        Ok("Unknown Phase".to_string())
    }

    /// Extract milestone title from milestone directory
    fn extract_milestone_title(&self, milestone_path: &Path) -> Result<String> {
        let vision_path = milestone_path.join("VISION.md");
        if vision_path.exists() {
            let content = std::fs::read_to_string(&vision_path)?;
            for line in content.lines() {
                if line.starts_with("# ") {
                    return Ok(line.trim_start_matches("# ").trim().to_string());
                }
            }
        }

        let roadmap_path = milestone_path.join("ROADMAP.md");
        if roadmap_path.exists() {
            let content = std::fs::read_to_string(&roadmap_path)?;
            for line in content.lines() {
                if line.starts_with("# ") {
                    return Ok(line.trim_start_matches("# ").trim().to_string());
                }
            }
        }

        Ok("Unknown Milestone".to_string())
    }

    /// Find current and next phases
    fn find_current_and_next_phases(&self) -> Result<(Option<String>, Option<String>)> {
        let mut current_phase = None;
        let mut next_phase = None;
        let mut found_current = false;

        // Iterate through milestones in order
        let mut milestone_ids: Vec<_> = self.milestones.keys().cloned().collect();
        milestone_ids.sort();

        for milestone_id in milestone_ids {
            let milestone = &self.milestones[&milestone_id];

            for phase in &milestone.phases {
                if found_current {
                    next_phase = Some(format!("{}: {}", milestone_id, phase.title));
                    break;
                }

                if !phase.completed {
                    current_phase = Some(format!("{}: {}", milestone_id, phase.title));
                    found_current = true;
                }
            }

            if next_phase.is_some() {
                break;
            }
        }

        Ok((current_phase, next_phase))
    }

    /// Get progress as a formatted string for display
    pub fn format_progress(&mut self) -> Result<String> {
        let report = self.generate_report()?;

        let mut output = String::new();
        output.push_str("📊 Orchestra Progress Report\n");
        output.push_str("═══════════════════\n\n");
        output.push_str(&format!(
            "Overall: {:.1}% complete\n",
            report.progress_percent
        ));
        output.push_str(&format!(
            "Phases: {}/{} completed\n\n",
            report.completed_phases, report.total_phases
        ));

        if let Some(current) = &report.current_phase {
            output.push_str(&format!("Current Phase:\n  {}\n", current));
        }

        if let Some(next) = &report.next_phase {
            output.push_str(&format!("Next Phase:\n  {}\n", next));
        }

        // Milestone breakdown
        output.push_str("\n📁 Milestone Breakdown:\n");
        let mut milestone_ids: Vec<_> = self.milestones.keys().cloned().collect();
        milestone_ids.sort();

        for milestone_id in milestone_ids {
            let milestone = &self.milestones[&milestone_id];
            let percent = if milestone.total_phases > 0 {
                (milestone.completed_phases as f32 / milestone.total_phases as f32) * 100.0
            } else {
                0.0
            };

            output.push_str(&format!(
                "\n  {}: {} - {:.0}%\n",
                milestone.id, milestone.title, percent
            ));

            for phase in &milestone.phases {
                let status = if phase.completed { "✅" } else { "🔄" };
                output.push_str(&format!("    {} {} {}\n", status, milestone.id, phase.id));
            }
        }

        Ok(output)
    }

    /// Clear cached milestone data
    pub fn clear_cache(&mut self) {
        self.milestones.clear();
    }
}

/// Milestone progress data
#[derive(Debug, Clone)]
pub struct MilestoneProgress {
    pub id: String,
    pub title: String,
    pub total_phases: usize,
    pub completed_phases: usize,
    pub phases: Vec<PhaseStatus>,
}

/// Phase/slice status
#[derive(Debug, Clone)]
pub struct PhaseStatus {
    pub id: String,
    pub title: String,
    pub completed: bool,
}

/// Progress report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressReport {
    pub total_phases: usize,
    pub completed_phases: usize,
    pub progress_percent: f32,
    pub current_phase: Option<String>,
    pub next_phase: Option<String>,
}

impl Default for ProgressReport {
    fn default() -> Self {
        Self {
            total_phases: 0,
            completed_phases: 0,
            progress_percent: 0.0,
            current_phase: None,
            next_phase: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_progress_report_empty_project() {
        let temp = TempDir::new().unwrap();
        let mut tracker = ProgressTracker::new(temp.path().to_path_buf());

        let report = tracker.generate_report().unwrap();

        assert_eq!(report.total_phases, 0);
        assert_eq!(report.completed_phases, 0);
        assert_eq!(report.progress_percent, 0.0);
    }

    #[test]
    fn test_progress_report_with_data() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        // Create milestone structure
        let orchestra_dir = project_root.join(".orchestra");
        let milestones_dir = orchestra_dir.join("milestones");
        let m1_dir = milestones_dir.join("M01");
        let slices_dir = m1_dir.join("slices");
        let s1_dir = slices_dir.join("S01");

        std::fs::create_dir_all(&s1_dir).unwrap();

        // Create PLAN.md with completed task
        let plan_content = r#"# Slice S01

## Goal
Test goal

## Demo
Test demo

## Tasks
- [x] T01: Completed task
"#;
        std::fs::write(s1_dir.join("PLAN.md"), plan_content).unwrap();

        let mut tracker = ProgressTracker::new(project_root.to_path_buf());
        let report = tracker.generate_report().unwrap();

        assert_eq!(report.total_phases, 1);
        assert_eq!(report.completed_phases, 1);
        assert_eq!(report.progress_percent, 100.0);
    }

    // --- Pure data type tests ---

    #[test]
    fn progress_report_default() {
        let report = ProgressReport::default();
        assert_eq!(report.total_phases, 0);
        assert_eq!(report.completed_phases, 0);
        assert_eq!(report.progress_percent, 0.0);
        assert!(report.current_phase.is_none());
        assert!(report.next_phase.is_none());
    }

    #[test]
    fn progress_report_serde_roundtrip() {
        let report = ProgressReport {
            total_phases: 10,
            completed_phases: 7,
            progress_percent: 70.0,
            current_phase: Some("M01: Phase 8".to_string()),
            next_phase: Some("M01: Phase 9".to_string()),
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: ProgressReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_phases, 10);
        assert_eq!(decoded.completed_phases, 7);
        assert!((decoded.progress_percent - 70.0).abs() < f32::EPSILON);
        assert_eq!(decoded.current_phase.unwrap(), "M01: Phase 8");
        assert_eq!(decoded.next_phase.unwrap(), "M01: Phase 9");
    }

    #[test]
    fn progress_report_serde_with_nones() {
        let report = ProgressReport {
            total_phases: 0,
            completed_phases: 0,
            progress_percent: 0.0,
            current_phase: None,
            next_phase: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: ProgressReport = serde_json::from_str(&json).unwrap();
        assert!(decoded.current_phase.is_none());
        assert!(decoded.next_phase.is_none());
    }

    #[test]
    fn phase_status_fields() {
        let phase = PhaseStatus {
            id: "S01".to_string(),
            title: "Setup project".to_string(),
            completed: true,
        };
        assert_eq!(phase.id, "S01");
        assert!(phase.completed);
    }

    #[test]
    fn milestone_progress_fields() {
        let milestone = MilestoneProgress {
            id: "M01".to_string(),
            title: "Foundation".to_string(),
            total_phases: 5,
            completed_phases: 3,
            phases: vec![
                PhaseStatus {
                    id: "S01".to_string(),
                    title: "Phase 1".to_string(),
                    completed: true,
                },
                PhaseStatus {
                    id: "S02".to_string(),
                    title: "Phase 2".to_string(),
                    completed: false,
                },
            ],
        };
        assert_eq!(milestone.id, "M01");
        assert_eq!(milestone.phases.len(), 2);
    }

    // --- Filesystem-based tests ---

    #[test]
    fn test_progress_with_incomplete_slice() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let slices_dir = project_root.join(".orchestra/milestones/M01/slices/S01");
        std::fs::create_dir_all(&slices_dir).unwrap();

        let plan_content = r#"# Slice S01

## Goal
Test goal

## Demo
Test demo

## Tasks
- [x] T01: Done task
- [ ] T02: Pending task
"#;
        std::fs::write(slices_dir.join("PLAN.md"), plan_content).unwrap();

        let mut tracker = ProgressTracker::new(project_root.to_path_buf());
        let report = tracker.generate_report().unwrap();

        assert_eq!(report.total_phases, 1);
        assert_eq!(report.completed_phases, 0);
        assert!((report.progress_percent - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_progress_with_multiple_milestones() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        // Milestone 1 - completed
        let s1 = project_root.join(".orchestra/milestones/M01/slices/S01");
        std::fs::create_dir_all(&s1).unwrap();
        std::fs::write(s1.join("PLAN.md"), "# S1\n\n## Tasks\n- [x] T01: Done\n").unwrap();

        // Milestone 2 - in progress
        let s2 = project_root.join(".orchestra/milestones/M02/slices/S01");
        std::fs::create_dir_all(&s2).unwrap();
        std::fs::write(s2.join("PLAN.md"), "# S2\n\n## Tasks\n- [ ] T01: Pending\n").unwrap();

        let mut tracker = ProgressTracker::new(project_root.to_path_buf());
        let report = tracker.generate_report().unwrap();

        assert_eq!(report.total_phases, 2);
        assert_eq!(report.completed_phases, 1);
    }

    #[test]
    fn test_milestone_status_not_found() {
        let temp = TempDir::new().unwrap();
        let mut tracker = ProgressTracker::new(temp.path().to_path_buf());
        let result = tracker.milestone_status("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_clear_cache() {
        let temp = TempDir::new().unwrap();
        let mut tracker = ProgressTracker::new(temp.path().to_path_buf());
        tracker.generate_report().unwrap();
        tracker.clear_cache();
        // After clearing, milestones should be empty (internal state)
        let report = tracker.generate_report().unwrap();
        assert_eq!(report.total_phases, 0);
    }

    #[test]
    fn test_format_progress_empty_project() {
        let temp = TempDir::new().unwrap();
        let mut tracker = ProgressTracker::new(temp.path().to_path_buf());
        let output = tracker.format_progress().unwrap();
        assert!(output.contains("Orchestra Progress Report"));
        assert!(output.contains("0.0%"));
    }

    #[test]
    fn test_progress_no_tasks_means_completed() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let s1 = project_root.join(".orchestra/milestones/M01/slices/S01");
        std::fs::create_dir_all(&s1).unwrap();
        // Plan with no tasks - all (zero) tasks are "done"
        std::fs::write(
            s1.join("PLAN.md"),
            "# S1\n\n## Goal\nDo stuff\n\n## Demo\nN/A\n\n## Tasks\n",
        )
        .unwrap();

        let mut tracker = ProgressTracker::new(project_root.to_path_buf());
        let report = tracker.generate_report().unwrap();

        assert_eq!(report.total_phases, 1);
        assert_eq!(report.completed_phases, 1);
    }
}
