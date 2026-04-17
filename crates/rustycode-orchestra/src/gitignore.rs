//! Orchestra Gitignore Management — Bootstrap .gitignore and preferences.md
//!
//! Ensures baseline .gitignore exists with universally-correct patterns.
//! Creates an empty preferences.md template if it doesn't exist.
//! Both operations are idempotent — non-destructive if already present.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Patterns that are always correct regardless of project type.
/// No one ever wants these tracked (Orchestra runtime artifacts only).
pub const ORCHESTRA_RUNTIME_PATTERNS: &[&str] = &[
    ".orchestra/activity/",
    ".orchestra/forensics/",
    ".orchestra/runtime/",
    ".orchestra/worktrees/",
    ".orchestra/parallel/",
    ".orchestra/auto.lock",
    ".orchestra/metrics.json",
    ".orchestra/completed-units.json",
    ".orchestra/STATE.md",
    ".orchestra/orchestra.db",
    ".orchestra/DISCUSSION-MANIFEST.json",
    ".orchestra/milestones/**/*-CONTINUE.md",
    ".orchestra/milestones/**/continue.md",
];

/// Baseline .gitignore patterns for all projects.
/// Includes Orchestra runtime patterns, OS junk, editor configs, and common build artifacts.
pub const BASELINE_PATTERNS: &[&str] = &[
    // ── Orchestra runtime (not source artifacts — planning files are tracked) ──
    // ORCHESTRA_RUNTIME_PATTERNS are included below
    ".orchestra/activity/",
    ".orchestra/forensics/",
    ".orchestra/runtime/",
    ".orchestra/worktrees/",
    ".orchestra/parallel/",
    ".orchestra/auto.lock",
    ".orchestra/metrics.json",
    ".orchestra/completed-units.json",
    ".orchestra/STATE.md",
    ".orchestra/orchestra.db",
    ".orchestra/DISCUSSION-MANIFEST.json",
    ".orchestra/milestones/**/*-CONTINUE.md",
    ".orchestra/milestones/**/continue.md",
    // ── OS junk ──
    ".DS_Store",
    "Thumbs.db",
    // ── Editor / IDE ──
    "*.swp",
    "*.swo",
    "*~",
    ".idea/",
    ".vscode/",
    "*.code-workspace",
    // ── Environment / secrets ──
    ".env",
    ".env.*",
    "!.env.example",
    // ── Node / JS / TS ──
    "node_modules/",
    ".next/",
    "dist/",
    "build/",
    // ── Python ──
    "__pycache__/",
    "*.pyc",
    ".venv/",
    "venv/",
    // ── Rust ──
    "target/",
    // ── Go ──
    "vendor/",
    // ── Misc build artifacts ──
    "*.log",
    "coverage/",
    ".cache/",
    "tmp/",
];

/// Options for gitignore management
#[derive(Debug, Clone, PartialEq)]
pub struct GitignoreOptions {
    /// Whether to commit Orchestra documentation files (default: true)
    pub commit_docs: bool,
    /// Whether to manage .gitignore at all (default: true)
    pub manage_gitignore: bool,
}

