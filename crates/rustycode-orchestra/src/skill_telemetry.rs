//! Orchestra Skill Telemetry — Track which skills are loaded per unit
//!
//! Captures skill names at dispatch time for inclusion in UnitMetrics.
//! Distinguishes between "available" skills (in system prompt) and
//! "actively loaded" skills (read via tool calls during execution).
//!
//! Data flow:
//!   1. At dispatch, capture_available_skills() records skills from the system prompt
//!   2. During execution, record_skill_read() tracks explicit SKILL.md reads
//!   3. At unit completion, get_and_clear_skills() returns the loaded list for metrics
//!
//! Matches orchestra-2's skill-telemetry.ts implementation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Skill usage data with timestamp
#[derive(Debug, Clone)]
pub struct SkillUsage {
    pub skill_name: String,
    pub last_used_timestamp: u64,
}

// ─── State ─────────────────────────────────────────────────────────────────────

/// Global skill telemetry state
struct SkillTelemetryState {
    /// Skills available in the system prompt for the current unit
    available_skills: Vec<String>,
    /// Skills explicitly read (SKILL.md loaded) during the current unit
    actively_loaded_skills: HashSet<String>,
    /// Agent directory for skills
    agent_dir: Option<PathBuf>,
}

impl SkillTelemetryState {
    fn new() -> Self {
        Self {
            available_skills: Vec::new(),
            actively_loaded_skills: HashSet::new(),
            agent_dir: None,
        }
    }
}

/// Global state
static STATE: OnceLock<Mutex<SkillTelemetryState>> = OnceLock::new();

fn state() -> &'static Mutex<SkillTelemetryState> {
    STATE.get_or_init(|| Mutex::new(SkillTelemetryState::new()))
}

// ─── Configuration ─────────────────────────────────────────────────────────────

/// Set the agent directory for skills
pub fn set_agent_dir(dir: PathBuf) {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    s.agent_dir = Some(dir);
}

