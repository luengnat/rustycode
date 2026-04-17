//! Orchestra Doctor Types — Type definitions for health check diagnostics
//!
//! Provides types for doctor issue codes, severities, and reports.
//! Used by the doctor system to identify and categorize project health issues.

use serde::{Deserialize, Serialize};

/// Doctor issue severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DoctorSeverity {
    Info,
    Warning,
    Error,
}

/// Doctor issue codes
///
/// Each code represents a specific type of issue that the doctor can detect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DoctorIssueCode {
    InvalidPreferences,
    MissingTasksDir,
    MissingSlicePlan,
    TaskDoneMissingSummary,
    TaskSummaryWithoutDoneCheckbox,
    AllTasksDoneMissingSliceSummary,
    AllTasksDoneMissingSliceUat,
    AllTasksDoneRoadmapNotChecked,
    SliceCheckedMissingSummary,
    SliceCheckedMissingUat,
    AllSlicesDoneMissingMilestoneValidation,
    AllSlicesDoneMissingMilestoneSummary,
    TaskDoneMustHavesNotVerified,
    ActiveRequirementMissingOwner,
    BlockedRequirementMissingReason,
    BlockerDiscoveredNoReplan,
    DelimiterInTitle,
    OrphanedAutoWorktree,
    StaleMilestoneBranch,
    CorruptMergeState,
    TrackedRuntimeFiles,
    LegacySliceBranches,
    StaleCrashLock,
    StaleParallelSession,
    OrphanedCompletedUnits,
    StaleHookState,
    ActivityLogBloat,
    StateFileStale,
    StateFileMissing,
    GitignoreMissingPatterns,
    UnresolvableDependency,
}

impl DoctorIssueCode {
    /// Get the string representation of the issue code
    pub fn as_str(&self) -> &'static str {
        match self {
            DoctorIssueCode::InvalidPreferences => "invalid_preferences",
            DoctorIssueCode::MissingTasksDir => "missing_tasks_dir",
            DoctorIssueCode::MissingSlicePlan => "missing_slice_plan",
            DoctorIssueCode::TaskDoneMissingSummary => "task_done_missing_summary",
            DoctorIssueCode::TaskSummaryWithoutDoneCheckbox => "task_summary_without_done_checkbox",
            DoctorIssueCode::AllTasksDoneMissingSliceSummary => {
                "all_tasks_done_missing_slice_summary"
            }
            DoctorIssueCode::AllTasksDoneMissingSliceUat => "all_tasks_done_missing_slice_uat",
            DoctorIssueCode::AllTasksDoneRoadmapNotChecked => "all_tasks_done_roadmap_not_checked",
            DoctorIssueCode::SliceCheckedMissingSummary => "slice_checked_missing_summary",
            DoctorIssueCode::SliceCheckedMissingUat => "slice_checked_missing_uat",
            DoctorIssueCode::AllSlicesDoneMissingMilestoneValidation => {
                "all_slices_done_missing_milestone_validation"
            }
            DoctorIssueCode::AllSlicesDoneMissingMilestoneSummary => {
                "all_slices_done_missing_milestone_summary"
            }
            DoctorIssueCode::TaskDoneMustHavesNotVerified => "task_done_must_haves_not_verified",
            DoctorIssueCode::ActiveRequirementMissingOwner => "active_requirement_missing_owner",
            DoctorIssueCode::BlockedRequirementMissingReason => {
                "blocked_requirement_missing_reason"
            }
            DoctorIssueCode::BlockerDiscoveredNoReplan => "blocker_discovered_no_replan",
            DoctorIssueCode::DelimiterInTitle => "delimiter_in_title",
            DoctorIssueCode::OrphanedAutoWorktree => "orphaned_auto_worktree",
            DoctorIssueCode::StaleMilestoneBranch => "stale_milestone_branch",
            DoctorIssueCode::CorruptMergeState => "corrupt_merge_state",
            DoctorIssueCode::TrackedRuntimeFiles => "tracked_runtime_files",
            DoctorIssueCode::LegacySliceBranches => "legacy_slice_branches",
            DoctorIssueCode::StaleCrashLock => "stale_crash_lock",
            DoctorIssueCode::StaleParallelSession => "stale_parallel_session",
            DoctorIssueCode::OrphanedCompletedUnits => "orphaned_completed_units",
            DoctorIssueCode::StaleHookState => "stale_hook_state",
            DoctorIssueCode::ActivityLogBloat => "activity_log_bloat",
            DoctorIssueCode::StateFileStale => "state_file_stale",
            DoctorIssueCode::StateFileMissing => "state_file_missing",
            DoctorIssueCode::GitignoreMissingPatterns => "gitignore_missing_patterns",
            DoctorIssueCode::UnresolvableDependency => "unresolvable_dependency",
        }
    }
}

