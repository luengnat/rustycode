//! Orchestra Session/Milestone Export — Generate shareable reports.
//!
//! Exports session and milestone data to JSON or markdown format.
//! Generates reports from metrics ledger including costs, tokens,
//! duration, and unit history.
//!
//! Matches orchestra-2's export.ts implementation.

use crate::error::{OrchestraV2Error, Result};
use crate::metrics::{format_cost, format_duration, format_token_count, TokenCounts, UnitMetrics};
use crate::paths::orchestra_root;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ─── Types ───────────────────────────────────────────────────────────────────

/// Export report data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReport {
    pub exported_at: String,
    pub project: String,
    pub totals: ExportTotals,
    pub by_phase: Vec<PhaseBreakdown>,
    pub by_slice: Vec<SliceBreakdown>,
    pub by_model: Vec<ModelBreakdown>,
    pub units: Vec<UnitMetrics>,
}

/// Export totals (simplified version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTotals {
    pub units: usize,
    pub cost: f64,
    pub tokens: u64,
    pub duration: u64,
    pub tool_calls: u64,
}

/// Phase breakdown for exports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseBreakdown {
    pub phase: String,
    pub units: usize,
    pub cost: f64,
    pub tokens: TokenCounts,
    pub duration: u64,
}

/// Slice breakdown for exports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceBreakdown {
    pub slice_id: String,
    pub units: usize,
    pub cost: f64,
    pub tokens: TokenCounts,
    pub duration: u64,
}

/// Model breakdown for exports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBreakdown {
    pub model: String,
    pub units: usize,
    pub cost: f64,
    pub tokens: TokenCounts,
    pub duration: u64,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Write an export file in JSON or markdown format.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `format` - Export format ("json" or "markdown")
///
/// # Returns
/// Path to exported file, or error if no units to export
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::export::*;
///
/// let out_path = write_export_file(
///     Path::new("/my/project"),
///     "markdown",
/// )?;
/// ```
pub fn write_export_file(base_path: &Path, format: &str) -> Result<PathBuf> {
    // Get units from metrics file
    let units = load_units_for_export(base_path)?;

    let project_name = base_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");

    let export_dir = orchestra_root(base_path);
    fs::create_dir_all(&export_dir).map_err(OrchestraV2Error::Io)?;

    let timestamp = chrono::Utc::now().to_rfc3339().replace([':', '.'], "-");

    let out_path = if format == "json" {
        export_json_report(&export_dir, &timestamp, project_name, &units)?
    } else {
        export_markdown_report(&export_dir, &timestamp, project_name, &units)?
    };

    Ok(out_path)
}

/// Load units from metrics ledger file
fn load_units_for_export(base_path: &Path) -> Result<Vec<UnitMetrics>> {
    let metrics_path = base_path.join(".orchestra/metrics.json");

    if !metrics_path.exists() {
        return Err(OrchestraV2Error::Serialization(
            "Nothing to export - no metrics file found".to_string(),
        ));
    }

    let content = fs::read_to_string(&metrics_path).map_err(OrchestraV2Error::Io)?;

    let ledger: MetricsLedger = serde_json::from_str(&content)
        .map_err(|e| OrchestraV2Error::Serialization(format!("Failed to parse metrics: {}", e)))?;

    if ledger.units.is_empty() {
        return Err(OrchestraV2Error::Serialization(
            "Nothing to export - no units executed yet".to_string(),
        ));
    }

    Ok(ledger.units)
}

/// Export report as JSON
fn export_json_report(
    export_dir: &Path,
    timestamp: &str,
    project_name: &str,
    units: &[UnitMetrics],
) -> Result<PathBuf> {
    let totals = calculate_totals(units);
    let by_phase = aggregate_by_phase(units);
    let by_slice = aggregate_by_slice_export(units);
    let by_model = aggregate_by_model(units);

    let report = ExportReport {
        exported_at: chrono::Utc::now().to_rfc3339(),
        project: project_name.to_string(),
        totals,
        by_phase,
        by_slice,
        by_model,
        units: units.to_vec(),
    };

    let json = serde_json::to_string_pretty(&report).map_err(|e| {
        OrchestraV2Error::Serialization(format!("Failed to serialize export: {}", e))
    })?;

    let out_path = export_dir.join(format!("export-{}.json", timestamp));
    fs::write(&out_path, json + "\n").map_err(OrchestraV2Error::Io)?;

    Ok(out_path)
}

