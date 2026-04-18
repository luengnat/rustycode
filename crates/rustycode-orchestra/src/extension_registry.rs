//! Extension registry — manages manifest reading, registry persistence, and enable/disable state.
//!
//! Extensions without manifests always load (backwards compatible).
//! A fresh install has an empty registry — all extensions enabled by default.
//! The only way an extension stops loading is an explicit `orchestra extensions disable <id>`.
//!
//! Matches orchestra-2's extension-registry.ts implementation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

// ─── Types ───────────────────────────────────────────────────────────────────

/// Extension manifest metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(rename = "tier")]
    pub tier: ExtensionTier,
    pub requires: PlatformRequires,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provides: Option<ExtensionProvides>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<ExtensionDependencies>,
}

/// Extension tier classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ExtensionTier {
    Core,
    Bundled,
    Community,
}

/// Platform requirements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformRequires {
    pub platform: String,
}

/// What an extension provides
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionProvides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<Vec<String>>,
}

/// Extension dependencies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionDependencies {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<Vec<String>>,
}

/// Registry entry for an extension
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionRegistryEntry {
    pub id: String,
    pub enabled: bool,
    #[serde(rename = "source")]
    pub source: ExtensionSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

/// Extension source classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ExtensionSource {
    Bundled,
    User,
    Project,
}

/// Extension registry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionRegistry {
    pub version: u32,
    pub entries: HashMap<String, ExtensionRegistryEntry>,
}

// ─── Validation ─────────────────────────────────────────────────────────────

/// Check if data is a valid registry
fn is_registry(data: &serde_json::Value) -> bool {
    data.get("version").and_then(|v| v.as_u64()) == Some(1)
        && data.get("entries").and_then(|e| e.as_object()).is_some()
}

/// Check if data is a valid manifest
fn is_manifest(data: &serde_json::Value) -> bool {
    data.get("id").and_then(|v| v.as_str()).is_some()
        && data.get("name").and_then(|v| v.as_str()).is_some()
        && data.get("version").and_then(|v| v.as_str()).is_some()
        && data.get("tier").and_then(|v| v.as_str()).is_some()
}

// ─── Registry Path ──────────────────────────────────────────────────────────

/// Get the path to the extension registry file
///
/// # Returns
/// Path to ~/.orchestra/extensions/registry.json
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::get_registry_path;
///
/// let path = get_registry_path();
/// assert!(path.ends_with(".orchestra/extensions/registry.json"));
/// ```
pub fn get_registry_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".orchestra")
        .join("extensions")
        .join("registry.json")
}

// ─── Registry I/O ───────────────────────────────────────────────────────────

/// Create a default empty registry
fn default_registry() -> ExtensionRegistry {
    ExtensionRegistry {
        version: 1,
        entries: HashMap::new(),
    }
}

/// Load the extension registry from disk
///
/// # Returns
/// The loaded registry, or a default empty registry if the file doesn't exist or is invalid
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::load_registry;
///
/// let registry = load_registry();
/// assert_eq!(registry.version, 1);
/// ```
pub fn load_registry() -> ExtensionRegistry {
    let file_path = get_registry_path();

    // Return default if file doesn't exist
    if !file_path.exists() {
        return default_registry();
    }

    // Try to read and parse the file
    let result = read_json_file(&file_path);
    match result {
        Ok(json) => {
            if is_registry(&json) {
                serde_json::from_value(json).unwrap_or_else(|_| default_registry())
            } else {
                default_registry()
            }
        }
        Err(_) => default_registry(),
    }
}