/// Doctor issue detected during health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorIssue {
    pub severity: DoctorSeverity,
    pub code: DoctorIssueCode,
    pub scope: DoctorScope,
    pub unit_id: String,
    pub message: String,
    pub file: Option<String>,
    pub fixable: bool,
}

/// Scope of the doctor issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DoctorScope {
    Project,
    Milestone,
    Slice,
    Task,
}

/// Doctor report from a health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub base_path: String,
    pub issues: Vec<DoctorIssue>,
    pub fixes_applied: Vec<String>,
}

/// Doctor summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSummary {
    pub total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub fixable: usize,
    pub by_code: Vec<DoctorIssueCodeCount>,
}

/// Count of issues by code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorIssueCodeCount {
    pub code: DoctorIssueCode,
    pub count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_severity() {
        let severities = [
            DoctorSeverity::Info,
            DoctorSeverity::Warning,
            DoctorSeverity::Error,
        ];

        assert_eq!(severities.len(), 3);
    }

    #[test]
    fn test_doctor_issue_code_as_str() {
        assert_eq!(
            DoctorIssueCode::InvalidPreferences.as_str(),
            "invalid_preferences"
        );
        assert_eq!(
            DoctorIssueCode::StateFileMissing.as_str(),
            "state_file_missing"
        );
    }

    #[test]
    fn test_doctor_issue_code_count() {
        // Count all issue codes (should be 31 to match TypeScript)
        let all_codes = vec![
            DoctorIssueCode::InvalidPreferences,
            DoctorIssueCode::MissingTasksDir,
            DoctorIssueCode::MissingSlicePlan,
            DoctorIssueCode::TaskDoneMissingSummary,
            DoctorIssueCode::TaskSummaryWithoutDoneCheckbox,
            DoctorIssueCode::AllTasksDoneMissingSliceSummary,
            DoctorIssueCode::AllTasksDoneMissingSliceUat,
            DoctorIssueCode::AllTasksDoneRoadmapNotChecked,
            DoctorIssueCode::SliceCheckedMissingSummary,
            DoctorIssueCode::SliceCheckedMissingUat,
            DoctorIssueCode::AllSlicesDoneMissingMilestoneValidation,
            DoctorIssueCode::AllSlicesDoneMissingMilestoneSummary,
            DoctorIssueCode::TaskDoneMustHavesNotVerified,
            DoctorIssueCode::ActiveRequirementMissingOwner,
            DoctorIssueCode::BlockedRequirementMissingReason,
            DoctorIssueCode::BlockerDiscoveredNoReplan,
            DoctorIssueCode::DelimiterInTitle,
            DoctorIssueCode::OrphanedAutoWorktree,
            DoctorIssueCode::StaleMilestoneBranch,
            DoctorIssueCode::CorruptMergeState,
            DoctorIssueCode::TrackedRuntimeFiles,
            DoctorIssueCode::LegacySliceBranches,
            DoctorIssueCode::StaleCrashLock,
            DoctorIssueCode::StaleParallelSession,
            DoctorIssueCode::OrphanedCompletedUnits,
            DoctorIssueCode::StaleHookState,
            DoctorIssueCode::ActivityLogBloat,
            DoctorIssueCode::StateFileStale,
            DoctorIssueCode::StateFileMissing,
            DoctorIssueCode::GitignoreMissingPatterns,
            DoctorIssueCode::UnresolvableDependency,
        ];

        assert_eq!(all_codes.len(), 31);
    }

    #[test]
    fn test_doctor_issue_serialization() {
        let issue = DoctorIssue {
            severity: DoctorSeverity::Error,
            code: DoctorIssueCode::StateFileMissing,
            scope: DoctorScope::Project,
            unit_id: "PROJECT".to_string(),
            message: "State file is missing".to_string(),
            file: Some("/path/to/file.md".to_string()),
            fixable: true,
        };

        // Test serialization
        let json = serde_json::to_string(&issue).unwrap();
        assert!(json.contains("state_file_missing"));

        // Test deserialization
        let parsed: DoctorIssue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.severity, DoctorSeverity::Error);
        assert_eq!(parsed.code, DoctorIssueCode::StateFileMissing);
    }

    #[test]
    fn test_doctor_report_serialization() {
        let report = DoctorReport {
            ok: false,
            base_path: "/project".to_string(),
            issues: vec![],
            fixes_applied: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: DoctorReport = serde_json::from_str(&json).unwrap();

        assert!(!parsed.ok);
        assert_eq!(parsed.base_path, "/project");
    }

    #[test]
    fn test_doctor_summary() {
        let summary = DoctorSummary {
            total: 10,
            errors: 2,
            warnings: 5,
            infos: 3,
            fixable: 8,
            by_code: vec![],
        };

        assert_eq!(summary.total, 10);
        assert_eq!(summary.errors, 2);
        assert_eq!(summary.fixable, 8);
    }
}
