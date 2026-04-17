//! Orchestra Auto Direct Dispatch — Handles manual /orchestra dispatch commands
//!
//! Resolves phase name → unit type + prompt, creates a session, and sends the message.

use crate::files::{load_file, parse_roadmap};
use crate::paths::{rel_slice_file, resolve_milestone_file, resolve_slice_file};
use std::path::Path;

/// Direct dispatch result
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DirectDispatchResult {
    /// Successfully dispatched
    Dispatched {
        unit_type: String,
        unit_id: String,
        prompt: String,
    },
    /// Cannot dispatch - missing context
    CannotDispatch { reason: String },
    /// Unknown phase
    UnknownPhase { phase: String },
    /// Session cancelled
    Cancelled,
}

/// Dispatch a direct phase command
///
/// # Arguments
/// * `phase` - Phase name to dispatch
/// * `base_path` - Project base path
/// * `state` - Current Orchestra state
///
/// # Returns
/// Direct dispatch result
///
/// # Example
/// ```
/// use rustycode_orchestra::auto_direct_dispatch::*;
/// use rustycode_orchestra::state_derivation::OrchestraState;
///
/// let state = OrchestraState::default();
/// let result = dispatch_direct_phase("plan-slice", "/project", &state);
/// ```
pub fn dispatch_direct_phase(
    phase: &str,
    base_path: &str,
    state: &crate::state_derivation::OrchestraState,
) -> DirectDispatchResult {
    let normalized = phase.to_lowercase();

    let mid = match &state.active_milestone {
        Some(m) => m.id.clone(),
        None => {
            return DirectDispatchResult::CannotDispatch {
                reason: "no active milestone".to_string(),
            }
        }
    };

    let mid_title = state
        .active_milestone
        .as_ref()
        .map(|m| m.title.clone())
        .unwrap_or_default();

    match normalized.as_str() {
        "research" | "research-milestone" | "research-slice" => {
            let is_slice = normalized == "research-slice"
                || (normalized == "research"
                    && !matches!(state.phase, crate::phases::Phase::Research));

            if is_slice {
                let sid = match &state.active_slice {
                    Some(s) => s.id.clone(),
                    None => {
                        return DirectDispatchResult::CannotDispatch {
                            reason: "no active slice".to_string(),
                        }
                    }
                };

                let s_title = state
                    .active_slice
                    .as_ref()
                    .map(|s| s.title.clone())
                    .unwrap_or_default();

                // Check for require_slice_discussion preference
                let slice_context_file =
                    resolve_slice_file(Path::new(base_path), &mid, &sid, "CONTEXT");
                let require_discussion = check_require_slice_discussion();

                if require_discussion && slice_context_file.is_none() {
                    return DirectDispatchResult::CannotDispatch {
                        reason: format!(
                            "Slice {} requires discussion before planning. Run /orchestra discuss to discuss this slice, then /orchestra auto to resume.",
                            sid
                        ),
                    };
                }

                let prompt =
                    build_research_slice_prompt(&mid, &mid_title, &sid, &s_title, base_path);

                DirectDispatchResult::Dispatched {
                    unit_type: "research-slice".to_string(),
                    unit_id: format!("{}/{}", mid, sid),
                    prompt,
                }
            } else {
                let prompt = build_research_milestone_prompt(&mid, &mid_title, base_path);

                DirectDispatchResult::Dispatched {
                    unit_type: "research-milestone".to_string(),
                    unit_id: mid.clone(),
                    prompt,
                }
            }
        }

        "plan" | "plan-milestone" | "plan-slice" => {
            let is_slice = normalized == "plan-slice"
                || (normalized == "plan" && !matches!(state.phase, crate::phases::Phase::Research));

            if is_slice {
                let sid = match &state.active_slice {
                    Some(s) => s.id.clone(),
                    None => {
                        return DirectDispatchResult::CannotDispatch {
                            reason: "no active slice".to_string(),
                        }
                    }
                };

                let s_title = state
                    .active_slice
                    .as_ref()
                    .map(|s| s.title.clone())
                    .unwrap_or_default();

                let prompt = build_plan_slice_prompt(&mid, &mid_title, &sid, &s_title, base_path);

                DirectDispatchResult::Dispatched {
                    unit_type: "plan-slice".to_string(),
                    unit_id: format!("{}/{}", mid, sid),
                    prompt,
                }
            } else {
                let prompt = build_plan_milestone_prompt(&mid, &mid_title, base_path);

                DirectDispatchResult::Dispatched {
                    unit_type: "plan-milestone".to_string(),
                    unit_id: mid.clone(),
                    prompt,
                }
            }
        }

        "execute" | "execute-task" => {
            let sid = match &state.active_slice {
                Some(s) => s.id.clone(),
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "no active slice".to_string(),
                    }
                }
            };

            let s_title = state
                .active_slice
                .as_ref()
                .map(|s| s.title.clone())
                .unwrap_or_default();

            let tid = match &state.active_task {
                Some(t) => t.id.clone(),
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "no active task".to_string(),
                    }
                }
            };

            let t_title = state
                .active_task
                .as_ref()
                .map(|t| t.title.clone())
                .unwrap_or_default();

            let prompt = build_execute_task_prompt(&mid, &sid, &s_title, &tid, &t_title, base_path);

            DirectDispatchResult::Dispatched {
                unit_type: "execute-task".to_string(),
                unit_id: format!("{}/{}/{}", mid, sid, tid),
                prompt,
            }
        }

        "complete" | "complete-slice" | "complete-milestone" => {
            let is_slice = normalized == "complete-slice"
                || (normalized == "complete"
                    && matches!(state.phase, crate::phases::Phase::Complete));

            if is_slice {
                let sid = match &state.active_slice {
                    Some(s) => s.id.clone(),
                    None => {
                        return DirectDispatchResult::CannotDispatch {
                            reason: "no active slice".to_string(),
                        }
                    }
                };

                let s_title = state
                    .active_slice
                    .as_ref()
                    .map(|s| s.title.clone())
                    .unwrap_or_default();

                let prompt =
                    build_complete_slice_prompt(&mid, &mid_title, &sid, &s_title, base_path);

                DirectDispatchResult::Dispatched {
                    unit_type: "complete-slice".to_string(),
                    unit_id: format!("{}/{}", mid, sid),
                    prompt,
                }
            } else {
                let prompt = build_complete_milestone_prompt(&mid, &mid_title, base_path);

                DirectDispatchResult::Dispatched {
                    unit_type: "complete-milestone".to_string(),
                    unit_id: mid.clone(),
                    prompt,
                }
            }
        }

        "reassess" | "reassess-roadmap" => {
            let roadmap_file = resolve_milestone_file(Path::new(base_path), &mid, "ROADMAP");

            let roadmap_content = match roadmap_file {
                Some(path) => match load_file(&path) {
                    Some(content) => content,
                    None => {
                        return DirectDispatchResult::CannotDispatch {
                            reason: "no roadmap found".to_string(),
                        }
                    }
                },
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "no roadmap found".to_string(),
                    }
                }
            };

            let roadmap = parse_roadmap(&roadmap_content);
            let completed_slices: Vec<_> = roadmap
                .slices
                .iter()
                .filter(|s| s.status == "done")
                .collect();

            if completed_slices.is_empty() {
                return DirectDispatchResult::CannotDispatch {
                    reason: "no completed slices".to_string(),
                };
            }

            let completed_slice_id = &completed_slices.last().unwrap().id;
            let prompt =
                build_reassess_roadmap_prompt(&mid, &mid_title, completed_slice_id, base_path);

            DirectDispatchResult::Dispatched {
                unit_type: "reassess-roadmap".to_string(),
                unit_id: format!("{}/{}", mid, completed_slice_id),
                prompt,
            }
        }

        "uat" | "run-uat" => {
            let sid = match &state.active_slice {
                Some(s) => s.id.clone(),
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "no active slice".to_string(),
                    }
                }
            };

            let uat_file = resolve_slice_file(Path::new(base_path), &mid, &sid, "UAT");

            let uat_path = match uat_file {
                Some(path) => path,
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "no UAT file found".to_string(),
                    }
                }
            };

            let uat_content = match load_file(&uat_path) {
                Some(content) => content,
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "UAT file is empty".to_string(),
                    }
                }
            };

            let uat_rel_path = rel_slice_file(Path::new(base_path), &mid, &sid, "UAT")
                .to_string_lossy()
                .to_string();

            let prompt = build_run_uat_prompt(&mid, &sid, &uat_rel_path, &uat_content, base_path);

            DirectDispatchResult::Dispatched {
                unit_type: "run-uat".to_string(),
                unit_id: format!("{}/{}", mid, sid),
                prompt,
            }
        }

        "replan" | "replan-slice" => {
            let sid = match &state.active_slice {
                Some(s) => s.id.clone(),
                None => {
                    return DirectDispatchResult::CannotDispatch {
                        reason: "no active slice".to_string(),
                    }
                }
            };

            let s_title = state
                .active_slice
                .as_ref()
                .map(|s| s.title.clone())
                .unwrap_or_default();

            let prompt = build_replan_slice_prompt(&mid, &mid_title, &sid, &s_title, base_path);

            DirectDispatchResult::Dispatched {
                unit_type: "replan-slice".to_string(),
                unit_id: format!("{}/{}", mid, sid),
                prompt,
            }
        }

        _ => DirectDispatchResult::UnknownPhase {
            phase: phase.to_string(),
        },
    }
}