/// Get the agent directory
pub fn get_agent_dir() -> Option<PathBuf> {
    let s = state().lock().unwrap_or_else(|e| e.into_inner());
    s.agent_dir.clone()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Capture the list of available skill names at dispatch time
///
/// Called before each unit starts.
pub fn capture_available_skills() {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

    let skills_dir = if let Some(ref agent_dir) = s.agent_dir {
        agent_dir.join("skills")
    } else {
        // Default to ~/.claude/skills if no agent dir set
        let home = std::env::var("HOME").unwrap_or(".".to_string());
        PathBuf::from(home).join(".claude").join("skills")
    };

    s.available_skills = list_skill_names(&skills_dir);
    s.actively_loaded_skills.clear();
}

/// Record that a skill was actively loaded (its SKILL.md was read)
///
/// Call this when the agent reads a SKILL.md file.
pub fn record_skill_read(skill_name: &str) {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    s.actively_loaded_skills.insert(skill_name.to_string());
}

/// Get the skill names for the current unit and clear state
///
/// Returns actively loaded skills if any, otherwise available skills.
pub fn get_and_clear_skills() -> Vec<String> {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

    let result = if !s.actively_loaded_skills.is_empty() {
        s.actively_loaded_skills.iter().cloned().collect()
    } else {
        s.available_skills.clone()
    };

    s.available_skills.clear();
    s.actively_loaded_skills.clear();
    result
}

/// Reset all telemetry state
pub fn reset_skill_telemetry() {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    s.available_skills.clear();
    s.actively_loaded_skills.clear();
    s.agent_dir = None;
}

/// Get last-used timestamps for all skills from metrics data
pub fn get_skill_last_used(units: &[SkillUsage]) -> HashMap<String, u64> {
    let mut last_used = HashMap::new();

    for unit in units {
        let existing = last_used.get(&unit.skill_name).copied().unwrap_or(0);
        if unit.last_used_timestamp > existing {
            last_used.insert(unit.skill_name.clone(), unit.last_used_timestamp);
        }
    }

    last_used
}

/// Detect stale skills — those not used within the given threshold
pub fn detect_stale_skills(units: &[SkillUsage], threshold_days: i64) -> Vec<String> {
    if threshold_days <= 0 {
        return Vec::new();
    }

    let last_used = get_skill_last_used(units);
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        - (threshold_days * 24 * 60 * 60 * 1000) as u64;

    let s = state().lock().unwrap_or_else(|e| e.into_inner());
    let skills_dir = if let Some(ref agent_dir) = s.agent_dir {
        agent_dir.join("skills")
    } else {
        let home = std::env::var("HOME").unwrap_or(".".to_string());
        PathBuf::from(home).join(".claude").join("skills")
    };

    let installed = list_skill_names(&skills_dir);
    let mut stale = Vec::new();

    for skill in &installed {
        let last_ts = last_used.get(skill).copied().unwrap_or(0);
        if last_ts == 0 || last_ts < cutoff {
            stale.push(skill.clone());
        }
    }

    stale
}

// ─── Internals ────────────────────────────────────────────────────────────────

/// List skill names from a skills directory
fn list_skill_names(skills_dir: &Path) -> Vec<String> {
    if !skills_dir.exists() {
        return Vec::new();
    }

    let entries = match fs::read_dir(skills_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| !name.starts_with('.'))
                .unwrap_or(false)
        })
        .filter_map(|entry| -> Option<String> {
            let skill_md = entry.path().join("SKILL.md");
            if skill_md.exists() {
                entry.file_name().to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_list_skill_names_empty_dir() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");

        let names = list_skill_names(&skills_dir);
        assert!(names.is_empty());
    }

    #[test]
    fn test_list_skill_names_with_skills() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create skill directories with SKILL.md
        let skill1 = skills_dir.join("skill1");
        let skill2 = skills_dir.join("skill2");
        fs::create_dir(&skill1).unwrap();
        fs::create_dir(&skill2).unwrap();
        fs::write(skill1.join("SKILL.md"), "# Skill 1").unwrap();
        fs::write(skill2.join("SKILL.md"), "# Skill 2").unwrap();

        let names = list_skill_names(&skills_dir);
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"skill1".to_string()));
        assert!(names.contains(&"skill2".to_string()));
    }

    #[test]
    fn test_list_skill_names_ignores_hidden() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create hidden directory
        let hidden = skills_dir.join(".hidden");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("SKILL.md"), "# Hidden").unwrap();

        let names = list_skill_names(&skills_dir);
        assert!(names.is_empty());
    }

    #[test]
    fn test_list_skill_names_requires_skill_md() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create directory without SKILL.md
        let skill1 = skills_dir.join("skill1");
        fs::create_dir(&skill1).unwrap();

        let names = list_skill_names(&skills_dir);
        assert!(names.is_empty());
    }

    #[test]
    fn test_capture_and_get_available_skills() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Reset state to ensure clean test
        reset_skill_telemetry();

        let temp_dir = TempDir::new().unwrap();
        set_agent_dir(temp_dir.path().to_path_buf());

        // Create skills directory
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill1 = skills_dir.join("test-skill");
        fs::create_dir(&skill1).unwrap();
        fs::write(skill1.join("SKILL.md"), "# Test").unwrap();

        capture_available_skills();

        let skills = get_and_clear_skills();
        assert_eq!(skills.len(), 1);
        assert!(skills.contains(&"test-skill".to_string()));
    }

    #[test]
    fn test_record_skill_read() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        reset_skill_telemetry();

        record_skill_read("tdd-guide");
        record_skill_read("code-reviewer");

        let skills = get_and_clear_skills();
        assert_eq!(skills.len(), 2);
        assert!(skills.contains(&"tdd-guide".to_string()));
        assert!(skills.contains(&"code-reviewer".to_string()));
    }

    #[test]
    fn test_actively_loaded_takes_precedence() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        reset_skill_telemetry();

        let temp_dir = TempDir::new().unwrap();
        set_agent_dir(temp_dir.path().to_path_buf());

        // Create skills directory
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill1 = skills_dir.join("available-skill");
        fs::create_dir(&skill1).unwrap();
        fs::write(skill1.join("SKILL.md"), "# Available").unwrap();

        capture_available_skills();
        record_skill_read("actively-loaded");

        let skills = get_and_clear_skills();
        assert_eq!(skills.len(), 1);
        assert!(skills.contains(&"actively-loaded".to_string()));
        assert!(!skills.contains(&"available-skill".to_string()));
    }

    #[test]
    fn test_reset_skill_telemetry() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        record_skill_read("test-skill");
        reset_skill_telemetry();

        let skills = get_and_clear_skills();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_get_skill_last_used() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let units = vec![
            SkillUsage {
                skill_name: "skill1".to_string(),
                last_used_timestamp: 1000,
            },
            SkillUsage {
                skill_name: "skill1".to_string(),
                last_used_timestamp: 2000,
            },
            SkillUsage {
                skill_name: "skill2".to_string(),
                last_used_timestamp: 1500,
            },
        ];

        let last_used = get_skill_last_used(&units);
        assert_eq!(last_used.get("skill1"), Some(&2000));
        assert_eq!(last_used.get("skill2"), Some(&1500));
    }

    #[test]
    fn test_detect_stale_skills_zero_threshold() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let units = vec![SkillUsage {
            skill_name: "skill1".to_string(),
            last_used_timestamp: 1000,
        }];

        let stale = detect_stale_skills(&units, 0);
        assert!(stale.is_empty());
    }

    #[test]
    fn test_detect_stale_skills_negative_threshold() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let units = vec![];

        let stale = detect_stale_skills(&units, -1);
        assert!(stale.is_empty());
    }

    #[test]
    fn test_detect_stale_skills_old_skill() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        set_agent_dir(temp_dir.path().to_path_buf());

        // Create skills directory with one skill
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill1 = skills_dir.join("old-skill");
        fs::create_dir(&skill1).unwrap();
        fs::write(skill1.join("SKILL.md"), "# Old").unwrap();

        // Create usage data with old timestamp (more than 1 day ago)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let units = vec![SkillUsage {
            skill_name: "old-skill".to_string(),
            last_used_timestamp: now - (2 * 24 * 60 * 60 * 1000), // 2 days ago
        }];

        let stale = detect_stale_skills(&units, 1);
        assert!(stale.contains(&"old-skill".to_string()));
    }

    #[test]
    fn test_detect_stale_skills_recent_skill() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        set_agent_dir(temp_dir.path().to_path_buf());

        // Create skills directory with one skill
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let skill1 = skills_dir.join("recent-skill");
        fs::create_dir(&skill1).unwrap();
        fs::write(skill1.join("SKILL.md"), "# Recent").unwrap();

        // Create usage data with recent timestamp (less than 1 day ago)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let units = vec![SkillUsage {
            skill_name: "recent-skill".to_string(),
            last_used_timestamp: now - (12 * 60 * 60 * 1000), // 12 hours ago
        }];

        let stale = detect_stale_skills(&units, 1);
        assert!(!stale.contains(&"recent-skill".to_string()));
    }
}