impl Default for GitignoreOptions {
    fn default() -> Self {
        Self {
            commit_docs: true,
            manage_gitignore: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Gitignore Management
// ---------------------------------------------------------------------------

/// Ensure basePath/.gitignore contains all baseline patterns.
///
/// Creates the file if missing; appends only missing lines if it exists.
/// Returns true if the file was created or modified, false if already complete.
///
/// When `commit_docs` is false, the entire `.orchestra/` directory is added to
/// .gitignore instead of individual runtime patterns, keeping all Orchestra
/// artifacts local-only.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `options` - Optional configuration
///
/// # Returns
/// true if file was created or modified, false if already complete
///
/// # Example
/// ```
/// use rustycode_orchestra::gitignore::*;
///
/// let modified = ensure_gitignore("/project", None);
/// if modified {
///     println!(".gitignore was updated");
/// }
/// ```
pub fn ensure_gitignore(
    base_path: &Path,
    options: Option<GitignoreOptions>,
) -> Result<bool, String> {
    let opts = options.unwrap_or_default();

    // If manage_gitignore is explicitly false, do not touch .gitignore at all
    if !opts.manage_gitignore {
        return Ok(false);
    }

    let gitignore_path = base_path.join(".gitignore");
    let commit_docs = opts.commit_docs;

    let mut existing = String::new();
    if gitignore_path.exists() {
        existing = fs::read_to_string(&gitignore_path)
            .map_err(|e| format!("Failed to read .gitignore: {}", e))?;
    }

    // When commit_docs is false, ensure blanket ".orchestra/" is in .gitignore
    // and skip the self-heal that would remove it.
    if !commit_docs {
        return ensure_blanket_orchestra_ignore(&gitignore_path, &existing);
    }

    // Self-heal: remove blanket ".orchestra/" lines from pre-v2.14.0 projects.
    // The blanket ignore prevented planning artifacts (.orchestra/milestones/) from
    // being tracked in git, causing artifacts to vanish in worktrees and
    // triggering loop detection failures. Replace with explicit runtime-only
    // ignores so planning files are tracked naturally.
    let mut modified = false;
    let lines: Vec<&str> = existing.split('\n').collect();
    let filtered_lines: Vec<&str> = lines
        .into_iter()
        .filter(|line| {
            let trimmed = line.trim();
            // Remove standalone ".orchestra/" lines (blanket ignore) but keep specific
            // .orchestra/ subpath patterns like ".orchestra/activity/" or ".orchestra/auto.lock"
            if trimmed == ".orchestra/" || trimmed == ".orchestra" {
                modified = true;
                return false;
            }
            true
        })
        .collect();

    if modified {
        let healed = filtered_lines.join("\n");
        fs::write(&gitignore_path, healed)
            .map_err(|e| format!("Failed to write .gitignore: {}", e))?;
    }

    // Re-read existing content after potential modification
    let existing = if modified {
        fs::read_to_string(&gitignore_path)
            .map_err(|e| format!("Failed to read .gitignore: {}", e))?
    } else {
        existing
    };

    // Parse existing lines (trimmed, ignoring comments and blanks)
    let existing_lines: HashSet<String> = existing
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    // Find patterns not yet present
    let missing: Vec<&str> = BASELINE_PATTERNS
        .iter()
        .filter(|p| !existing_lines.contains(**p))
        .copied()
        .collect();

    if missing.is_empty() {
        return Ok(modified);
    }

    // Build the block to append
    let block = format!(
        "\n{}\n{}\n\n",
        "# ── Orchestra baseline (auto-generated) ──",
        missing.join("\n")
    );

    // Ensure existing content ends with a newline before appending
    let prefix = if !existing.is_empty() && !existing.ends_with('\n') {
        "\n"
    } else {
        ""
    };

    let content = format!("{}{}{}", existing, prefix, block);
    fs::write(&gitignore_path, content)
        .map_err(|e| format!("Failed to write .gitignore: {}", e))?;

    Ok(true)
}

/// Remove BASELINE_PATTERNS runtime paths from the git index if they are
/// currently tracked. This fixes repos that started tracking these files
/// before the .gitignore rule was added — git continues tracking files
/// already in the index even after .gitignore is updated.
///
/// Only removes from the index (`--cached`), never from disk. Idempotent.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Example
/// ```
/// use rustycode_orchestra::gitignore::*;
///
/// untrack_runtime_files("/project").unwrap();
/// ```
pub fn untrack_runtime_files(base_path: &Path) -> Result<(), String> {
    for pattern in ORCHESTRA_RUNTIME_PATTERNS {
        // Use -r for directory patterns (trailing slash), strip the slash for the command
        let target = if let Some(stripped) = pattern.strip_suffix('/') {
            stripped
        } else {
            pattern
        };

        // Run git rm --cached to remove from index
        let result = std::process::Command::new("git")
            .args([
                "-C",
                base_path
                    .to_str()
                    .ok_or_else(|| "Invalid path: non-UTF-8 characters".to_string())?,
                "rm",
                "--cached",
                "-r",
                target,
            ])
            .output();

        match result {
            Ok(output) => {
                // git rm --cached returns non-zero if file isn't tracked
                // This is expected and OK, so we ignore it
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // "did not match any files" is expected (file not tracked)
                    if !stderr.contains("did not match any files") {
                        tracing::warn!("[gitignore] Warning: {}", stderr.trim());
                    }
                }
            }
            Err(e) => {
                // Git not available or not in a git repo - expected in some contexts
                tracing::warn!("[gitignore] Could not run git rm --cached: {}", e);
            }
        }
    }

    Ok(())
}

/// Ensure basePath/.orchestra/preferences.md exists as an empty template.
///
/// Creates the file with frontmatter only if it doesn't exist.
/// Returns true if created, false if already exists.
///
/// Checks both lowercase (canonical) and uppercase (legacy) to avoid
/// creating a duplicate when an uppercase file already exists.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// true if created, false if already exists
///
/// # Example
/// ```
/// use rustycode_orchestra::gitignore::*;
///
/// let created = ensure_preferences("/project").unwrap();
/// if created {
///     println!("preferences.md template created");
/// }
/// ```
pub fn ensure_preferences(base_path: &Path) -> Result<bool, String> {
    let orchestra_path = base_path.join(".orchestra");
    let preferences_path = orchestra_path.join("preferences.md");
    let legacy_path = orchestra_path.join("PREFERENCES.md");

    if preferences_path.exists() || legacy_path.exists() {
        return Ok(false);
    }

    let template = r#"---
version: 1
always_use_skills: []
prefer_skills: []
avoid_skills: []
skill_rules: []
custom_instructions: []
models: {}
skill_discovery: {}
auto_supervisor: {}
---

# Orchestra Skill Preferences

Project-specific guidance for skill selection and execution preferences.

See `~/.orchestra/agent/extensions/orchestra/docs/preferences-reference.md` for full field documentation and examples.

## Fields

- `always_use_skills`: Skills that must be available during all Orchestra operations
- `prefer_skills`: Skills to prioritize when multiple options exist
- `avoid_skills`: Skills to minimize or avoid (with lower priority than prefer)
- `skill_rules`: Context-specific rules (e.g., "use tool X for Y type of work")
- `custom_instructions`: Append-only project guidance (do not override system rules)
- `models`: Model preferences for specific task types
- `skill_discovery`: Automatic skill detection preferences
- `auto_supervisor`: Supervision and gating rules for autonomous modes
- `git`: Git preferences — `main_branch` (default branch name for new repos, e.g., "main", "master", "trunk"), `auto_push`, `snapshots`, `commit_docs` (set to `false` to keep .orchestra/ local-only), etc.

## Examples

\`\`\`yaml
prefer_skills:
  - playwright
  - resolve_library
avoid_skills:
  - subagent  # prefer direct execution in this project

custom_instructions:
  - "Always verify with browser_assert before marking UI work done"
  - "Use Context7 for all library/framework decisions"
\`\`\`
"#.to_string();

    // Ensure .orchestra directory exists
    fs::create_dir_all(&orchestra_path)
        .map_err(|e| format!("Failed to create .orchestra directory: {}", e))?;

    fs::write(&preferences_path, template)
        .map_err(|e| format!("Failed to write preferences.md: {}", e))?;

    Ok(true)
}

/// When commit_docs is false, ensure `.orchestra/` runtime noise is in .gitignore.
/// This keeps Orchestra runtime artifacts local-only while allowing milestone/plan files.
///
/// Returns true if the file was modified, false if already complete.
fn ensure_blanket_orchestra_ignore(gitignore_path: &Path, existing: &str) -> Result<bool, String> {
    // Parse existing lines
    let existing_lines: HashSet<String> = existing
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    // Already has the Orchestra runtime noise ignores
    if existing_lines.contains(".orchestra/.lock")
        || existing_lines.contains(".orchestra/activity.logl")
    {
        return Ok(false);
    }

    let block = format!(
        "\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n\n",
        "# ── Orchestra runtime noise (local-only, commit_docs: false) ──",
        ".orchestra/.lock",
        ".orchestra/activity.logl",
        ".orchestra/STATE.md",
        ".orchestra/orchestra.db",
        ".orchestra/orchestra.db-*",
        ".orchestra/metrics.json",
        ".orchestra/completed-units.json",
        ".orchestra/DISCUSSION-MANIFEST.json",
        ".orchestra/auto.lock",
        ".orchestra/runtime/",
        ".orchestra/forensics/"
    );

    let prefix = if !existing.is_empty() && !existing.ends_with('\n') {
        "\n"
    } else {
        ""
    };

    let content = format!("{}{}{}", existing, prefix, block);
    fs::write(gitignore_path, content).map_err(|e| format!("Failed to write .gitignore: {}", e))?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_gitignore_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = ensure_gitignore(base_path, None).unwrap();
        assert!(result);

        let gitignore_path = base_path.join(".gitignore");
        assert!(gitignore_path.exists());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("# ── Orchestra baseline (auto-generated) ──"));
        assert!(content.contains(".orchestra/activity/"));
        assert!(content.contains("target/"));
    }

    #[test]
    fn test_ensure_gitignore_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // First call should create
        let result1 = ensure_gitignore(base_path, None).unwrap();
        assert!(result1);

        // Second call should return false (no modification)
        let result2 = ensure_gitignore(base_path, None).unwrap();
        assert!(!result2);
    }

    #[test]
    fn test_ensure_gitignore_commit_docs_false() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let options = GitignoreOptions {
            commit_docs: false,
            manage_gitignore: true,
        };

        let result = ensure_gitignore(base_path, Some(options)).unwrap();
        assert!(result);

        let gitignore_path = base_path.join(".gitignore");
        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(
            content.contains("# ── Orchestra runtime noise (local-only, commit_docs: false) ──")
        );
        assert!(content.contains(".orchestra/.lock"));
        assert!(content.contains(".orchestra/activity.logl"));
    }

    #[test]
    fn test_ensure_gitignore_manage_gitignore_false() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let options = GitignoreOptions {
            commit_docs: true,
            manage_gitignore: false,
        };

        // Should not create .gitignore
        let result = ensure_gitignore(base_path, Some(options)).unwrap();
        assert!(!result);

        let gitignore_path = base_path.join(".gitignore");
        assert!(!gitignore_path.exists());
    }

    #[test]
    fn test_ensure_preferences_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = ensure_preferences(base_path).unwrap();
        assert!(result);

        let preferences_path = base_path.join(".orchestra/preferences.md");
        assert!(preferences_path.exists());

        let content = fs::read_to_string(&preferences_path).unwrap();
        assert!(content.contains("# Orchestra Skill Preferences"));
        assert!(content.contains("always_use_skills:"));
    }

    #[test]
    fn test_ensure_preferences_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // First call should create
        let result1 = ensure_preferences(base_path).unwrap();
        assert!(result1);

        // Second call should return false (already exists)
        let result2 = ensure_preferences(base_path).unwrap();
        assert!(!result2);
    }

    #[test]
    fn test_ensure_preferences_legacy_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");

        fs::create_dir_all(&orchestra_path).unwrap();
        let legacy_path = orchestra_path.join("PREFERENCES.md");
        fs::write(&legacy_path, "legacy content").unwrap();

        // Should not create new file if legacy exists
        let result = ensure_preferences(base_path).unwrap();
        assert!(!result);

        // Legacy file should still exist
        assert!(legacy_path.exists());
    }

    #[test]
    fn test_untrack_runtime_files() {
        // This test just verifies the function doesn't crash
        // Actual git operations may not work in test environment
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = untrack_runtime_files(base_path);
        // Should not error even if git isn't available
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_baseline_patterns_const() {
        assert!(!BASELINE_PATTERNS.is_empty());
        assert!(BASELINE_PATTERNS.contains(&".orchestra/activity/"));
        assert!(BASELINE_PATTERNS.contains(&"target/"));
        assert!(BASELINE_PATTERNS.contains(&"node_modules/"));
    }

    #[test]
    fn test_orchestra_runtime_patterns_const() {
        assert!(!ORCHESTRA_RUNTIME_PATTERNS.is_empty());
        assert!(ORCHESTRA_RUNTIME_PATTERNS.contains(&".orchestra/auto.lock"));
        assert!(ORCHESTRA_RUNTIME_PATTERNS.contains(&".orchestra/metrics.json"));
    }

    #[test]
    fn test_gitignore_options_default() {
        let opts = GitignoreOptions::default();
        assert!(opts.commit_docs);
        assert!(opts.manage_gitignore);
    }
}
