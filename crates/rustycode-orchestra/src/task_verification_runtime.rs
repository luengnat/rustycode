//! Deterministic task verification for the Orchestra runtime.
//!
//! Keeps command discovery, verification execution, and evidence writing
//! out of the main executor loop.

use std::path::Path;

use anyhow::Result;
use tracing::warn;

use crate::verification_evidence::{
    write_verification_json, DiscoverySource as EvidenceDiscoverySource,
    VerificationCheck as EvidenceVerificationCheck,
    VerificationResult as EvidenceVerificationResult,
};
use crate::verification_gate::{
    discover_commands, format_failure_context, run_verification_gate, DiscoverCommandsOptions,
    DiscoverySource as GateDiscoverySource,
};

#[derive(Debug, Clone)]
pub struct TaskVerificationOutcome {
    pub passed: bool,
    pub failure_context: String,
}

pub fn run_task_verification(
    project_root: &Path,
    unit_id: &str,
    task_plan: &str,
    task_dir: &Path,
    retry_attempt: u32,
    max_retries: u32,
) -> Result<TaskVerificationOutcome> {
    let discovered = discover_commands(&DiscoverCommandsOptions {
        preference_commands: None,
        task_plan_verify: extract_task_plan_verify(task_plan),
        cwd: project_root.to_string_lossy().to_string(),
    });

    let gate_result = run_verification_gate(
        &discovered.commands,
        &project_root.to_string_lossy(),
        discovered.source.clone(),
    );

    let evidence_result = EvidenceVerificationResult {
        passed: gate_result.all_passed,
        checks: gate_result
            .checks
            .iter()
            .map(|check| EvidenceVerificationCheck {
                command: check.command.clone(),
                exit_code: check.exit_code,
                stdout: check.stdout.clone().unwrap_or_default(),
                stderr: check.stderr.clone().unwrap_or_default(),
                duration_ms: check.duration_ms,
            })
            .collect(),
        discovery_source: map_discovery_source(&discovered.source),
        timestamp: chrono::Utc::now().timestamp_millis(),
        runtime_errors: None,
        audit_warnings: None,
    };

    if let Err(e) = write_verification_json(
        &evidence_result,
        task_dir,
        unit_id,
        Some(unit_id),
        Some(retry_attempt),
        Some(max_retries),
    ) {
        warn!(
            "Failed to write verification evidence for {}: {}",
            unit_id, e
        );
    }

    Ok(TaskVerificationOutcome {
        passed: gate_result.all_passed,
        failure_context: format_failure_context(&gate_result),
    })
}

fn extract_task_plan_verify(task_plan: &str) -> Option<String> {
    let mut in_verify_section = false;
    let mut commands = Vec::new();

    for line in task_plan.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            if heading.starts_with("verify") || heading.starts_with("verification") {
                in_verify_section = true;
                continue;
            }

            if in_verify_section {
                break;
            }
        }

        if !in_verify_section || trimmed.is_empty() {
            continue;
        }

        let candidate = trimmed
            .trim_start_matches("- [ ]")
            .trim_start_matches("- [x]")
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim();

        if candidate.contains('`') {
            let inline = candidate.replace('`', "");
            if !inline.is_empty() {
                commands.push(inline);
                continue;
            }
        }

        if candidate.starts_with("cargo ")
            || candidate.starts_with("npm ")
            || candidate.starts_with("pnpm ")
            || candidate.starts_with("yarn ")
            || candidate.starts_with("bun ")
            || candidate.starts_with("go ")
            || candidate.starts_with("python ")
            || candidate.starts_with("pytest ")
            || candidate.starts_with("make ")
        {
            commands.push(candidate.to_string());
        }
    }

    if commands.is_empty() {
        None
    } else {
        Some(commands.join(" && "))
    }
}

fn map_discovery_source(source: &GateDiscoverySource) -> EvidenceDiscoverySource {
    match source {
        GateDiscoverySource::Preference => EvidenceDiscoverySource::Preference,
        GateDiscoverySource::TaskPlan => EvidenceDiscoverySource::TaskPlan,
        GateDiscoverySource::PackageJson => EvidenceDiscoverySource::PackageJson,
        GateDiscoverySource::None => EvidenceDiscoverySource::None,
    }
}
