//! Orchestra Detection — Project state and ecosystem detection
//!
//! Pure functions, zero UI dependencies, zero side effects.
//! Used to determine what onboarding flow to show when entering
//! a project directory.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// What kind of Orchestra state exists in this directory
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum OrchestraProjectState {
    /// No Orchestra state found
    None,
    /// V1 .planning/ directory detected
    V1Planning,
    /// V2 .orchestra/ with milestones
    V2Orchestra,
    /// V2 .orchestra/ exists but empty
    V2OrchestraEmpty,
}

/// Full project detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetection {
    /// What kind of Orchestra state exists in this directory
    pub state: OrchestraProjectState,

    /// Is this the first time Orchestra has been used on this machine?
    pub is_first_ever_launch: bool,

    /// Does ~/.orchestra/ exist with preferences?
    pub has_global_setup: bool,

    /// v1 details (only when state === V1Planning)
    pub v1: Option<V1Detection>,

    /// v2 details (only when state === V2Orchestra or V2OrchestraEmpty)
    pub v2: Option<V2Detection>,

    /// Detected project ecosystem signals
    pub project_signals: ProjectSignals,
}

/// V1 Orchestra detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V1Detection {
    pub path: String,
    pub has_phases_dir: bool,
    pub has_roadmap: bool,
    pub phase_count: usize,
}

/// V2 Orchestra detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2Detection {
    pub milestone_count: usize,
    pub has_preferences: bool,
    pub has_context: bool,
}

/// Detected project ecosystem signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSignals {
    /// Detected project/package files
    pub detected_files: Vec<String>,

    /// Is this already a git repo?
    pub is_git_repo: bool,

    /// Is this a monorepo?
    pub is_monorepo: bool,

    /// Primary language hint
    pub primary_language: Option<String>,

    /// Has existing CI configuration?
    pub has_ci: bool,

    /// Has existing test setup?
    pub has_tests: bool,

    /// Detected package manager
    pub package_manager: Option<String>,

    /// Auto-detected verification commands
    pub verification_commands: Vec<String>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Project file markers that indicate a project exists
pub const PROJECT_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "Gemfile",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "CMakeLists.txt",
    "Makefile",
    "composer.json",
    "pubspec.yaml",
    "Package.swift",
    "mix.exs",
    "deno.json",
    "deno.jsonc",
];

/// Language mapping for project files
pub fn language_map() -> &'static HashMap<&'static str, &'static str> {
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("package.json", "javascript/typescript");
        m.insert("Cargo.toml", "rust");
        m.insert("go.mod", "go");
        m.insert("pyproject.toml", "python");
        m.insert("setup.py", "python");
        m.insert("Gemfile", "ruby");
        m.insert("pom.xml", "java");
        m.insert("build.gradle", "java/kotlin");
        m.insert("build.gradle.kts", "kotlin");
        m.insert("CMakeLists.txt", "c/c++");
        m.insert("composer.json", "php");
        m.insert("pubspec.yaml", "dart/flutter");
        m.insert("Package.swift", "swift");
        m.insert("mix.exs", "elixir");
        m.insert("deno.json", "typescript/deno");
        m.insert("deno.jsonc", "typescript/deno");
        m
    })
}

/// Monorepo marker files
pub const MONOREPO_MARKERS: &[&str] =
    &["lerna.json", "nx.json", "turbo.json", "pnpm-workspace.yaml"];

/// CI configuration markers
pub const CI_MARKERS: &[&str] = &[
    ".github/workflows",
    ".gitlab-ci.yml",
    "Jenkinsfile",
    ".circleci",
    ".travis.yml",
    "azure-pipelines.yml",
    "bitbucket-pipelines.yml",
];

/// Test directory/config markers
pub const TEST_MARKERS: &[&str] = &[
    "__tests__",
    "tests",
    "test",
    "spec",
    "jest.config.js",
    "jest.config.ts",
    "vitest.config.ts",
    "vitest.config.js",
    ".mocharc.yml",
    "pytest.ini",
    "conftest.py",
    "phpunit.xml",
];