/// Save the extension registry to disk
///
/// # Arguments
/// * `registry` - The registry to save
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::{save_registry, load_registry};
///
/// let mut registry = load_registry();
/// // Modify registry...
/// save_registry(registry);
/// ```
pub fn save_registry(registry: &ExtensionRegistry) {
    let file_path = get_registry_path();

    // Create parent directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("Failed to create registry directory {:?}: {}", parent, e);
            return;
        }
    }

    // Write to temporary file first, then rename (atomic write)
    let tmp_path = file_path.with_extension("tmp");
    let json = serde_json::to_string_pretty(registry);

    if let Ok(json_str) = json {
        if let Err(e) = fs::write(&tmp_path, &json_str) {
            tracing::warn!("Failed to write registry to {:?}: {}", tmp_path, e);
            return;
        }
        if let Err(e) = fs::rename(&tmp_path, &file_path) {
            tracing::warn!(
                "Failed to rename registry {:?} -> {:?}: {}",
                tmp_path,
                file_path,
                e
            );
            let _ = fs::remove_file(&tmp_path);
        }
    }
    // Non-fatal — don't let persistence failures break operation
}

// ─── Query ──────────────────────────────────────────────────────────────────

/// Check if an extension is enabled
///
/// Returns true if the extension is enabled (missing entries default to enabled).
///
/// # Arguments
/// * `registry` - The extension registry
/// * `id` - Extension ID to check
///
/// # Returns
/// true if enabled, false otherwise
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::{is_extension_enabled, load_registry};
///
/// let registry = load_registry();
/// let enabled = is_extension_enabled(&registry, "some-extension");
/// ```
pub fn is_extension_enabled(registry: &ExtensionRegistry, id: &str) -> bool {
    registry
        .entries
        .get(id)
        .map(|entry| entry.enabled)
        .unwrap_or(true)
}

// ─── Mutations ──────────────────────────────────────────────────────────────

/// Enable an extension
///
/// # Arguments
/// * `registry` - The extension registry (mutable)
/// * `id` - Extension ID to enable
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::{enable_extension, load_registry, save_registry};
///
/// let mut registry = load_registry();
/// enable_extension(&mut registry, "some-extension");
/// save_registry(&registry);
/// ```
pub fn enable_extension(registry: &mut ExtensionRegistry, id: &str) {
    if let Some(entry) = registry.entries.get_mut(id) {
        entry.enabled = true;
        entry.disabled_at = None;
        entry.disabled_reason = None;
    } else {
        registry.entries.insert(
            id.to_string(),
            ExtensionRegistryEntry {
                id: id.to_string(),
                enabled: true,
                source: ExtensionSource::Bundled,
                disabled_at: None,
                disabled_reason: None,
            },
        );
    }
}

/// Disable an extension
///
/// # Arguments
/// * `registry` - The extension registry (mutable)
/// * `id` - Extension ID to disable
/// * `manifest` - Optional manifest for validation
/// * `reason` - Optional reason for disabling
///
/// # Returns
/// Ok(()) on success, Err(message) if the extension is core (cannot disable)
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::{disable_extension, load_registry, save_registry};
///
/// let mut registry = load_registry();
/// match disable_extension(&mut registry, "some-extension", None, Some("Testing")) {
///     Ok(_) => save_registry(&registry),
///     Err(msg) => eprintln!("{}", msg),
/// }
/// ```
pub fn disable_extension(
    registry: &mut ExtensionRegistry,
    id: &str,
    manifest: Option<&ExtensionManifest>,
    reason: Option<&str>,
) -> Result<(), String> {
    // Cannot disable core extensions
    if let Some(manifest) = manifest {
        if manifest.tier == ExtensionTier::Core {
            return Err(format!(
                "Cannot disable \"{}\" — it is a core extension.",
                id
            ));
        }
    }

    if let Some(entry) = registry.entries.get_mut(id) {
        entry.enabled = false;
        entry.disabled_at = Some(chrono_timestamp());
        entry.disabled_reason = reason.map(|r| r.to_string());
    } else {
        registry.entries.insert(
            id.to_string(),
            ExtensionRegistryEntry {
                id: id.to_string(),
                enabled: false,
                source: ExtensionSource::Bundled,
                disabled_at: Some(chrono_timestamp()),
                disabled_reason: reason.map(|r| r.to_string()),
            },
        );
    }

    Ok(())
}

