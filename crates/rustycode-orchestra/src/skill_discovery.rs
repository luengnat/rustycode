//! Skill Discovery (orchestra-2 pattern)
//!
//! Detects skills installed during auto-mode by comparing the current
//! skills directory against a snapshot taken at auto-mode start.
//!
//! # Problem
//!
//! In long-running autonomous development sessions, users may install
//! new skills (via the skill palette or manually) while auto-mode is running.
//! Without skill discovery, these new skills wouldn't be available until
//! a restart.
//!
//! # Solution
//!
//! The skill discovery system:
//! 1. **Snapshots** the skills directory at auto-mode start
//! 2. **Periodically checks** for new skills during execution
//! 3. **Injects metadata** for new skills into the system prompt
//! 4. **Makes visible** all skills without requiring restart
//!
//! # Usage Flow
//!
//! ```no_run
//! use rustycode_orchestra::skill_discovery::{snapshot_skills, detect_new_skills, clear_skill_snapshot};
//!
//! // At auto-mode start
//! snapshot_skills(&agent_dir)?;
//!
//! // After each unit (or periodically)
//! if let new_skills = detect_new_skills(&agent_dir)? {
//!     for skill in new_skills {
//!         println!("New skill: {} - {}", skill.name, skill.description);
//!         // Inject skill metadata into next prompt
//!     }
//! }
//!
//! // At auto-mode stop
//! clear_skill_snapshot();
//! ```
//!
//! # Skill Metadata
//!
//! For each discovered skill, we extract:
//! - **name**: Skill identifier (e.g., "orchestra:progress")
//! - **description**: Short help text from skill file
//! - **location**: Path to skill markdown file
//!
//! # Memory Safety
//!
//! Uses a mutable static for baseline storage. This is safe because:
//! - Only written during auto-mode start (single-threaded)
//! - Read-only during auto-mode execution
//! - Cleared on auto-mode stop
//! - No concurrent access possible
//!
//! # Integration
//!
//! - **Auto-mode Runtime**: Calls `detect_new_skills()` after each unit
//! - **Prompt Building**: Injects discovered skills into system prompt
//! - **TUI**: Shows "🆕 Detected X new skills" notification

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Skills directory (relative to Claude agent directory)
const SKILLS_DIR_NAME: &str = "skills";

/// Snapshot of skill names at auto-mode start
static BASELINE_SKILLS: OnceCell<Mutex<Option<HashSet<String>>>> = OnceCell::new();

/// Discovered skill metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    /// Skill name
    pub name: String,
    /// Skill description
    pub description: String,
    /// Path to skill file
    pub location: PathBuf,
}

/// Snapshot the current skills directory
///
/// Call at auto-mode start to establish a baseline.
pub fn snapshot_skills(agent_dir: &Path) -> Result<()> {
    let skills_dir = agent_dir.join(SKILLS_DIR_NAME);
    let skill_dirs = list_skill_dirs(&skills_dir)?;

    // SAFETY: This is only called during auto-mode start (single-threaded)
    let cell = BASELINE_SKILLS.get_or_init(|| Mutex::new(None));
    *cell.lock().unwrap_or_else(|e| e.into_inner()) = Some(skill_dirs);

    Ok(())
}