/// Check if require_slice_discussion preference is enabled
///
/// This function checks the user's preference for requiring slice discussion
/// before proceeding with execution. Currently returns `false` as a default.
///
/// # Future Implementation
///
/// This should load from the Orchestra preferences system (`.orchestra/preferences.md`):
/// ```yaml
/// require_slice_discussion: true  # Require discussion before each slice
/// ```
///
/// # Returns
/// `true` if slice discussion is required, `false` otherwise
fn check_require_slice_discussion() -> bool {
    // Currently defaults to false - slice discussion not required
    // TODO: Load from .orchestra/preferences.md when preferences system is implemented
    false
}

/// Build research slice prompt
fn build_research_slice_prompt(
    mid: &str,
    mid_title: &str,
    sid: &str,
    s_title: &str,
    _base: &str,
) -> String {
    format!(
        "# Research Slice: {} - {}\n\n\
        You are researching the requirements for slice {} of milestone {}.\n\n\
        ## Slice Context\n\
        - Milestone: {} - {}\n\
        - Slice: {} - {}\n\n\
        Please gather and document requirements for this slice.",
        sid, s_title, sid, mid, mid, mid_title, sid, s_title
    )
}

/// Build research milestone prompt
fn build_research_milestone_prompt(mid: &str, mid_title: &str, _base: &str) -> String {
    format!(
        "# Research Milestone: {} - {}\n\n\
        You are researching the requirements for milestone {}.\n\n\
        Please gather and document requirements for this milestone.",
        mid, mid_title, mid
    )
}