// ---------------------------------------------------------------------------
// Core Detection
// ---------------------------------------------------------------------------

/// Detect the full project state for a given directory.
/// This is the main entry point — calls all sub-detectors.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// Complete project detection result
///
/// # Example
/// ```
/// use rustycode_orchestra::detection::*;
///
/// let detection = detect_project_state("/project");
/// match detection.state {
///     OrchestraProjectState::V2Orchestra => println!("V2 project with {} milestones", detection.v2.unwrap().milestone_count),
///     OrchestraProjectState::None => println!("New project - needs init"),
///     _ => {},
/// }
/// ```
pub fn detect_project_state(base_path: &Path) -> ProjectDetection {
    let v1 = detect_v1_planning(base_path);
    let v2 = detect_v2_orchestra(base_path);
    let project_signals = detect_project_signals(base_path);
    let global_setup = has_global_setup();
    let first_ever = is_first_ever_launch();

    let state = if let Some(ref v2) = v2 {
        if v2.milestone_count > 0 {
            OrchestraProjectState::V2Orchestra
        } else {
            OrchestraProjectState::V2OrchestraEmpty
        }
    } else if v1.is_some() {
        OrchestraProjectState::V1Planning
    } else {
        OrchestraProjectState::None
    };

    ProjectDetection {
        state,
        is_first_ever_launch: first_ever,
        has_global_setup: global_setup,
        v1,
        v2,
        project_signals,
    }
}

/// Detect a v1 .planning/ directory with Orchestra v1 markers.
/// Returns None if no .planning/ directory found.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// V1 detection result if .planning/ exists, None otherwise
pub fn detect_v1_planning(base_path: &Path) -> Option<V1Detection> {
    let planning_path = base_path.join(".planning");

    if !planning_path.exists() {
        return None;
    }

    // Verify it's a directory
    if !planning_path.is_dir() {
        return None;
    }

    let has_roadmap = planning_path.join("ROADMAP.md").exists();
    let phases_path = planning_path.join("phases");
    let has_phases_dir = phases_path.exists();

    let mut phase_count = 0;
    if has_phases_dir {
        if let Ok(entries) = fs::read_dir(&phases_path) {
            phase_count = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count();
        }
    }

    Some(V1Detection {
        path: planning_path.to_string_lossy().to_string(),
        has_phases_dir,
        has_roadmap,
        phase_count,
    })
}

/// Detect V2 Orchestra state (.orchestra/ directory)
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// V2 detection result if .orchestra/ exists, None otherwise
fn detect_v2_orchestra(base_path: &Path) -> Option<V2Detection> {
    let orchestra_path = base_path.join(".orchestra");

    if !orchestra_path.exists() {
        return None;
    }

    let has_preferences = orchestra_path.join("preferences.md").exists()
        || orchestra_path.join("PREFERENCES.md").exists();

    let has_context = orchestra_path.join("CONTEXT.md").exists();

    let mut milestone_count = 0;
    let milestones_path = orchestra_path.join("milestones");
    if milestones_path.exists() {
        if let Ok(entries) = fs::read_dir(&milestones_path) {
            milestone_count = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count();
        }
    }

    Some(V2Detection {
        milestone_count,
        has_preferences,
        has_context,
    })
}

