//! Orchestra Paths — ID-based Path Resolution
//!
//! Provides path resolution for Orchestra project structure:
//! * Milestone, slice, and task path resolution
//! * File name builders (ID-SUFFIX.md format)
//! * Directory and file finding with legacy support
//! * Relative path builders for prompts
//! * Directory listing cache for performance
//!
//! Critical for autonomous systems to locate artifacts without tool calls.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::debug;

// ─── Constants ────────────────────────────────────────────────────────────────────

/// Orchestra root directory names
pub const ORCHESTRA_DIR: &str = ".orchestra";

/// Maximum directory cache size
const DIR_CACHE_MAX: usize = 128;

/// Orchestra root file names
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum OrchestraRootFile {
    Project,
    Decisions,
    Queue,
    State,
    Requirements,
    Overrides,
    Knowledge,
}

impl OrchestraRootFile {
    fn as_str(&self) -> &'static str {
        match self {
            OrchestraRootFile::Project => "PROJECT.md",
            OrchestraRootFile::Decisions => "DECISIONS.md",
            OrchestraRootFile::Queue => "QUEUE.md",
            OrchestraRootFile::State => "STATE.md",
            OrchestraRootFile::Requirements => "REQUIREMENTS.md",
            OrchestraRootFile::Overrides => "OVERRIDES.md",
            OrchestraRootFile::Knowledge => "KNOWLEDGE.md",
        }
    }

    fn legacy_name(&self) -> &'static str {
        match self {
            OrchestraRootFile::Project => "project.md",
            OrchestraRootFile::Decisions => "decisions.md",
            OrchestraRootFile::Queue => "queue.md",
            OrchestraRootFile::State => "state.md",
            OrchestraRootFile::Requirements => "requirements.md",
            OrchestraRootFile::Overrides => "overrides.md",
            OrchestraRootFile::Knowledge => "knowledge.md",
        }
    }
}

// ─── Directory Listing Cache ───────────────────────────────────────────────────

/// Global directory listing cache
///
/// This cache is used to avoid repeated filesystem calls when resolving
/// paths. It should be cleared after milestone transitions, file creation
/// in planning directories, or at the start/end of a dispatch cycle.
static DIR_CACHE: OnceLock<Mutex<HashMap<PathBuf, Vec<String>>>> = OnceLock::new();

fn get_dir_cache() -> &'static Mutex<HashMap<PathBuf, Vec<String>>> {
    DIR_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clear the directory listing cache
///
/// Call after milestone transitions, file creation in planning directories,
/// or at the start/end of a dispatch cycle.
pub fn clear_path_cache() {
    let mut cache = get_dir_cache().lock().unwrap_or_else(|e| e.into_inner());
    cache.clear();
    debug!("Path cache cleared");
}

/// Read directory entries with caching
fn read_dir_cached(dir: &Path) -> Result<Vec<String>> {
    let cache = get_dir_cache().lock().unwrap_or_else(|e| e.into_inner());

    if let Some(entries) = cache.get(dir) {
        return Ok(entries.clone());
    }

    // Release lock before filesystem operation
    drop(cache);

    // Read directory
    let entries: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();

    // Re-acquire lock to update cache
    let mut cache = get_dir_cache().lock().unwrap_or_else(|e| e.into_inner());

    // Enforce cache size limit
    if cache.len() >= DIR_CACHE_MAX {
        // Clear oldest entries (simple strategy: clear all)
        cache.clear();
    }

    cache.insert(dir.to_path_buf(), entries.clone());
    debug!("Cached {} entries from {:?}", entries.len(), dir);

    Ok(entries)
}

// ─── Name Builders ──────────────────────────────────────────────────────────────

/// Build a directory name from an ID
///
/// # Example
/// ```
/// assert_eq!(build_dir_name("M001"), "M001");
/// ```
pub fn build_dir_name(id: &str) -> String {
    id.to_string()
}

/// Build a milestone-level file name
///
/// # Example
/// ```
/// assert_eq!(build_milestone_file_name("M001", "ROADMAP"), "M001-ROADMAP.md");
/// ```
pub fn build_milestone_file_name(milestone_id: &str, suffix: &str) -> String {
    format!("{}-{}.md", milestone_id, suffix)
}

/// Build a slice-level file name
///
/// # Example
/// ```
/// assert_eq!(build_slice_file_name("S01", "PLAN"), "S01-PLAN.md");
/// ```
pub fn build_slice_file_name(slice_id: &str, suffix: &str) -> String {
    format!("{}-{}.md", slice_id, suffix)
}