/// Build plan slice prompt
fn build_plan_slice_prompt(
    mid: &str,
    mid_title: &str,
    sid: &str,
    s_title: &str,
    _base: &str,
) -> String {
    format!(
        "# Plan Slice: {} - {}\n\n\
        You are creating a detailed plan for slice {} of milestone {}.\n\n\
        ## Slice Context\n\
        - Milestone: {} - {}\n\
        - Slice: {} - {}\n\n\
        Please create a comprehensive plan for this slice.",
        sid, s_title, sid, mid, mid, mid_title, sid, s_title
    )
}

/// Build plan milestone prompt
fn build_plan_milestone_prompt(mid: &str, mid_title: &str, _base: &str) -> String {
    format!(
        "# Plan Milestone: {} - {}\n\n\
        You are creating a detailed plan for milestone {}.\n\n\
        Please create a comprehensive plan for this milestone.",
        mid, mid_title, mid
    )
}

/// Build execute task prompt
fn build_execute_task_prompt(
    mid: &str,
    sid: &str,
    s_title: &str,
    tid: &str,
    t_title: &str,
    _base: &str,
) -> String {
    format!(
        "# Execute Task: {} - {}\n\n\
        You are executing task {} of slice {} in milestone {}.\n\n\
        ## Task Context\n\
        - Slice: {} - {}\n\
        - Task: {} - {}\n\n\
        Please execute this task following the plan.",
        tid, t_title, tid, sid, mid, sid, s_title, tid, t_title
    )
}