/// Export report as markdown
fn export_markdown_report(
    export_dir: &Path,
    timestamp: &str,
    project_name: &str,
    units: &[UnitMetrics],
) -> Result<PathBuf> {
    let totals = calculate_totals(units);
    let phases = aggregate_by_phase(units);
    let slices = aggregate_by_slice_export(units);

    let mut md_lines = vec![
        format!("# Orchestra Session Report — {}", project_name),
        String::new(),
        format!("**Generated**: {}", chrono::Utc::now().to_rfc3339()),
        format!("**Units completed**: {}", totals.units),
        format!("**Total cost**: {}", format_cost(totals.cost)),
        format!("**Total tokens**: {}", format_token_count(totals.tokens)),
        format!(
            "**Total duration**: {}",
            format_duration(totals.duration.try_into().unwrap_or(0))
        ),
        format!("**Tool calls**: {}", totals.tool_calls),
        String::new(),
        "## Cost by Phase".to_string(),
        String::new(),
        "| Phase | Units | Cost | Tokens | Duration |".to_string(),
        "|-------|-------|------|--------|----------|".to_string(),
    ];

    for p in &phases {
        md_lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            p.phase,
            p.units,
            format_cost(p.cost),
            format_token_count(p.tokens.total),
            format_duration(p.duration.try_into().unwrap_or(0))
        ));
    }

    md_lines.push(String::new());
    md_lines.push("## Cost by Slice".to_string());
    md_lines.push(String::new());
    md_lines.push("| Slice | Units | Cost | Tokens | Duration |".to_string());
    md_lines.push("|-------|-------|------|--------|----------|".to_string());

    for s in &slices {
        md_lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            s.slice_id,
            s.units,
            format_cost(s.cost),
            format_token_count(s.tokens.total),
            format_duration(s.duration.try_into().unwrap_or(0))
        ));
    }

    md_lines.push(String::new());
    md_lines.push("## Unit History".to_string());
    md_lines.push(String::new());
    md_lines.push("| Type | ID | Model | Cost | Tokens | Duration |".to_string());
    md_lines.push("|------|-----|-------|------|--------|----------|".to_string());

    for u in units {
        let model_name = u.model.replace("claude-", "");
        let duration = u.finished_at.saturating_sub(u.started_at);
        md_lines.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            u.unit_type,
            u.id,
            model_name,
            format_cost(u.cost),
            format_token_count(u.tokens.total),
            format_duration(duration.max(0))
        ));
    }

    let md = md_lines.join("\n");
    let out_path = export_dir.join(format!("export-{}.md", timestamp));
    fs::write(&out_path, md).map_err(OrchestraV2Error::Io)?;

    Ok(out_path)
}

/// Calculate totals from units
fn calculate_totals(units: &[UnitMetrics]) -> ExportTotals {
    let mut totals = ExportTotals {
        units: units.len(),
        cost: 0.0,
        tokens: 0,
        duration: 0,
        tool_calls: 0,
    };

    for unit in units {
        totals.cost += unit.cost;
        totals.tokens += unit.tokens.total;
        totals.duration += unit.finished_at.saturating_sub(unit.started_at) as u64;
        totals.tool_calls += unit.tool_calls;
    }

    totals
}

/// Aggregate by phase for export
fn aggregate_by_phase(units: &[UnitMetrics]) -> Vec<PhaseBreakdown> {
    let mut phase_data: HashMap<String, PhaseData> = HashMap::new();

    for unit in units {
        // Extract phase from unit type
        let phase = extract_phase(&unit.unit_type).to_string();

        let entry = phase_data
            .entry(phase.clone())
            .or_insert_with(|| PhaseData {
                phase,
                units: 0,
                cost: 0.0,
                tokens: TokenCounts::new(),
                duration: 0,
            });

        entry.units += 1;
        entry.cost += unit.cost;
        entry.tokens.total += unit.tokens.total;
        entry.tokens.input += unit.tokens.input;
        entry.tokens.output += unit.tokens.output;
        entry.tokens.cache_write += unit.tokens.cache_write;
        entry.tokens.cache_read += unit.tokens.cache_read;
        entry.duration += unit.finished_at.saturating_sub(unit.started_at);
    }

    let mut phases: Vec<PhaseBreakdown> = phase_data
        .into_values()
        .map(|pd| PhaseBreakdown {
            phase: pd.phase,
            units: pd.units,
            cost: pd.cost,
            tokens: pd.tokens,
            duration: pd.duration as u64,
        })
        .collect();
    phases.sort_by(|a, b| a.phase.cmp(&b.phase));
    phases
}

/// Extract phase from unit type
fn extract_phase(unit_type: &str) -> &str {
    if unit_type.contains("research") {
        "research"
    } else if unit_type.contains("plan") {
        "plan"
    } else if unit_type.contains("execute") {
        "execute"
    } else if unit_type.contains("complete") {
        "complete"
    } else if unit_type.contains("reassess") {
        "reassess"
    } else if unit_type.contains("replan") {
        "replan"
    } else if unit_type.contains("rewrite") {
        "rewrite"
    } else if unit_type.contains("uat") {
        "uat"
    } else {
        "other"
    }
}