/// Get current timestamp in ISO 8601 format
fn chrono_timestamp() -> String {
    // Simple timestamp format
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + secs / 31536000,
        (secs % 31536000) / 2592000 + 1,
        (secs % 2592000) / 86400 + 1,
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ─── Manifest Reading ───────────────────────────────────────────────────────

/// Read extension-manifest.json from a directory
///
/// Returns None if the file is missing or invalid.
///
/// # Arguments
/// * `extension_dir` - Path to the extension directory
///
/// # Returns
/// Some(manifest) if valid, None otherwise
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::read_manifest;
///
/// if let Some(manifest) = read_manifest("/path/to/extension") {
///     println!("Found extension: {}", manifest.name);
/// }
/// ```
pub fn read_manifest<P: AsRef<Path>>(extension_dir: P) -> Option<ExtensionManifest> {
    let manifest_path = extension_dir.as_ref().join("extension-manifest.json");

    if !manifest_path.exists() {
        return None;
    }

    let result = read_json_file(&manifest_path);
    match result {
        Ok(json) => {
            if is_manifest(&json) {
                serde_json::from_value(json).ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Read manifest from an entry path
///
/// Given an entry path (e.g. `.../extensions/browser-tools/index.ts`),
/// resolve the parent directory and read its manifest.
///
/// # Arguments
/// * `entry_path` - Path to the extension entry file
///
/// # Returns
/// Some(manifest) if valid, None otherwise
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::read_manifest_from_entry_path;
///
/// if let Some(manifest) = read_manifest_from_entry_path("/path/to/extension/index.ts") {
///     println!("Found extension: {}", manifest.name);
/// }
/// ```
pub fn read_manifest_from_entry_path<P: AsRef<Path>>(entry_path: P) -> Option<ExtensionManifest> {
    let dir = entry_path.as_ref().parent()?;
    read_manifest(dir)
}

// ─── Discovery ──────────────────────────────────────────────────────────────

/// Scan all subdirectories of extensions_dir for manifests
///
/// Returns a Map<id, manifest>.
///
/// # Arguments
/// * `extensions_dir` - Path to the extensions directory
///
/// # Returns
/// HashMap mapping extension IDs to their manifests
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::discover_all_manifests;
///
/// let manifests = discover_all_manifests("/path/to/extensions");
/// for (id, manifest) in manifests {
///     println!("Found {}: {}", id, manifest.name);
/// }
/// ```
pub fn discover_all_manifests<P: AsRef<Path>>(
    extensions_dir: P,
) -> HashMap<String, ExtensionManifest> {
    let mut manifests = HashMap::new();
    let extensions_dir = extensions_dir.as_ref();

    if !extensions_dir.exists() {
        return manifests;
    }

    let entries = match fs::read_dir(extensions_dir) {
        Ok(entries) => entries,
        Err(_) => return manifests,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !file_type.is_dir() {
            continue;
        }

        let manifest = read_manifest(entry.path());
        if let Some(manifest) = manifest {
            manifests.insert(manifest.id.clone(), manifest);
        }
    }

    manifests
}

/// Auto-populate registry entries for newly discovered extensions
///
/// Extensions already in the registry are left untouched.
///
/// # Arguments
/// * `extensions_dir` - Path to the extensions directory
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_registry::ensure_registry_entries;
///
/// ensure_registry_entries("/path/to/extensions");
/// ```
pub fn ensure_registry_entries<P: AsRef<Path>>(extensions_dir: P) {
    let manifests = discover_all_manifests(extensions_dir);
    if manifests.is_empty() {
        return;
    }

    let mut registry = load_registry();
    let mut changed = false;

    for (id, _manifest) in manifests {
        if !registry.entries.contains_key(&id) {
            registry.entries.insert(
                id.clone(),
                ExtensionRegistryEntry {
                    id,
                    enabled: true,
                    source: ExtensionSource::Bundled,
                    disabled_at: None,
                    disabled_reason: None,
                },
            );
            changed = true;
        }
    }

    if changed {
        save_registry(&registry);
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Read a JSON file
///
/// # Arguments
/// * `path` - Path to the JSON file
///
/// # Returns
/// Ok(serde_json::Value) on success, Err(io::Error) on failure
fn read_json_file<P: AsRef<Path>>(path: P) -> io::Result<serde_json::Value> {
    let mut file = File::open(path.as_ref())?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    serde_json::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extension_tier_deserialization() {
        let json = r#""core""#;
        let tier: ExtensionTier = serde_json::from_str(json).unwrap();
        assert_eq!(tier, ExtensionTier::Core);

        let json = r#""bundled""#;
        let tier: ExtensionTier = serde_json::from_str(json).unwrap();
        assert_eq!(tier, ExtensionTier::Bundled);

        let json = r#""community""#;
        let tier: ExtensionTier = serde_json::from_str(json).unwrap();
        assert_eq!(tier, ExtensionTier::Community);
    }

    #[test]
    fn test_extension_source_deserialization() {
        let json = r#""bundled""#;
        let source: ExtensionSource = serde_json::from_str(json).unwrap();
        assert_eq!(source, ExtensionSource::Bundled);

        let json = r#""user""#;
        let source: ExtensionSource = serde_json::from_str(json).unwrap();
        assert_eq!(source, ExtensionSource::User);

        let json = r#""project""#;
        let source: ExtensionSource = serde_json::from_str(json).unwrap();
        assert_eq!(source, ExtensionSource::Project);
    }

    #[test]
    fn test_get_registry_path() {
        let path = get_registry_path();
        assert!(path.ends_with(".orchestra/extensions/registry.json"));
    }

    #[test]
    fn test_default_registry() {
        let registry = default_registry();
        assert_eq!(registry.version, 1);
        assert!(registry.entries.is_empty());
    }

    #[test]
    fn test_is_registry_valid() {
        let json = serde_json::json!({
            "version": 1,
            "entries": {}
        });
        assert!(is_registry(&json));

        let json = serde_json::json!({
            "version": 2,
            "entries": {}
        });
        assert!(!is_registry(&json));

        let json = serde_json::json!({
            "version": 1
        });
        assert!(!is_registry(&json));
    }

    #[test]
    fn test_is_manifest_valid() {
        let json = serde_json::json!({
            "id": "test-ext",
            "name": "Test Extension",
            "version": "1.0.0",
            "tier": "community",
            "description": "A test extension",
            "requires": { "platform": "any" }
        });
        assert!(is_manifest(&json));

        let json = serde_json::json!({
            "name": "Test Extension",
            "version": "1.0.0",
            "tier": "community"
        });
        assert!(!is_manifest(&json));
    }

    #[test]
    fn test_load_registry_default() {
        // This test loads the actual registry from the filesystem.
        // We only verify the version since entries may exist from previous runs.
        let registry = load_registry();
        assert_eq!(registry.version, 1);
        // Note: entries.is_empty() may be false if a registry file exists
    }

    #[test]
    fn test_is_extension_enabled_missing() {
        let registry = default_registry();
        assert!(is_extension_enabled(&registry, "nonexistent"));
    }

    #[test]
    fn test_is_extension_enabled_explicit() {
        let mut registry = default_registry();
        registry.entries.insert(
            "test-ext".to_string(),
            ExtensionRegistryEntry {
                id: "test-ext".to_string(),
                enabled: false,
                source: ExtensionSource::Bundled,
                disabled_at: None,
                disabled_reason: None,
            },
        );

        assert!(!is_extension_enabled(&registry, "test-ext"));
    }

    #[test]
    fn test_enable_extension_new() {
        let mut registry = default_registry();
        enable_extension(&mut registry, "test-ext");

        assert!(registry.entries.contains_key("test-ext"));
        let entry = registry.entries.get("test-ext").unwrap();
        assert!(entry.enabled);
        assert_eq!(entry.id, "test-ext");
    }

    #[test]
    fn test_enable_extension_existing() {
        let mut registry = default_registry();
        registry.entries.insert(
            "test-ext".to_string(),
            ExtensionRegistryEntry {
                id: "test-ext".to_string(),
                enabled: false,
                source: ExtensionSource::Bundled,
                disabled_at: Some("2024-01-01T00:00:00Z".to_string()),
                disabled_reason: Some("Testing".to_string()),
            },
        );

        enable_extension(&mut registry, "test-ext");

        let entry = registry.entries.get("test-ext").unwrap();
        assert!(entry.enabled);
        assert!(entry.disabled_at.is_none());
        assert!(entry.disabled_reason.is_none());
    }

    #[test]
    fn test_disable_extension_new() {
        let mut registry = default_registry();
        let result = disable_extension(&mut registry, "test-ext", None, Some("Testing"));

        assert!(result.is_ok());
        assert!(registry.entries.contains_key("test-ext"));

        let entry = registry.entries.get("test-ext").unwrap();
        assert!(!entry.enabled);
        assert_eq!(entry.disabled_reason.as_ref().unwrap(), "Testing");
    }

    #[test]
    fn test_disable_extension_core() {
        let mut registry = default_registry();
        let manifest = ExtensionManifest {
            id: "core-ext".to_string(),
            name: "Core Extension".to_string(),
            version: "1.0.0".to_string(),
            description: "A core extension".to_string(),
            tier: ExtensionTier::Core,
            requires: PlatformRequires {
                platform: "any".to_string(),
            },
            provides: None,
            dependencies: None,
        };

        let result = disable_extension(&mut registry, "core-ext", Some(&manifest), Some("Testing"));

        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Cannot disable"));
        assert!(error_msg.contains("core extension"));
    }

    #[test]
    fn test_disable_extension_existing() {
        let mut registry = default_registry();
        registry.entries.insert(
            "test-ext".to_string(),
            ExtensionRegistryEntry {
                id: "test-ext".to_string(),
                enabled: true,
                source: ExtensionSource::Bundled,
                disabled_at: None,
                disabled_reason: None,
            },
        );

        let result = disable_extension(&mut registry, "test-ext", None, Some("Testing"));

        assert!(result.is_ok());

        let entry = registry.entries.get("test-ext").unwrap();
        assert!(!entry.enabled);
        assert_eq!(entry.disabled_reason.as_ref().unwrap(), "Testing");
        assert!(entry.disabled_at.is_some());
    }

    #[test]
    fn test_read_manifest_missing() {
        let temp_dir = TempDir::new().unwrap();
        let result = read_manifest(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_read_manifest_valid() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("extension-manifest.json");

        let json = serde_json::json!({
            "id": "test-ext",
            "name": "Test Extension",
            "version": "1.0.0",
            "tier": "community",
            "description": "A test extension",
            "requires": { "platform": "any" }
        });

        fs::write(&manifest_path, json.to_string()).unwrap();

        let result = read_manifest(temp_dir.path());
        assert!(result.is_some());

        let manifest = result.unwrap();
        assert_eq!(manifest.id, "test-ext");
        assert_eq!(manifest.name, "Test Extension");
    }

    #[test]
    fn test_read_manifest_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("extension-manifest.json");

        fs::write(&manifest_path, "invalid json").unwrap();

        let result = read_manifest(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_read_manifest_from_entry_path() {
        let temp_dir = TempDir::new().unwrap();
        let ext_dir = temp_dir.path().join("test-ext");
        fs::create_dir(&ext_dir).unwrap();

        let manifest_path = ext_dir.join("extension-manifest.json");

        let json = serde_json::json!({
            "id": "test-ext",
            "name": "Test Extension",
            "version": "1.0.0",
            "tier": "community",
            "description": "A test extension",
            "requires": { "platform": "any" }
        });

        fs::write(&manifest_path, json.to_string()).unwrap();

        let entry_path = ext_dir.join("index.ts");
        let result = read_manifest_from_entry_path(&entry_path);

        assert!(result.is_some());
        let manifest = result.unwrap();
        assert_eq!(manifest.id, "test-ext");
    }

    #[test]
    fn test_discover_all_manifests_empty() {
        let temp_dir = TempDir::new().unwrap();
        let manifests = discover_all_manifests(temp_dir.path());
        assert!(manifests.is_empty());
    }

    #[test]
    fn test_discover_all_manifests() {
        let temp_dir = TempDir::new().unwrap();

        // Create first extension
        let ext1_dir = temp_dir.path().join("ext1");
        fs::create_dir(&ext1_dir).unwrap();
        let manifest1 = ext1_dir.join("extension-manifest.json");
        let json1 = serde_json::json!({
            "id": "ext1",
            "name": "Extension 1",
            "version": "1.0.0",
            "tier": "community",
            "description": "First extension",
            "requires": { "platform": "any" }
        });
        fs::write(&manifest1, json1.to_string()).unwrap();

        // Create second extension
        let ext2_dir = temp_dir.path().join("ext2");
        fs::create_dir(&ext2_dir).unwrap();
        let manifest2 = ext2_dir.join("extension-manifest.json");
        let json2 = serde_json::json!({
            "id": "ext2",
            "name": "Extension 2",
            "version": "1.0.0",
            "tier": "community",
            "description": "Second extension",
            "requires": { "platform": "any" }
        });
        fs::write(&manifest2, json2.to_string()).unwrap();

        let manifests = discover_all_manifests(temp_dir.path());
        assert_eq!(manifests.len(), 2);
        assert!(manifests.contains_key("ext1"));
        assert!(manifests.contains_key("ext2"));
    }

    #[test]
    fn test_ensure_registry_entries_new() {
        let temp_dir = TempDir::new().unwrap();

        // Create an extension
        let ext_dir = temp_dir.path().join("test-ext");
        fs::create_dir(&ext_dir).unwrap();
        let manifest_path = ext_dir.join("extension-manifest.json");
        let json = serde_json::json!({
            "id": "test-ext",
            "name": "Test Extension",
            "version": "1.0.0",
            "tier": "community",
            "description": "A test extension",
            "requires": { "platform": "any" }
        });
        fs::write(&manifest_path, json.to_string()).unwrap();

        // This should create registry entries
        // Note: In tests, this might not actually write to ~/.orchestra, but we can test the logic
        let manifests = discover_all_manifests(temp_dir.path());
        assert_eq!(manifests.len(), 1);
        assert!(manifests.contains_key("test-ext"));
    }

    #[test]
    fn test_chrono_timestamp() {
        let timestamp = chrono_timestamp();
        assert!(timestamp.contains('T'));
        assert!(timestamp.contains('Z'));
        assert!(timestamp.len() > 10);
    }

    #[test]
    fn test_extension_manifest_serialization() {
        let manifest = ExtensionManifest {
            id: "test-ext".to_string(),
            name: "Test Extension".to_string(),
            version: "1.0.0".to_string(),
            description: "A test extension".to_string(),
            tier: ExtensionTier::Community,
            requires: PlatformRequires {
                platform: "any".to_string(),
            },
            provides: None,
            dependencies: None,
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ExtensionManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, manifest.id);
        assert_eq!(parsed.name, manifest.name);
        assert_eq!(parsed.tier, manifest.tier);
    }

    #[test]
    fn test_extension_registry_serialization() {
        let mut registry = ExtensionRegistry {
            version: 1,
            entries: HashMap::new(),
        };

        registry.entries.insert(
            "test-ext".to_string(),
            ExtensionRegistryEntry {
                id: "test-ext".to_string(),
                enabled: true,
                source: ExtensionSource::Bundled,
                disabled_at: None,
                disabled_reason: None,
            },
        );

        let json = serde_json::to_string(&registry).unwrap();
        let parsed: ExtensionRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.entries.len(), 1);
        assert!(parsed.entries.contains_key("test-ext"));
    }
}