/// Quick filesystem scan for project ecosystem markers.
/// Reads only file existence + minimal content (package.json for monorepo/scripts).
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// Project signals detection result
pub fn detect_project_signals(base_path: &Path) -> ProjectSignals {
    let mut detected_files: Vec<String> = Vec::new();
    let mut primary_language: Option<String> = None;

    // Detect project files
    for file in PROJECT_FILES {
        let file_path = base_path.join(file);
        if file_path.exists() {
            detected_files.push(file.to_string());
            if primary_language.is_none() {
                primary_language = language_map().get(file as &str).map(|s| s.to_string());
            }
        }
    }

    // Git repo detection
    let is_git_repo = base_path.join(".git").exists();

    // Monorepo detection
    let mut is_monorepo = false;
    for marker in MONOREPO_MARKERS {
        if base_path.join(marker).exists() {
            is_monorepo = true;
            break;
        }
    }

    // Also check package.json workspaces
    if !is_monorepo && detected_files.iter().any(|f| f.contains("package.json")) {
        is_monorepo = package_json_has_workspaces(base_path);
    }

    // CI detection
    let mut has_ci = false;
    for marker in CI_MARKERS {
        if base_path.join(marker).exists() {
            has_ci = true;
            break;
        }
    }

    // Test detection
    let mut has_tests = false;
    for marker in TEST_MARKERS {
        if base_path.join(marker).exists() {
            has_tests = true;
            break;
        }
    }

    // Package manager detection
    let package_manager = detect_package_manager(base_path);

    // Verification commands
    let verification_commands =
        detect_verification_commands(base_path, &detected_files, package_manager.as_deref());

    ProjectSignals {
        detected_files,
        is_git_repo,
        is_monorepo,
        primary_language,
        has_ci,
        has_tests,
        package_manager,
        verification_commands,
    }
}

/// Detect the package manager for a project
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// Package manager name if detected, None otherwise
pub fn detect_package_manager(base_path: &Path) -> Option<String> {
    // Check lock files first (most specific)
    if base_path.join("pnpm-lock.yaml").exists() {
        return Some("pnpm".to_string());
    }
    if base_path.join("yarn.lock").exists() {
        return Some("yarn".to_string());
    }
    if base_path.join("bun.lockb").exists() || base_path.join("bun.lock").exists() {
        return Some("bun".to_string());
    }
    if base_path.join("package-lock.json").exists() {
        return Some("npm".to_string());
    }
    // Fallback to package.json
    if base_path.join("package.json").exists() {
        return Some("npm".to_string());
    }

    None
}

/// Auto-detect verification commands from project files.
/// Returns commands in priority order (test first, then build, then lint).
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `detected_files` - Files that were detected
/// * `package_manager` - Optional package manager name
///
/// # Returns
/// List of verification commands in priority order
fn detect_verification_commands(
    base_path: &Path,
    detected_files: &[String],
    package_manager: Option<&str>,
) -> Vec<String> {
    let mut commands: Vec<String> = Vec::new();

    let pm = package_manager.unwrap_or("npm");
    let run = if pm == "npm" {
        "npm run".to_string()
    } else if pm == "yarn" {
        "yarn".to_string()
    } else if pm == "bun" {
        "bun run".to_string()
    } else {
        format!("{} run", pm)
    };

    // Node.js/TypeScript projects
    if detected_files.iter().any(|f| f.contains("package.json")) {
        if let Some(scripts) = read_package_json_scripts(base_path) {
            // Test commands (highest priority)
            if let Some(test_cmd) = scripts.get("test") {
                if !test_cmd.contains("Error: no test specified") {
                    let cmd = if pm == "npm" {
                        "npm test".to_string()
                    } else {
                        format!("{} test", pm)
                    };
                    commands.push(cmd);
                }
            }

            // Build commands
            if scripts.contains_key("build") {
                commands.push(format!("{} build", run));
            }

            // Lint commands
            if scripts.contains_key("lint") {
                commands.push(format!("{} lint", run));
            }

            // Typecheck commands
            if scripts.contains_key("typecheck") {
                commands.push(format!("{} typecheck", run));
            } else if scripts.contains_key("tsc") {
                commands.push(format!("{} tsc", run));
            }
        }
    }

    // Rust projects
    if detected_files.iter().any(|f| f.contains("Cargo.toml")) {
        commands.push("cargo test".to_string());
        commands.push("cargo clippy".to_string());
    }

    // Go projects
    if detected_files.iter().any(|f| f.contains("go.mod")) {
        commands.push("go test ./...".to_string());
        commands.push("go vet ./...".to_string());
    }

    // Python projects
    if detected_files
        .iter()
        .any(|f| f.contains("pyproject.toml") || f.contains("setup.py"))
    {
        commands.push("pytest".to_string());
    }

    // Ruby projects
    if detected_files.iter().any(|f| f.contains("Gemfile")) {
        // Check for rspec vs minitest
        if base_path.join("spec").exists() {
            commands.push("bundle exec rspec".to_string());
        } else {
            commands.push("bundle exec rake test".to_string());
        }
    }

    // Makefile projects
    if detected_files.iter().any(|f| f.contains("Makefile")) {
        let make_targets = read_makefile_targets(base_path);
        if make_targets.contains(&"test".to_string()) {
            commands.push("make test".to_string());
        }
    }

    commands
}