/// Build complete slice prompt
fn build_complete_slice_prompt(
    mid: &str,
    mid_title: &str,
    sid: &str,
    s_title: &str,
    _base: &str,
) -> String {
    format!(
        "# Complete Slice: {} - {}\n\n\
        You are completing slice {} of milestone {}.\n\n\
        ## Slice Context\n\
        - Milestone: {} - {}\n\
        - Slice: {} - {}\n\n\
        Please create a summary of this slice, including:\n\
        - What was accomplished\n\
        - Any deviations from the plan\n\
        - Lessons learned\n\
        - Next steps",
        sid, s_title, sid, mid, mid, mid_title, sid, s_title
    )
}

/// Build complete milestone prompt
fn build_complete_milestone_prompt(mid: &str, mid_title: &str, _base: &str) -> String {
    format!(
        "# Complete Milestone: {} - {}\n\n\
        You are completing milestone {}.\n\n\
        Please create a comprehensive summary of this milestone, including:\n\
        - What was accomplished\n\
        - Metrics and outcomes\n\
        - Challenges and how they were overcome\n\
        - Lessons learned\n\
        - Recommendations for future milestones",
        mid, mid_title, mid
    )
}

/// Build reassess roadmap prompt
fn build_reassess_roadmap_prompt(
    mid: &str,
    mid_title: &str,
    completed_slice_id: &str,
    _base: &str,
) -> String {
    format!(
        "# Reassess Roadmap: {}\n\n\
        You are reassessing the roadmap for milestone {} after completing slice {}.\n\n\
        Please review the current roadmap and recommend any necessary adjustments.",
        mid, mid_title, completed_slice_id
    )
}

/// Build run UAT prompt
fn build_run_uat_prompt(
    _mid: &str,
    sid: &str,
    uat_path: &str,
    uat_content: &str,
    _base: &str,
) -> String {
    format!(
        "# Run UAT: {} - {}\n\n\
        You are running user acceptance tests for slice {}.\n\n\
        ## UAT Plan\n\
        File: {}\n\n\
        {}\n\n\
        Please execute the UAT plan and document results.",
        sid, uat_path, sid, uat_path, uat_content
    )
}