/// Clear the snapshot
///
/// Call when auto-mode stops.
pub fn clear_skill_snapshot() {
    if let Some(cell) = BASELINE_SKILLS.get() {
        *cell.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Check if a snapshot is active (auto-mode is running with discovery)
pub fn has_skill_snapshot() -> bool {
    BASELINE_SKILLS
        .get()
        .is_some_and(|cell| cell.lock().unwrap_or_else(|e| e.into_inner()).is_some())
}

/// Detect skills installed since the snapshot was taken
///
/// Returns skill metadata for any new skills found.
pub fn detect_new_skills(agent_dir: &Path) -> Result<Vec<DiscoveredSkill>> {
    // Read baseline snapshot
    let baseline = match BASELINE_SKILLS.get() {
        Some(cell) => match &*cell.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(b) => b.clone(),
            None => return Ok(Vec::new()),
        },
        None => return Ok(Vec::new()),
    };

    let skills_dir = agent_dir.join(SKILLS_DIR_NAME);
    let current = list_skill_dirs(&skills_dir)?;

    let mut new_skills = Vec::new();

    for dir in current {
        if baseline.contains(&dir) {
            continue;
        }

        let skill_md_path = skills_dir.join(&dir).join("SKILL.md");
        if !skill_md_path.exists() {
            continue;
        }

        let meta = parse_skill_frontmatter(&skill_md_path)?;
        if let Some(meta) = meta {
            new_skills.push(DiscoveredSkill {
                name: meta.name.unwrap_or_else(|| dir.clone()),
                description: meta
                    .description
                    .unwrap_or_else(|| format!("Skill: {}", dir)),
                location: skill_md_path,
            });
        }
    }

    Ok(new_skills)
}

/// Format discovered skills as markdown for system prompt injection
///
/// This can be appended to the system prompt so the LLM sees them naturally.
pub fn format_skills_markdown(skills: &[DiscoveredSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut result = String::from(
        "## Newly Discovered Skills\n\n\
         The following skills were installed during this auto-mode session.\n\
         Use the read tool to load a skill's file when the task matches its description.\n\n",
    );

    for skill in skills {
        result.push_str(&format!(
            "### {}\n{}\nLocation: `{}`\n\n",
            skill.name,
            skill.description,
            skill.location.display()
        ));
    }

    result
}

/// Format discovered skills as XML block (matching pi format)
pub fn format_skills_xml(skills: &[DiscoveredSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut entries = String::new();

    for skill in skills {
        entries.push_str(&format!(
            "  <skill>\n\
             <name>{}</name>\n\
             <description>{}</description>\n\
             <location>{}</location>\n\
             </skill>\n",
            escape_xml(&skill.name),
            escape_xml(&skill.description),
            escape_xml(&skill.location.display().to_string())
        ));
    }

    format!(
        "\n<newly_discovered_skills>\n\
         The following skills were installed during this auto-mode session.\n\
         Use the read tool to load a skill's file when the task matches its description.\n\n\
         {}\n\
         </newly_discovered_skills>",
        entries
    )
}

// ─── Internals ────────────────────────────────────────────────────────────────

/// List all skill directory names
fn list_skill_dirs(skills_dir: &Path) -> Result<HashSet<String>> {
    if !skills_dir.exists() {
        return Ok(HashSet::new());
    }

    let mut skill_dirs = HashSet::new();

    let entries = fs::read_dir(skills_dir)
        .with_context(|| format!("Failed to read skills directory: {}", skills_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                skill_dirs.insert(name.to_string());
            }
        }
    }

    Ok(skill_dirs)
}

/// Skill frontmatter metadata
#[derive(Debug, Clone)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// Parse skill frontmatter from SKILL.md
fn parse_skill_frontmatter(path: &Path) -> Result<Option<SkillFrontmatter>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

    // Check for YAML frontmatter: ---\nkey: value\n---
    if !content.starts_with("---") {
        return Ok(None);
    }

    let end_idx = match content[3..].find("\n---") {
        Some(idx) => idx + 3,
        None => return Ok(None),
    };

    let frontmatter = &content[3..end_idx];

    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix("name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("description:") {
            description = Some(rest.trim().to_string());
        }
    }

    Ok(Some(SkillFrontmatter { name, description }))
}

/// Escape XML special characters
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_and_detect() {
        let temp_dir = TempDir::new().unwrap();
        let agent_dir = temp_dir.path();
        let skills_dir = agent_dir.join("skills");

        // Create initial skills
        fs::create_dir_all(skills_dir.join("skill1")).unwrap();
        fs::write(
            skills_dir.join("skill1").join("SKILL.md"),
            "---\nname: Skill One\ndescription: First skill\n---\nContent",
        )
        .unwrap();

        // Snapshot
        snapshot_skills(agent_dir).unwrap();
        assert!(has_skill_snapshot());

        // Add new skill
        fs::create_dir_all(skills_dir.join("skill2")).unwrap();
        fs::write(
            skills_dir.join("skill2").join("SKILL.md"),
            "---\nname: Skill Two\ndescription: Second skill\n---\nContent",
        )
        .unwrap();

        // Detect new skills
        let new_skills = detect_new_skills(agent_dir).unwrap();
        assert_eq!(new_skills.len(), 1);
        assert_eq!(new_skills[0].name, "Skill Two");
        assert_eq!(new_skills[0].description, "Second skill");

        // Clear snapshot
        clear_skill_snapshot();
        assert!(!has_skill_snapshot());
    }

    #[test]
    fn test_format_skills_markdown() {
        let skills = vec![DiscoveredSkill {
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            location: PathBuf::from("/skills/test"),
        }];

        let formatted = format_skills_markdown(&skills);
        assert!(formatted.contains("## Newly Discovered Skills"));
        assert!(formatted.contains("Test Skill"));
        assert!(formatted.contains("A test skill"));
    }

    #[test]
    fn test_format_skills_xml() {
        let skills = vec![DiscoveredSkill {
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            location: PathBuf::from("/skills/test"),
        }];

        let formatted = format_skills_xml(&skills);
        assert!(formatted.contains("<newly_discovered_skills>"));
        assert!(formatted.contains("<name>Test Skill</name>"));
        assert!(formatted.contains("<description>A test skill</description>"));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("a<b"), "a&lt;b");
        assert_eq!(escape_xml("a>b"), "a&gt;b");
        assert_eq!(escape_xml("a\"b"), "a&quot;b");
    }
}