/// Check if global Orchestra setup exists (has ~/.orchestra/ with preferences)
///
/// # Returns
/// true if ~/.orchestra/preferences.md or ~/.orchestra/PREFERENCES.md exists
pub fn has_global_setup() -> bool {
    let home = dirs::home_dir();

    if let Some(home_path) = home {
        let orchestra_home = home_path.join(".orchestra");
        return orchestra_home.join("preferences.md").exists()
            || orchestra_home.join("PREFERENCES.md").exists();
    }

    false
}

/// Check if this is the very first time Orchestra has been used on this machine.
/// Returns true if ~/.orchestra/ doesn't exist or has no preferences or auth.
///
/// # Returns
/// true if first launch, false otherwise
pub fn is_first_ever_launch() -> bool {
    let home = dirs::home_dir();

    if let Some(home_path) = home {
        let orchestra_home = home_path.join(".orchestra");

        // If ~/.orchestra/ doesn't exist, it's first launch
        if !orchestra_home.exists() {
            return true;
        }

        // If we have preferences, not first launch
        if orchestra_home.join("preferences.md").exists()
            || orchestra_home.join("PREFERENCES.md").exists()
        {
            return false;
        }

        // If we have auth.json, not first launch
        if orchestra_home.join("agent").join("auth.json").exists() {
            return false;
        }

        // Check legacy path too
        let legacy_path = home_path
            .join(".pi")
            .join("agent")
            .join("orchestra-preferences.md");

        if legacy_path.exists() {
            return false;
        }

        return true;
    }

    // Can't determine home dir - assume not first launch
    false
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Check if package.json has workspaces configured
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// true if package.json has workspaces
fn package_json_has_workspaces(base_path: &Path) -> bool {
    let package_json_path = base_path.join("package.json");

    if let Ok(raw) = fs::read_to_string(&package_json_path) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(workspaces) = pkg.get("workspaces") {
                // workspaces can be an array or an object
                return workspaces.is_array() || workspaces.is_object();
            }
        }
    }

    false
}

/// Read scripts from package.json
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// HashMap of script names to commands, or None if invalid
fn read_package_json_scripts(base_path: &Path) -> Option<HashMap<String, String>> {
    let package_json_path = base_path.join("package.json");

    if let Ok(raw) = fs::read_to_string(&package_json_path) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(scripts) = pkg.get("scripts") {
                if scripts.is_object() {
                    let mut map = HashMap::new();
                    if let Some(obj) = scripts.as_object() {
                        for (key, value) in obj {
                            if let Some(cmd) = value.as_str() {
                                map.insert(key.clone(), cmd.to_string());
                            }
                        }
                    }
                    return Some(map);
                }
            }
        }
    }

    None
}