/// Build a task file name
///
/// # Example
/// ```
/// assert_eq!(build_task_file_name("T01", "PLAN"), "T01-PLAN.md");
/// assert_eq!(build_task_file_name("T03", "SUMMARY"), "T03-SUMMARY.md");
/// ```
pub fn build_task_file_name(task_id: &str, suffix: &str) -> String {
    format!("{}-{}.md", task_id, suffix)
}

// ─── Path Resolution ─────────────────────────────────────────────────────────────

/// Get the .orchestra root directory path
///
/// # Example
/// ```
/// use std::path::Path;
/// let root = orchestra_root(Path::new("/project"));
/// assert_eq!(root, Path::new("/project/.orchestra"));
/// ```
pub fn orchestra_root(base: &Path) -> PathBuf {
    base.join(ORCHESTRA_DIR)
}

/// Get the milestones directory path
///
/// # Example
/// ```
/// use std::path::Path;
/// let dir = milestones_dir(Path::new("/project"));
/// assert_eq!(dir, Path::new("/project/.orchestra/milestones"));
/// ```
pub fn milestones_dir(base: &Path) -> PathBuf {
    orchestra_root(base).join("milestones")
}

/// Resolve a Orchestra root file path (PROJECT.md, DECISIONS.md, etc.)
///
/// Checks canonical name first, then legacy fallback
///
/// # Example
/// ```
/// use std::path::Path;
/// let path = resolve_orchestra_root_file(Path::new("/project"), OrchestraRootFile::Project);
/// // Returns /project/.orchestra/PROJECT.md or /project/.orchestra/project.md
/// ```
pub fn resolve_orchestra_root_file(base: &Path, key: OrchestraRootFile) -> PathBuf {
    let root = orchestra_root(base);
    let canonical = root.join(key.as_str());

    if canonical.exists() {
        return canonical;
    }

    // Legacy fallback
    root.join(key.legacy_name())
}

/// Build relative .orchestra/ path to a root file
///
/// # Example
/// ```
/// use std::path::Path;
/// let rel = rel_orchestra_root_file(OrchestraRootFile::Project);
/// assert_eq!(rel, Path::new(".orchestra/PROJECT.md"));
/// ```
pub fn rel_orchestra_root_file(key: OrchestraRootFile) -> PathBuf {
    PathBuf::from(".orchestra").join(key.as_str())
}

/// Find a directory entry by ID prefix within a parent directory
///
/// Exact match first (M001), then prefix match (M001-SOMETHING)
/// for backward compatibility with legacy descriptor directories.
///
/// Returns the full directory name or None
pub fn resolve_dir(parent_dir: &Path, id_prefix: &str) -> Option<String> {
    if !parent_dir.exists() {
        return None;
    }

    let entries = read_dir_cached(parent_dir).ok()?;

    // Exact match first (current convention: bare ID)
    if entries.contains(&id_prefix.to_string()) {
        return Some(id_prefix.to_string());
    }

    // Prefix match for legacy descriptor dirs: M001-SOMETHING
    for entry in &entries {
        if entry.starts_with(id_prefix) && entry.len() > id_prefix.len() {
            let next_char = entry.chars().nth(id_prefix.len());
            if next_char == Some('-') {
                return Some(entry.clone());
            }
        }
    }

    None
}

/// Find a file by ID prefix and suffix within a directory
///
/// Checks in order:
/// 1. Direct: ID-SUFFIX.md (e.g., M001-ROADMAP.md, T03-PLAN.md)
/// 2. Legacy descriptor: ID-DESCRIPTOR-SUFFIX.md (e.g., T03-INSTALL-PACKAGES-PLAN.md)
/// 3. Legacy bare: suffix.md (e.g., roadmap.md)
pub fn resolve_file(dir: &Path, id_prefix: &str, suffix: &str) -> Option<String> {
    if !dir.exists() {
        return None;
    }

    let target = format!("{}-{}.md", id_prefix, suffix).to_uppercase();
    let entries = read_dir_cached(dir).ok()?;

    // Direct match: ID-SUFFIX.md
    for entry in &entries {
        if entry.to_uppercase() == target {
            return Some(entry.clone());
        }
    }

    // Legacy pattern match: ID-DESCRIPTOR-SUFFIX.md
    let pattern = format!(r"(?i)^{}-.*-{}\.md$", id_prefix, suffix);
    if let Ok(re) = regex::Regex::new(&pattern) {
        for entry in &entries {
            if re.is_match(entry) {
                return Some(entry.clone());
            }
        }
    }

    // Legacy fallback: suffix.md
    let legacy_target = format!("{}.md", suffix.to_lowercase());
    for entry in &entries {
        if entry.to_lowercase() == legacy_target {
            return Some(entry.clone());
        }
    }

    None
}

