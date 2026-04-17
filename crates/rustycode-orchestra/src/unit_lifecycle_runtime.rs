//! Deterministic slice and milestone lifecycle helpers.
//!
//! These unit types do not need LLM reasoning, so their file updates and
//! report generation live outside the main executor.

use std::path::Path;

use anyhow::Result;
use tracing::info;
use walkdir::WalkDir;

pub fn complete_slice(
    project_root: &Path,
    slice_id: &str,
    milestone_id: &str,
    slice_title: &str,
) -> Result<()> {
    info!("✅ Completing slice: {} ({})", slice_id, slice_title);

    let slice_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("slices")
        .join(slice_id);
    let tasks_dir = slice_path.join("tasks");

    let mut task_summaries = Vec::new();
    if tasks_dir.exists() {
        for entry in WalkDir::new(&tasks_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let task_id = entry
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let summary_path = entry.path().join(format!("{}-SUMMARY.md", task_id));
            if summary_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&summary_path) {
                    task_summaries.push((task_id.to_string(), content));
                }
            }
        }
    }

    let summary_path = slice_path.join(format!("{}-SUMMARY.md", slice_id));
    let summary_content = format!(
        "# Slice {} Summary: {}\n\n**Completed:** {}\n\n## Tasks Completed\n\n{}\n\n## Overview\n\nThis slice completed successfully with {} task(s).\n",
        slice_id,
        slice_title,
        chrono::Utc::now().format("%Y-%m-%d"),
        task_summaries
            .iter()
            .map(|(id, _)| format!("- {}", id))
            .collect::<Vec<_>>()
            .join("\n"),
        task_summaries.len()
    );
    std::fs::write(&summary_path, summary_content)?;
    info!("📝 Slice summary written: {:?}", summary_path);

    let roadmap_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("ROADMAP.md");
    mark_slice_done(&roadmap_path, slice_id)?;
    info!("✅ Slice {} marked as done in ROADMAP.md", slice_id);

    Ok(())
}

pub fn validate_milestone(
    project_root: &Path,
    milestone_id: &str,
    milestone_title: &str,
) -> Result<()> {
    info!(
        "✔️  Validating milestone: {} ({})",
        milestone_id, milestone_title
    );

    let roadmap_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("ROADMAP.md");
    let roadmap_content = std::fs::read_to_string(&roadmap_path)?;

    let mut complete_count = 0;
    let mut incomplete_count = 0;
    for line in roadmap_content.lines() {
        if line.contains("- [x]") {
            complete_count += 1;
        } else if line.contains("- [ ]") {
            incomplete_count += 1;
        }
    }

    if incomplete_count > 0 {
        info!("⚠️  Milestone has {} incomplete slice(s)", incomplete_count);
    }

    let validation_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("VALIDATION.md");
    let validation_content = format!(
        "# Milestone {} Validation\n\n**Milestone:** {} ({})\n**Validated:** {}\n\n## Results\n\n- Complete slices: {}\n- Incomplete slices: {}\n\n## Status\n\n{}\n",
        milestone_id,
        milestone_title,
        milestone_id,
        chrono::Utc::now().format("%Y-%m-%d"),
        complete_count,
        incomplete_count,
        if incomplete_count == 0 {
            "✅ All slices complete - milestone achieved!"
        } else {
            "⚠️  Milestone has incomplete slices - continue execution"
        }
    );
    std::fs::write(&validation_path, validation_content)?;
    info!("📝 Validation report written: {:?}", validation_path);

    Ok(())
}

fn mark_slice_done(roadmap_path: &Path, slice_id: &str) -> Result<()> {
    let content = std::fs::read_to_string(roadmap_path)?;

    let mut updated = String::new();
    for line in content.lines() {
        if line.contains(&format!("- [ ] {}:", slice_id)) {
            updated.push_str(&line.replace("- [ ]", "- [x]"));
        } else {
            updated.push_str(line);
        }
        updated.push('\n');
    }

    std::fs::write(roadmap_path, updated)?;
    Ok(())
}