/// Read target names from Makefile
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// List of make targets
fn read_makefile_targets(base_path: &Path) -> Vec<String> {
    let makefile_path = base_path.join("Makefile");

    if let Ok(raw) = fs::read_to_string(&makefile_path) {
        let mut targets: Vec<String> = Vec::new();
        let re = regex_lite::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_-]*):").unwrap();

        for line in raw.lines() {
            // Match targets like "test:", "build:", etc.
            if let Some(caps) = re.captures(line) {
                if let Some(target) = caps.get(1) {
                    targets.push(target.as_str().to_string());
                }
            }
        }

        targets
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_v1_planning_none() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = detect_v1_planning(base_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_v1_planning_with_roadmap() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let planning_path = base_path.join(".planning");

        fs::create_dir(&planning_path).unwrap();
        fs::write(planning_path.join("ROADMAP.md"), "# Roadmap").unwrap();

        let result = detect_v1_planning(base_path);
        assert!(result.is_some());
        let v1 = result.unwrap();
        assert!(v1.has_roadmap);
        assert!(!v1.has_phases_dir);
        assert_eq!(v1.phase_count, 0);
    }

    #[test]
    fn test_detect_v1_planning_with_phases() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let planning_path = base_path.join(".planning");
        let phases_path = planning_path.join("phases");

        fs::create_dir_all(&phases_path).unwrap();
        fs::create_dir(phases_path.join("phase1")).unwrap();
        fs::create_dir(phases_path.join("phase2")).unwrap();
        fs::create_dir(phases_path.join("phase3")).unwrap();

        let result = detect_v1_planning(base_path);
        assert!(result.is_some());
        let v1 = result.unwrap();
        assert!(v1.has_phases_dir);
        assert_eq!(v1.phase_count, 3);
    }

    #[test]
    fn test_detect_v2_orchestra_none() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = detect_v2_orchestra(base_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_v2_orchestra_with_milestones() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        fs::create_dir_all(&milestones_path).unwrap();
        fs::create_dir(milestones_path.join("m1")).unwrap();
        fs::create_dir(milestones_path.join("m2")).unwrap();
        fs::write(orchestra_path.join("preferences.md"), "# Preferences").unwrap();

        let result = detect_v2_orchestra(base_path);
        assert!(result.is_some());
        let v2 = result.unwrap();
        assert_eq!(v2.milestone_count, 2);
        assert!(v2.has_preferences);
        assert!(!v2.has_context);
    }

    #[test]
    fn test_detect_project_signals_node() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create package.json
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "scripts": {
                "test": "jest",
                "build": "webpack",
                "lint": "eslint"
            }
        }"#;
        fs::write(base_path.join("package.json"), pkg_json).unwrap();

        // Create .git
        fs::create_dir(base_path.join(".git")).unwrap();

        let signals = detect_project_signals(base_path);
        assert!(signals
            .detected_files
            .iter()
            .any(|f| f.contains("package.json")));
        assert_eq!(
            signals.primary_language,
            Some("javascript/typescript".to_string())
        );
        assert!(signals.is_git_repo);
        assert!(!signals.is_monorepo);
        assert_eq!(signals.package_manager, Some("npm".to_string()));

        // Check verification commands
        assert!(signals
            .verification_commands
            .iter()
            .any(|c| c.contains("npm test")));
        assert!(signals
            .verification_commands
            .iter()
            .any(|c| c.contains("build")));
        assert!(signals
            .verification_commands
            .iter()
            .any(|c| c.contains("lint")));
    }

    #[test]
    fn test_detect_project_signals_rust() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create Cargo.toml
        let cargo_toml = r#"[package]
name = "test"
version = "0.1.0"
"#;
        fs::write(base_path.join("Cargo.toml"), cargo_toml).unwrap();

        let signals = detect_project_signals(base_path);
        assert!(signals
            .detected_files
            .iter()
            .any(|f| f.contains("Cargo.toml")));
        assert_eq!(signals.primary_language, Some("rust".to_string()));

        // Check verification commands
        assert!(signals
            .verification_commands
            .iter()
            .any(|c| c.contains("cargo test")));
        assert!(signals
            .verification_commands
            .iter()
            .any(|c| c.contains("cargo clippy")));
    }

    #[test]
    fn test_detect_package_manager_pnpm() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        fs::write(base_path.join("pnpm-lock.yaml"), "").unwrap();

        let pm = detect_package_manager(base_path);
        assert_eq!(pm, Some("pnpm".to_string()));
    }

    #[test]
    fn test_detect_package_manager_yarn() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        fs::write(base_path.join("yarn.lock"), "").unwrap();

        let pm = detect_package_manager(base_path);
        assert_eq!(pm, Some("yarn".to_string()));
    }

    #[test]
    fn test_detect_package_manager_bun() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        fs::write(base_path.join("bun.lockb"), "").unwrap();

        let pm = detect_package_manager(base_path);
        assert_eq!(pm, Some("bun".to_string()));
    }

    #[test]
    fn test_package_json_has_workspaces_array() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let pkg_json = r#"{
            "name": "test",
            "workspaces": ["packages/*"]
        }"#;
        fs::write(base_path.join("package.json"), pkg_json).unwrap();

        assert!(package_json_has_workspaces(base_path));
    }

    #[test]
    fn test_package_json_has_workspaces_object() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let pkg_json = r#"{
            "name": "test",
            "workspaces": {
                "packages": ["packages/*"]
            }
        }"#;
        fs::write(base_path.join("package.json"), pkg_json).unwrap();

        assert!(package_json_has_workspaces(base_path));
    }

    #[test]
    fn test_read_package_json_scripts() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let pkg_json = r#"{
            "name": "test",
            "scripts": {
                "test": "jest",
                "build": "webpack"
            }
        }"#;
        fs::write(base_path.join("package.json"), pkg_json).unwrap();

        let scripts = read_package_json_scripts(base_path);
        assert!(scripts.is_some());
        let map = scripts.unwrap();
        assert_eq!(map.get("test"), Some(&"jest".to_string()));
        assert_eq!(map.get("build"), Some(&"webpack".to_string()));
    }

    #[test]
    fn test_read_makefile_targets() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let makefile = r#"