/// Find all task files matching a pattern in a tasks directory
///
/// Returns sorted file names matching T##-SUFFIX.md or legacy T##-*-SUFFIX.md
pub fn resolve_task_files(tasks_dir: &Path, suffix: &str) -> Vec<String> {
    if !tasks_dir.exists() {
        return Vec::new();
    }

    let entries = match read_dir_cached(tasks_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let current_pattern = format!(r"(?i)^T\d+-{}\.md$", suffix);
    let legacy_pattern = format!(r"(?i)^T\d+-.*-{}\.md$", suffix);

    let current_re = regex::Regex::new(&current_pattern);
    let legacy_re = regex::Regex::new(&legacy_pattern);

    let mut results: Vec<String> = entries
        .into_iter()
        .filter(|entry| {
            current_re
                .as_ref()
                .map(|r| r.is_match(entry))
                .unwrap_or(false)
                || legacy_re
                    .as_ref()
                    .map(|r| r.is_match(entry))
                    .unwrap_or(false)
        })
        .collect();

    results.sort();
    results
}

// ─── Full Path Builders ───────────────────────────────────────────────────────────

/// Resolve the full path to a milestone directory
///
/// Returns None if the milestone doesn't exist
pub fn resolve_milestone_path(base: &Path, milestone_id: &str) -> Option<PathBuf> {
    let milestones_dir = milestones_dir(base);
    let dir_name = resolve_dir(&milestones_dir, milestone_id)?;
    Some(milestones_dir.join(dir_name))
}

/// Resolve the full path to a milestone file (e.g., ROADMAP, CONTEXT, RESEARCH)
pub fn resolve_milestone_file(base: &Path, milestone_id: &str, suffix: &str) -> Option<PathBuf> {
    let m_dir = resolve_milestone_path(base, milestone_id)?;
    let file_name = resolve_file(&m_dir, milestone_id, suffix)?;
    Some(m_dir.join(file_name))
}

/// Resolve the full path to a slice directory within a milestone
pub fn resolve_slice_path(base: &Path, milestone_id: &str, slice_id: &str) -> Option<PathBuf> {
    let m_dir = resolve_milestone_path(base, milestone_id)?;
    let slices_dir = m_dir.join("slices");
    let dir_name = resolve_dir(&slices_dir, slice_id)?;
    Some(slices_dir.join(dir_name))
}

/// Resolve the full path to a slice file (e.g., PLAN, RESEARCH, CONTEXT, SUMMARY)
pub fn resolve_slice_file(
    base: &Path,
    milestone_id: &str,
    slice_id: &str,
    suffix: &str,
) -> Option<PathBuf> {
    let s_dir = resolve_slice_path(base, milestone_id, slice_id)?;
    let file_name = resolve_file(&s_dir, slice_id, suffix)?;
    Some(s_dir.join(file_name))
}

/// Resolve the tasks directory within a slice
pub fn resolve_tasks_dir(base: &Path, milestone_id: &str, slice_id: &str) -> Option<PathBuf> {
    let s_dir = resolve_slice_path(base, milestone_id, slice_id)?;
    let t_dir = s_dir.join("tasks");
    if t_dir.exists() {
        Some(t_dir)
    } else {
        None
    }
}

/// Resolve a specific task file
pub fn resolve_task_file(
    base: &Path,
    milestone_id: &str,
    slice_id: &str,
    task_id: &str,
    suffix: &str,
) -> Option<PathBuf> {
    let t_dir = resolve_tasks_dir(base, milestone_id, slice_id)?;
    let file_name = resolve_file(&t_dir, task_id, suffix)?;
    Some(t_dir.join(file_name))
}

// ─── Relative Path Builders ──────────────────────────────────────────────────────

/// Build relative .orchestra/ path to a milestone directory
///
/// Uses the actual directory name on disk if it exists, otherwise bare ID
pub fn rel_milestone_path(base: &Path, milestone_id: &str) -> PathBuf {
    let milestones_rel = PathBuf::from(".orchestra").join("milestones");
    let dir_name = resolve_dir(&milestones_dir(base), milestone_id)
        .unwrap_or_else(|| milestone_id.to_string());
    milestones_rel.join(dir_name)
}

/// Build relative .orchestra/ path to a milestone file
pub fn rel_milestone_file(base: &Path, milestone_id: &str, suffix: &str) -> PathBuf {
    let m_rel = rel_milestone_path(base, milestone_id);
    let m_dir = resolve_milestone_path(base, milestone_id);

    if let Some(dir) = m_dir {
        if let Some(file) = resolve_file(&dir, milestone_id, suffix) {
            return m_rel.join(file);
        }
    }

    m_rel.join(build_milestone_file_name(milestone_id, suffix))
}

/// Build relative .orchestra/ path to a slice directory
pub fn rel_slice_path(base: &Path, milestone_id: &str, slice_id: &str) -> PathBuf {
    let m_rel = rel_milestone_path(base, milestone_id);
    let m_dir = resolve_milestone_path(base, milestone_id);

    if let Some(dir) = m_dir {
        let slices_dir = dir.join("slices");
        let dir_name = resolve_dir(&slices_dir, slice_id).unwrap_or_else(|| slice_id.to_string());
        return m_rel.join("slices").join(dir_name);
    }

    m_rel.join("slices").join(slice_id)
}

/// Build relative .orchestra/ path to a slice file
pub fn rel_slice_file(base: &Path, milestone_id: &str, slice_id: &str, suffix: &str) -> PathBuf {
    let s_rel = rel_slice_path(base, milestone_id, slice_id);
    let s_dir = resolve_slice_path(base, milestone_id, slice_id);

    if let Some(dir) = s_dir {
        if let Some(file) = resolve_file(&dir, slice_id, suffix) {
            return s_rel.join(file);
        }
    }

    s_rel.join(build_slice_file_name(slice_id, suffix))
}

/// Build relative .orchestra/ path to a task file
pub fn rel_task_file(
    base: &Path,
    milestone_id: &str,
    slice_id: &str,
    task_id: &str,
    suffix: &str,
) -> PathBuf {
    let s_rel = rel_slice_path(base, milestone_id, slice_id);
    let t_dir = resolve_tasks_dir(base, milestone_id, slice_id);

    if let Some(dir) = t_dir {
        if let Some(file) = resolve_file(&dir, task_id, suffix) {
            return s_rel.join("tasks").join(file);
        }
    }

    s_rel
        .join("tasks")
        .join(build_task_file_name(task_id, suffix))
}

// ─── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_orchestra_root() {
        let base = Path::new("/project");
        let root = orchestra_root(base);
        assert_eq!(root, Path::new("/project/.orchestra"));
    }

    #[test]
    fn test_milestones_dir() {
        let base = Path::new("/project");
        let dir = milestones_dir(base);
        assert_eq!(dir, Path::new("/project/.orchestra/milestones"));
    }

    #[test]
    fn test_build_dir_name() {
        assert_eq!(build_dir_name("M001"), "M001");
        assert_eq!(build_dir_name("S01"), "S01");
    }

    #[test]
    fn test_build_milestone_file_name() {
        assert_eq!(
            build_milestone_file_name("M001", "ROADMAP"),
            "M001-ROADMAP.md"
        );
        assert_eq!(
            build_milestone_file_name("M002", "CONTEXT"),
            "M002-CONTEXT.md"
        );
    }

    #[test]
    fn test_build_slice_file_name() {
        assert_eq!(build_slice_file_name("S01", "PLAN"), "S01-PLAN.md");
        assert_eq!(build_slice_file_name("S02", "RESEARCH"), "S02-RESEARCH.md");
    }

    #[test]
    fn test_build_task_file_name() {
        assert_eq!(build_task_file_name("T01", "PLAN"), "T01-PLAN.md");
        assert_eq!(build_task_file_name("T03", "SUMMARY"), "T03-SUMMARY.md");
    }

    #[test]
    fn test_resolve_dir_exact_match() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().join("milestones");
        fs::create_dir_all(&parent).unwrap();

        // Create exact match directory
        let m_dir = parent.join("M001");
        fs::create_dir(&m_dir).unwrap();

        let result = resolve_dir(&parent, "M001");
        assert_eq!(result, Some("M001".to_string()));
    }

    #[test]
    fn test_resolve_dir_prefix_match() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().join("milestones");
        fs::create_dir_all(&parent).unwrap();

        // Create legacy descriptor directory
        let m_dir = parent.join("M001-DESCRIPTOR");
        fs::create_dir(&m_dir).unwrap();

        let result = resolve_dir(&parent, "M001");
        assert_eq!(result, Some("M001-DESCRIPTOR".to_string()));
    }

    #[test]
    fn test_resolve_dir_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path().join("milestones");
        fs::create_dir_all(&parent).unwrap();

        let result = resolve_dir(&parent, "M001");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_file_exact_match() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path()).unwrap();

        // Create exact match file
        let file_path = temp_dir.path().join("M001-ROADMAP.md");
        fs::write(&file_path, "").unwrap();

        let result = resolve_file(temp_dir.path(), "M001", "ROADMAP");
        assert_eq!(result, Some("M001-ROADMAP.md".to_string()));
    }

    #[test]
    fn test_resolve_file_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path()).unwrap();

        // Create lowercase file
        let file_path = temp_dir.path().join("m001-roadmap.md");
        fs::write(&file_path, "").unwrap();

        let result = resolve_file(temp_dir.path(), "M001", "ROADMAP");
        assert_eq!(result, Some("m001-roadmap.md".to_string()));
    }

    #[test]
    fn test_resolve_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path()).unwrap();

        let result = resolve_file(temp_dir.path(), "M001", "ROADMAP");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_task_files() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Create task files
        fs::write(tasks_dir.join("T01-PLAN.md"), "").unwrap();
        fs::write(tasks_dir.join("T02-PLAN.md"), "").unwrap();
        fs::write(tasks_dir.join("T03-SUMMARY.md"), "").unwrap();

        let results = resolve_task_files(&tasks_dir, "PLAN");
        assert_eq!(
            results,
            vec!["T01-PLAN.md".to_string(), "T02-PLAN.md".to_string()]
        );
    }

    #[test]
    fn test_resolve_milestone_path() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let milestones = base.join(".orchestra").join("milestones");
        fs::create_dir_all(&milestones).unwrap();
        fs::create_dir(milestones.join("M001")).unwrap();

        let result = resolve_milestone_path(base, "M001");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("M001"));
    }

    #[test]
    fn test_resolve_slice_path() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let m_dir = base.join(".orchestra").join("milestones").join("M001");
        fs::create_dir_all(&m_dir).unwrap();
        let s_dir = m_dir.join("slices").join("S01");
        fs::create_dir_all(&s_dir).unwrap();

        let result = resolve_slice_path(base, "M001", "S01");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("S01"));
    }

    #[test]
    fn test_resolve_tasks_dir() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let s_dir = base
            .join(".orchestra")
            .join("milestones")
            .join("M001")
            .join("slices")
            .join("S01");
        fs::create_dir_all(&s_dir).unwrap();
        let t_dir = s_dir.join("tasks");
        fs::create_dir(&t_dir).unwrap();

        let result = resolve_tasks_dir(base, "M001", "S01");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("tasks"));
    }

    #[test]
    fn test_resolve_task_file() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let t_dir = base
            .join(".orchestra")
            .join("milestones")
            .join("M001")
            .join("slices")
            .join("S01")
            .join("tasks");
        fs::create_dir_all(&t_dir).unwrap();
        fs::write(t_dir.join("T01-PLAN.md"), "").unwrap();

        let result = resolve_task_file(base, "M001", "S01", "T01", "PLAN");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("T01-PLAN.md"));
    }

    #[test]
    fn test_rel_milestone_path() {
        let base = Path::new("/project");
        let rel = rel_milestone_path(base, "M001");
        assert_eq!(rel, PathBuf::from(".orchestra/milestones/M001"));
    }

    #[test]
    fn test_rel_slice_path() {
        let base = Path::new("/project");
        let rel = rel_slice_path(base, "M001", "S01");
        assert_eq!(rel, PathBuf::from(".orchestra/milestones/M001/slices/S01"));
    }

    #[test]
    fn test_rel_task_file() {
        let base = Path::new("/project");
        let rel = rel_task_file(base, "M001", "S01", "T01", "PLAN");
        assert_eq!(
            rel,
            PathBuf::from(".orchestra/milestones/M001/slices/S01/tasks/T01-PLAN.md")
        );
    }

    #[test]
    fn test_clear_path_cache() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        let dir = base.join(".orchestra").join("milestones");
        fs::create_dir_all(&dir).unwrap();

        // Populate cache
        let _ = read_dir_cached(&dir);

        clear_path_cache();

        // Cache should be empty
        let cache = get_dir_cache().lock().unwrap_or_else(|e| e.into_inner());
        assert!(cache.is_empty());
    }
}