/// Build replan slice prompt
fn build_replan_slice_prompt(
    mid: &str,
    mid_title: &str,
    sid: &str,
    s_title: &str,
    _base: &str,
) -> String {
    format!(
        "# Replan Slice: {} - {}\n\n\
        You are replanning slice {} of milestone {}.\n\n\
        ## Slice Context\n\
        - Milestone: {} - {}\n\
        - Slice: {} - {}\n\n\
        Please review the current plan and create an updated plan based on:\n\
        - What has been accomplished so far\n\
        - Any issues or blockers encountered\n\
        - Changes in requirements or priorities\n\
        - Lessons learned from execution",
        sid, s_title, sid, mid, mid, mid_title, sid, s_title
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phases::Phase;
    use crate::state_derivation::{MilestoneRef, OrchestraState, SliceRef, TaskRef};
    use std::path::PathBuf;

    fn create_test_state() -> OrchestraState {
        OrchestraState {
            active_milestone: Some(MilestoneRef {
                id: "M01".to_string(),
                title: "First Milestone".to_string(),
                path: PathBuf::from("/project/.orchestra/milestones/M01"),
            }),
            active_slice: Some(SliceRef {
                id: "S01".to_string(),
                title: "First Slice".to_string(),
                path: PathBuf::from("/project/.orchestra/milestones/M01/slices/S01"),
            }),
            active_task: Some(TaskRef {
                id: "T01".to_string(),
                title: "First Task".to_string(),
                path: PathBuf::from("/project/.orchestra/milestones/M01/slices/S01/tasks/T01"),
                done: false,
            }),
            phase: Phase::Plan,
            milestones: Vec::new(),
        }
    }

    #[test]
    fn test_dispatch_plan_slice() {
        let state = create_test_state();
        let result = dispatch_direct_phase("plan-slice", "/project", &state);

        assert!(matches!(result, DirectDispatchResult::Dispatched { .. }));
        if let DirectDispatchResult::Dispatched {
            unit_type, unit_id, ..
        } = result
        {
            assert_eq!(unit_type, "plan-slice");
            assert_eq!(unit_id, "M01/S01");
        }
    }

    #[test]
    fn test_dispatch_execute_task() {
        let state = create_test_state();
        let result = dispatch_direct_phase("execute-task", "/project", &state);

        assert!(matches!(result, DirectDispatchResult::Dispatched { .. }));
        if let DirectDispatchResult::Dispatched {
            unit_type, unit_id, ..
        } = result
        {
            assert_eq!(unit_type, "execute-task");
            assert_eq!(unit_id, "M01/S01/T01");
        }
    }

    #[test]
    fn test_dispatch_complete_slice() {
        let state = create_test_state();
        let result = dispatch_direct_phase("complete-slice", "/project", &state);

        assert!(matches!(result, DirectDispatchResult::Dispatched { .. }));
        if let DirectDispatchResult::Dispatched {
            unit_type, unit_id, ..
        } = result
        {
            assert_eq!(unit_type, "complete-slice");
            assert_eq!(unit_id, "M01/S01");
        }
    }

    #[test]
    fn test_unknown_phase() {
        let state = create_test_state();
        let result = dispatch_direct_phase("unknown-phase", "/project", &state);

        assert!(matches!(result, DirectDispatchResult::UnknownPhase { .. }));
    }

    #[test]
    fn test_no_active_milestone() {
        let state = OrchestraState {
            phase: Phase::Plan,
            active_milestone: None,
            active_slice: None,
            active_task: None,
            milestones: Vec::new(),
        };

        let result = dispatch_direct_phase("plan-slice", "/project", &state);

        assert!(matches!(
            result,
            DirectDispatchResult::CannotDispatch { .. }
        ));
    }

    #[test]
    fn test_no_active_slice_for_slice_phase() {
        let state = OrchestraState {
            active_milestone: Some(MilestoneRef {
                id: "M01".to_string(),
                title: "First Milestone".to_string(),
                path: PathBuf::from("/project/.orchestra/milestones/M01"),
            }),
            phase: Phase::Plan,
            active_slice: None,
            active_task: None,
            milestones: Vec::new(),
        };

        let result = dispatch_direct_phase("plan-slice", "/project", &state);

        assert!(matches!(
            result,
            DirectDispatchResult::CannotDispatch { .. }
        ));
    }

    #[test]
    fn test_phase_normalization() {
        let state = create_test_state();

        // Test various case variations
        let phases = vec!["PLAN", "Plan", "PLAN-SLICE", "plan-slice"];

        for phase in phases {
            let result = dispatch_direct_phase(phase, "/project", &state);
            assert!(matches!(result, DirectDispatchResult::Dispatched { .. }));
        }
    }
}