.PHONY: test build

test:
    pytest

build:
    webpack

lint:
    eslint
"#;
        fs::write(base_path.join("Makefile"), makefile).unwrap();

        let targets = read_makefile_targets(base_path);
        assert!(targets.contains(&"test".to_string()));
        assert!(targets.contains(&"build".to_string()));
        assert!(targets.contains(&"lint".to_string()));
    }

    #[test]
    fn test_detect_project_state_none() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let detection = detect_project_state(base_path);
        assert_eq!(detection.state, OrchestraProjectState::None);
    }

    #[test]
    fn test_detect_project_state_v2_orchestra() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        fs::create_dir_all(&milestones_path).unwrap();
        fs::create_dir(milestones_path.join("m1")).unwrap();
        fs::write(orchestra_path.join("preferences.md"), "# Preferences").unwrap();

        let detection = detect_project_state(base_path);
        assert_eq!(detection.state, OrchestraProjectState::V2Orchestra);
        assert!(detection.v2.is_some());
        assert_eq!(detection.v2.as_ref().unwrap().milestone_count, 1);
    }

    #[test]
    fn test_language_map() {
        let map = language_map();
        assert_eq!(map.get("Cargo.toml"), Some(&"rust"));
        assert_eq!(map.get("package.json"), Some(&"javascript/typescript"));
        assert_eq!(map.get("go.mod"), Some(&"go"));
    }

    #[test]
    fn test_project_files_const() {
        assert!(PROJECT_FILES.contains(&"package.json"));
        assert!(PROJECT_FILES.contains(&"Cargo.toml"));
        assert!(PROJECT_FILES.contains(&"go.mod"));
    }

    #[test]
    fn test_monorepo_markers_const() {
        assert!(MONOREPO_MARKERS.contains(&"lerna.json"));
        assert!(MONOREPO_MARKERS.contains(&"nx.json"));
        assert!(MONOREPO_MARKERS.contains(&"pnpm-workspace.yaml"));
    }

    #[test]
    fn test_ci_markers_const() {
        assert!(CI_MARKERS.contains(&".github/workflows"));
        assert!(CI_MARKERS.contains(&".gitlab-ci.yml"));
        assert!(CI_MARKERS.contains(&"Jenkinsfile"));
    }

    #[test]
    fn test_test_markers_const() {
        assert!(TEST_MARKERS.contains(&"test"));
        assert!(TEST_MARKERS.contains(&"spec"));
        assert!(TEST_MARKERS.contains(&"pytest.ini"));
    }

    #[test]
    fn test_detect_project_signals_with_ci() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create .github/workflows
        fs::create_dir_all(base_path.join(".github/workflows")).unwrap();

        let signals = detect_project_signals(base_path);
        assert!(signals.has_ci);
    }

    #[test]
    fn test_detect_project_signals_with_tests() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create test directory
        fs::create_dir(base_path.join("test")).unwrap();

        let signals = detect_project_signals(base_path);
        assert!(signals.has_tests);
    }
}
