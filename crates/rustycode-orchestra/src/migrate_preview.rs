// rustycode-orchestra/src/migrate_preview.rs
//! Orchestra Migration Preview — Pre-write statistics
//!
//! Pure function, no I/O. Computes counts from a OrchestraProject.
//!
//! Used to show the user what a migration will produce before writing anything.

use serde::{Deserialize, Serialize};

// ============================================================================
// Type Definitions
// ============================================================================

/// Migration preview statistics computed from a Orchestra project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationPreview {
    /// Number of milestones in the project
    pub milestone_count: usize,

    /// Total number of slices across all milestones
    pub total_slices: usize,

    /// Total number of tasks across all slices
    pub total_tasks: usize,

    /// Number of completed slices
    pub done_slices: usize,

    /// Number of completed tasks
    pub done_tasks: usize,

    /// Percentage of slices completed (0-100)
    pub slice_completion_pct: u8,

    /// Percentage of tasks completed (0-100)
    pub task_completion_pct: u8,

    /// Requirements counts by status
    pub requirements: RequirementCounts,
}

/// Requirement counts by status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequirementCounts {
    /// Active requirements
    pub active: usize,

    /// Validated requirements
    pub validated: usize,

    /// Deferred requirements
    pub deferred: usize,

    /// Out of scope requirements
    pub out_of_scope: usize,

    /// Total requirements
    pub total: usize,
}

/// Slice data for preview computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceData {
    /// Whether the slice is done
    pub done: bool,

    /// Tasks in this slice
    pub tasks: Vec<TaskData>,
}

/// Task data for preview computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskData {
    /// Whether the task is done
    pub done: bool,
}

/// Milestone data for preview computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneData {
    /// Slices in this milestone
    pub slices: Vec<SliceData>,
}

/// Orchestra project data for preview computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestraProject {
    /// Milestones in the project
    pub milestones: Vec<MilestoneData>,

    /// Requirements in the project
    pub requirements: Vec<RequirementData>,
}

/// Requirement data for preview computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementData {
    /// Requirement status
    pub status: String,
}