/// Aggregate by slice for export (returns SliceBreakdown)
fn aggregate_by_slice_export(units: &[UnitMetrics]) -> Vec<SliceBreakdown> {
    let mut slice_data: HashMap<String, SliceData> = HashMap::new();

    for unit in units {
        // Extract slice ID from unit ID (format: M001-S01-T01)
        let slice_id = extract_slice_id(&unit.id);

        let entry = slice_data
            .entry(slice_id.clone())
            .or_insert_with(|| SliceData {
                slice_id: slice_id.clone(),
                units: 0,
                cost: 0.0,
                tokens: TokenCounts::new(),
                duration: 0,
            });

        entry.units += 1;
        entry.cost += unit.cost;
        entry.tokens.total += unit.tokens.total;
        entry.tokens.input += unit.tokens.input;
        entry.tokens.output += unit.tokens.output;
        entry.tokens.cache_write += unit.tokens.cache_write;
        entry.tokens.cache_read += unit.tokens.cache_read;
        entry.duration += unit.finished_at.saturating_sub(unit.started_at);
    }

    let mut slices: Vec<SliceBreakdown> = slice_data
        .into_values()
        .map(|sd| SliceBreakdown {
            slice_id: sd.slice_id,
            units: sd.units,
            cost: sd.cost,
            tokens: sd.tokens,
            duration: sd.duration as u64,
        })
        .collect();
    slices.sort_by(|a, b| a.slice_id.cmp(&b.slice_id));
    slices
}

/// Extract slice ID from unit ID
fn extract_slice_id(unit_id: &str) -> String {
    // Format: M001-S01-T01 or M001-S01
    let parts: Vec<&str> = unit_id.split('/').collect();
    if parts.len() >= 2 {
        parts[0..2].join("/")
    } else {
        unit_id.to_string()
    }
}

/// Aggregate by model for export
fn aggregate_by_model(units: &[UnitMetrics]) -> Vec<ModelBreakdown> {
    let mut model_data: HashMap<String, ModelData> = HashMap::new();

    for unit in units {
        let model_name = unit.model.replace("claude-", "");
        let entry = model_data
            .entry(model_name.clone())
            .or_insert_with(|| ModelData {
                model: model_name.clone(),
                units: 0,
                cost: 0.0,
                tokens: TokenCounts::new(),
                duration: 0,
            });

        entry.units += 1;
        entry.cost += unit.cost;
        entry.tokens.total += unit.tokens.total;
        entry.tokens.input += unit.tokens.input;
        entry.tokens.output += unit.tokens.output;
        entry.tokens.cache_write += unit.tokens.cache_write;
        entry.tokens.cache_read += unit.tokens.cache_read;
        entry.duration += unit.finished_at.saturating_sub(unit.started_at);
    }

    let mut models: Vec<ModelBreakdown> = model_data
        .into_values()
        .map(|md| ModelBreakdown {
            model: md.model,
            units: md.units,
            cost: md.cost,
            tokens: md.tokens,
            duration: md.duration as u64,
        })
        .collect();
    models.sort_by(|a, b| a.model.cmp(&b.model));
    models
}

/// Helper structure for phase aggregation
#[derive(Debug, Clone)]
struct PhaseData {
    phase: String,
    units: usize,
    cost: f64,
    tokens: TokenCounts,
    duration: i64,
}

/// Helper structure for slice aggregation
#[derive(Debug, Clone)]
struct SliceData {
    slice_id: String,
    units: usize,
    cost: f64,
    tokens: TokenCounts,
    duration: i64,
}

/// Helper structure for model aggregation
#[derive(Debug, Clone)]
struct ModelData {
    model: String,
    units: usize,
    cost: f64,
    tokens: TokenCounts,
    duration: i64,
}

/// Metrics ledger file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetricsLedger {
    units: Vec<UnitMetrics>,
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_units_for_export_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = load_units_for_export(base_path);

        // Should fail - no metrics file
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_totals() {
        // This would require mocking UnitMetrics
        // For now, just test that the function exists
        let units = vec![];
        let totals = calculate_totals(&units);
        assert_eq!(totals.units, 0);
        assert_eq!(totals.cost, 0.0);
    }

    #[test]
    fn test_extract_phase() {
        assert_eq!(extract_phase("research-milestone"), "research");
        assert_eq!(extract_phase("plan-slice"), "plan");
        assert_eq!(extract_phase("execute-task"), "execute");
        assert_eq!(extract_phase("complete-slice"), "complete");
        assert_eq!(extract_phase("unknown-type"), "other");
    }

    #[test]
    fn test_extract_slice_id() {
        assert_eq!(extract_slice_id("M001/S01/T01"), "M001/S01");
        assert_eq!(extract_slice_id("M001/S01"), "M001/S01");
        assert_eq!(extract_slice_id("T01"), "T01");
    }

    #[test]
    fn test_aggregate_by_phase_empty() {
        let units = vec![];
        let result = aggregate_by_phase(&units);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_aggregate_by_slice_export_empty() {
        let units = vec![];
        let result = aggregate_by_slice_export(&units);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_aggregate_by_model_empty() {
        let units = vec![];
        let result = aggregate_by_model(&units);
        assert_eq!(result.len(), 0);
    }
}
