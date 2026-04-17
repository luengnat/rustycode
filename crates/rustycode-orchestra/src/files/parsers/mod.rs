//! File parsing modules for Autonomous Mode
//!
//! Organized parser modules for different file types:
//! - Roadmap parsing (ROADMAP.md)
//! - Slice plan parsing (PLAN.md)
//! - Summary parsing (SUMMARY.md)
//! - Continue file parsing (CONTINUE.md)
//!
//! Common utilities are in `common.rs`.

pub mod common;
pub mod r#continue;
pub mod overrides;
pub mod plan;
pub mod requirements;
pub mod roadmap;
pub mod secrets;
pub mod summary;

// Re-export commonly used types at the module level
pub use common::{extract_all_sections, extract_bold_field, extract_section, parse_bullets};
pub use overrides::{
    extract_uat_type, format_overrides_section, parse_context_depends_on, parse_overrides,
    Override, OverrideScope, UatType,
};
pub use plan::{parse_plan, SlicePlan, TaskPlanEntry};
pub use r#continue::{format_continue, parse_continue, Continue, ContinueFrontmatter};
pub use requirements::{
    count_must_haves_mentioned_in_summary, parse_requirement_counts, parse_task_plan_must_haves,
    MustHaveItem, RequirementCounts,
};
pub use roadmap::{parse_roadmap, BoundaryMapEntry, Roadmap, RoadmapSlice};
pub use secrets::{
    format_secrets_manifest, parse_secrets_manifest, SecretsManifest, SecretsManifestEntry,
    VALID_STATUSES,
};
pub use summary::{parse_summary, FileModified, RequiresEntry, Summary, SummaryFrontmatter};