/// Compute pre-write statistics from a OrchestraProject without performing I/O.
///
/// Used to show the user what a migration will produce before writing anything.
///
/// # Arguments
/// * `project` - Orchestra project data
///
/// # Returns
/// Migration preview with statistics
///
/// # Examples
/// ```
/// use rustycode_orchestra::migrate_preview::{
///     generate_preview, OrchestraProject, MilestoneData, SliceData, TaskData,
/// };
///
/// let project = OrchestraProject {
///     milestones: vec![
///         MilestoneData {
///             slices: vec![
///                 SliceData {
///                     done: true,
///                     tasks: vec![
///                         TaskData { done: true },
///                         TaskData { done: false },
///                     ],
///                 },
///             ],
///         },
///     ],
///     requirements: vec![],
/// };
///
/// let preview = generate_preview(&project);
/// assert_eq!(preview.total_slices, 1);
/// assert_eq!(preview.done_slices, 1);
/// assert_eq!(preview.total_tasks, 2);
/// assert_eq!(preview.done_tasks, 1);
/// assert_eq!(preview.slice_completion_pct, 100);
/// assert_eq!(preview.task_completion_pct, 50);
/// ```
pub fn generate_preview(project: &OrchestraProject) -> MigrationPreview {
    let mut total_slices = 0;
    let mut total_tasks = 0;
    let mut done_slices = 0;
    let mut done_tasks = 0;

    // Count slices and tasks across all milestones
    for milestone in &project.milestones {
        for slice in &milestone.slices {
            total_slices += 1;
            if slice.done {
                done_slices += 1;
            }
            for task in &slice.tasks {
                total_tasks += 1;
                if task.done {
                    done_tasks += 1;
                }
            }
        }
    }

    // Count requirements by status
    let mut req_counts = RequirementCounts::default();
    for req in &project.requirements {
        let status = req.status.to_lowercase();
        match status.as_str() {
            "active" => req_counts.active += 1,
            "validated" => req_counts.validated += 1,
            "deferred" => req_counts.deferred += 1,
            "out-of-scope" => req_counts.out_of_scope += 1,
            _ => {
                // Unknown status - count as active by default
                req_counts.active += 1;
            }
        }
        req_counts.total += 1;
    }

    // Calculate percentages
    let slice_completion_pct = (done_slices * 100_usize)
        .checked_div(total_slices)
        .unwrap_or(0) as u8;

    let task_completion_pct = (done_tasks * 100_usize)
        .checked_div(total_tasks)
        .unwrap_or(0) as u8;

    MigrationPreview {
        milestone_count: project.milestones.len(),
        total_slices,
        total_tasks,
        done_slices,
        done_tasks,
        slice_completion_pct,
        task_completion_pct,
        requirements: req_counts,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_preview_empty() {
        let project = OrchestraProject {
            milestones: vec![],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.milestone_count, 0);
        assert_eq!(preview.total_slices, 0);
        assert_eq!(preview.total_tasks, 0);
        assert_eq!(preview.done_slices, 0);
        assert_eq!(preview.done_tasks, 0);
        assert_eq!(preview.slice_completion_pct, 0);
        assert_eq!(preview.task_completion_pct, 0);
        assert_eq!(preview.requirements.total, 0);
    }

    #[test]
    fn test_generate_preview_single_milestone() {
        let project = OrchestraProject {
            milestones: vec![MilestoneData {
                slices: vec![
                    SliceData {
                        done: true,
                        tasks: vec![TaskData { done: true }, TaskData { done: true }],
                    },
                    SliceData {
                        done: false,
                        tasks: vec![TaskData { done: false }, TaskData { done: true }],
                    },
                ],
            }],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.milestone_count, 1);
        assert_eq!(preview.total_slices, 2);
        assert_eq!(preview.total_tasks, 4);
        assert_eq!(preview.done_slices, 1);
        assert_eq!(preview.done_tasks, 3);
        assert_eq!(preview.slice_completion_pct, 50);
        assert_eq!(preview.task_completion_pct, 75);
    }

    #[test]
    fn test_generate_preview_requirements() {
        let project = OrchestraProject {
            milestones: vec![],
            requirements: vec![
                RequirementData {
                    status: "active".to_string(),
                },
                RequirementData {
                    status: "validated".to_string(),
                },
                RequirementData {
                    status: "deferred".to_string(),
                },
                RequirementData {
                    status: "out-of-scope".to_string(),
                },
                RequirementData {
                    status: "unknown".to_string(),
                },
            ],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.requirements.active, 2); // active + unknown
        assert_eq!(preview.requirements.validated, 1);
        assert_eq!(preview.requirements.deferred, 1);
        assert_eq!(preview.requirements.out_of_scope, 1);
        assert_eq!(preview.requirements.total, 5);
    }

    #[test]
    fn test_generate_preview_completion_percentages() {
        let project = OrchestraProject {
            milestones: vec![MilestoneData {
                slices: vec![
                    SliceData {
                        done: true,
                        tasks: vec![TaskData { done: true }],
                    },
                    SliceData {
                        done: true,
                        tasks: vec![TaskData { done: false }],
                    },
                    SliceData {
                        done: false,
                        tasks: vec![TaskData { done: false }],
                    },
                ],
            }],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.total_slices, 3);
        assert_eq!(preview.done_slices, 2);
        assert_eq!(preview.slice_completion_pct, 66); // 2/3 = 66.6% -> 66%
        assert_eq!(preview.total_tasks, 3);
        assert_eq!(preview.done_tasks, 1);
        assert_eq!(preview.task_completion_pct, 33); // 1/3 = 33.3% -> 33%
    }

    #[test]
    fn test_generate_preview_zero_division() {
        let project = OrchestraProject {
            milestones: vec![MilestoneData { slices: vec![] }],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.total_slices, 0);
        assert_eq!(preview.total_tasks, 0);
        assert_eq!(preview.slice_completion_pct, 0);
        assert_eq!(preview.task_completion_pct, 0);
    }

    #[test]
    fn test_generate_preview_multiple_milestones() {
        let project = OrchestraProject {
            milestones: vec![
                MilestoneData {
                    slices: vec![SliceData {
                        done: true,
                        tasks: vec![TaskData { done: true }],
                    }],
                },
                MilestoneData {
                    slices: vec![SliceData {
                        done: false,
                        tasks: vec![TaskData { done: false }],
                    }],
                },
            ],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.milestone_count, 2);
        assert_eq!(preview.total_slices, 2);
        assert_eq!(preview.done_slices, 1);
        assert_eq!(preview.total_tasks, 2);
        assert_eq!(preview.done_tasks, 1);
    }

    #[test]
    fn test_generate_preview_case_insensitive_status() {
        let project = OrchestraProject {
            milestones: vec![],
            requirements: vec![
                RequirementData {
                    status: "ACTIVE".to_string(),
                },
                RequirementData {
                    status: "Validated".to_string(),
                },
                RequirementData {
                    status: "DEFERRED".to_string(),
                },
            ],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.requirements.active, 1);
        assert_eq!(preview.requirements.validated, 1);
        assert_eq!(preview.requirements.deferred, 1);
    }

    #[test]
    fn test_generate_preview_all_tasks_done() {
        let project = OrchestraProject {
            milestones: vec![MilestoneData {
                slices: vec![SliceData {
                    done: true,
                    tasks: vec![
                        TaskData { done: true },
                        TaskData { done: true },
                        TaskData { done: true },
                    ],
                }],
            }],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.total_tasks, 3);
        assert_eq!(preview.done_tasks, 3);
        assert_eq!(preview.task_completion_pct, 100);
    }

    #[test]
    fn test_generate_preview_all_tasks_not_done() {
        let project = OrchestraProject {
            milestones: vec![MilestoneData {
                slices: vec![SliceData {
                    done: false,
                    tasks: vec![TaskData { done: false }, TaskData { done: false }],
                }],
            }],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.total_tasks, 2);
        assert_eq!(preview.done_tasks, 0);
        assert_eq!(preview.task_completion_pct, 0);
    }

    #[test]
    fn test_generate_preview_no_requirements() {
        let project = OrchestraProject {
            milestones: vec![],
            requirements: vec![],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.requirements.active, 0);
        assert_eq!(preview.requirements.validated, 0);
        assert_eq!(preview.requirements.deferred, 0);
        assert_eq!(preview.requirements.out_of_scope, 0);
        assert_eq!(preview.requirements.total, 0);
    }

    #[test]
    fn test_generate_preview_all_statuses() {
        let project = OrchestraProject {
            milestones: vec![],
            requirements: vec![
                RequirementData {
                    status: "active".to_string(),
                },
                RequirementData {
                    status: "validated".to_string(),
                },
                RequirementData {
                    status: "deferred".to_string(),
                },
                RequirementData {
                    status: "out-of-scope".to_string(),
                },
            ],
        };

        let preview = generate_preview(&project);

        assert_eq!(preview.requirements.active, 1);
        assert_eq!(preview.requirements.validated, 1);
        assert_eq!(preview.requirements.deferred, 1);
        assert_eq!(preview.requirements.out_of_scope, 1);
        assert_eq!(preview.requirements.total, 4);
    }
}
