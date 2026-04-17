//! Plugin manager - discovers, loads, and manages plugins
//!
//! # Experimental Status
//!
//! The plugin system is currently **experimental**. The core architecture is in place,
//! but dynamic library loading is not yet implemented.
//!
//! ## What Works
//!
//! - Plugin discovery via `plugin.toml` manifests
//! - Plugin enable/disable functionality
//! - Plugin metadata management
//! - Command registration from manifests
//!
//! ## What's Not Yet Implemented
//!
//! - **Dynamic library loading**: Plugins cannot load compiled code (`.so`, `.dylib`, `.dll`)
//! - **Command execution**: Commands return placeholder messages instead of calling actual handlers
//! - **Permission enforcement**: Permissions are parsed but not enforced
//!
//! ## Future Work
//!
//! To complete the plugin system:
//! 1. Implement dynamic library loading using `libloading` crate
//! 2. Define a stable plugin ABI (e.g., using `#[no_mangle]` extern "C" functions)
//! 3. Add permission enforcement before plugin operations
//! 4. Implement sandboxing for untrusted plugins
//! 5. Add plugin lifecycle hooks (on_load, on_unload)

use super::api::{CommandHandler, CommandResult, PluginAPI};
use super::manifest::PluginManifest;
use super::permissions::PluginPermissions;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

/// Loaded plugin instance
pub struct Plugin {
    /// Plugin manifest
    pub manifest: PluginManifest,

    /// Whether the plugin is enabled
    pub enabled: bool,

    /// Plugin permissions
    pub permissions: PluginPermissions,

    /// Registered command handlers
    pub command_handlers: HashMap<String, CommandHandler>,

    /// Plugin-specific API instance
    pub api: PluginAPI,
}

/// Plugin manager
pub struct PluginManager {
    /// Loaded plugins
    plugins: HashMap<String, Plugin>,

    /// Plugin directory
    plugin_dir: PathBuf,
}

impl PluginManager {
    /// Create new plugin manager
    pub fn new() -> Result<Self> {
        let plugin_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
            .join(".rustycode/plugins");

        // Create plugin directory if it doesn't exist
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir).context("Failed to create plugin directory")?;
        }

        Ok(Self {
            plugins: HashMap::new(),
            plugin_dir,
        })
    }

    /// Discover and load all plugins
    pub fn discover_plugins(&mut self) -> Result<usize> {
        let mut loaded = 0;

        if !self.plugin_dir.exists() {
            tracing::info!("Plugin directory does not exist: {:?}", self.plugin_dir);
            return Ok(0);
        }

        // Iterate through subdirectories
        for entry in
            std::fs::read_dir(&self.plugin_dir).context("Failed to read plugin directory")?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            // Look for plugin.toml
            let manifest_path = path.join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }

            // Try to load the plugin
            if let Err(e) = self.load_plugin(&manifest_path) {
                tracing::warn!("Failed to load plugin at {:?}: {}", path, e);
                continue;
            }

            loaded += 1;
        }

        tracing::info!("Loaded {} plugins", loaded);
        Ok(loaded)
    }

    /// Load a plugin from its manifest path
    pub fn load_plugin(&mut self, manifest_path: &PathBuf) -> Result<()> {
        // Load and parse manifest
        let manifest =
            PluginManifest::from_path(manifest_path).context("Failed to load plugin manifest")?;

        // Check if plugin already loaded
        if self.plugins.contains_key(&manifest.name) {
            tracing::warn!("Plugin '{}' already loaded, skipping", manifest.name);
            return Ok(());
        }

        // Parse permissions
        let permissions = PluginPermissions::from_strings(manifest.permissions.clone());

        // EXPERIMENTAL: Permission checking is not yet enforced
        // Permissions are parsed and logged, but not checked against operations
        tracing::info!(
            "Plugin '{}' requires permissions: {:?}",
            manifest.name,
            permissions.permissions
        );

        // Create plugin API instance
        let api = PluginAPI::new(manifest.name.clone());

        // Load plugin library (if entry point exists)
        let plugin_dir = manifest_path.parent().unwrap();
        let lib_path = plugin_dir.join(&manifest.entry_point);

        let command_handlers = HashMap::new();

        if lib_path.exists() {
            // Load dynamic library
            tracing::info!("Loading plugin library: {:?}", lib_path);

            // EXPERIMENTAL: Dynamic library loading not yet implemented
            // Requires:
            // 1. Add `libloading` crate to Cargo.toml
            // 2. Define plugin ABI with extern "C" functions
            // 3. Implement safe wrapper for loaded symbols
            // 4. Handle platform-specific library extensions (.so, .dylib, .dll)
            tracing::warn!(
                "Dynamic library loading not yet implemented - using manifest commands only"
            );
        } else {
            tracing::info!("Plugin library not found, using manifest commands only");
        }

        // Register commands from manifest
        // NOTE: Currently commands are only tracked for discovery
        // Actual handler execution requires dynamic library loading
        for cmd in &manifest.slash_commands {
            tracing::info!(
                "Registered command: /{} (handler: {})",
                cmd.name,
                cmd.handler
            );
        }

        // Create plugin instance
        let plugin = Plugin {
            manifest: manifest.clone(),
            enabled: true,
            permissions,
            command_handlers,
            api,
        };

        // Add to plugins map
        self.plugins.insert(manifest.name.clone(), plugin);

        tracing::info!("Loaded plugin: {} v{}", manifest.name, manifest.version);

        Ok(())
    }

    /// Unload a plugin
    pub fn unload_plugin(&mut self, name: &str) -> Result<()> {
        if !self.plugins.contains_key(name) {
            return Err(anyhow::anyhow!("Plugin '{}' not found", name));
        }

        // Remove plugin
        self.plugins.remove(name);

        tracing::info!("Unloaded plugin: {}", name);

        Ok(())
    }

    /// Enable a plugin
    pub fn enable_plugin(&mut self, name: &str) -> Result<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", name))?;

        plugin.enabled = true;
        tracing::info!("Enabled plugin: {}", name);

        Ok(())
    }

    /// Disable a plugin
    pub fn disable_plugin(&mut self, name: &str) -> Result<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", name))?;

        plugin.enabled = false;
        tracing::info!("Disabled plugin: {}", name);

        Ok(())
    }

    /// Get all plugins
    pub fn get_plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().collect()
    }

    /// Get enabled plugins
    pub fn get_enabled_plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().filter(|p| p.enabled).collect()
    }

    /// Get a specific plugin
    pub fn get_plugin(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name)
    }

    /// Get plugin mutable
    pub fn get_plugin_mut(&mut self, name: &str) -> Option<&mut Plugin> {
        self.plugins.get_mut(name)
    }

    /// Execute a slash command
    pub fn execute_command(&mut self, command: &str, _args: Vec<String>) -> Result<CommandResult> {
        // Find which plugin handles this command
        let (plugin_name, _handler_name) = self.find_command_handler(command)?;

        let plugin = self
            .plugins
            .get_mut(&plugin_name)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_name))?;

        if !plugin.enabled {
            return Ok(CommandResult::Error(format!(
                "Plugin '{}' is disabled",
                plugin_name
            )));
        }

        // EXPERIMENTAL: Command execution returns a placeholder
        // Real execution requires:
        // 1. Load the plugin's dynamic library
        // 2. Look up the handler function by name
        // 3. Call the function with the provided arguments
        // 4. Handle panics/errors from plugin code safely
        Ok(CommandResult::Message(format!(
            "[EXPERIMENTAL] Command '{}' from plugin '{}' - dynamic loading not yet implemented",
            command, plugin_name
        )))
    }

    /// Find which plugin handles a command
    fn find_command_handler(&self, command: &str) -> Result<(String, String)> {
        for (name, plugin) in &self.plugins {
            for cmd in &plugin.manifest.slash_commands {
                if cmd.name == command {
                    return Ok((name.clone(), cmd.handler.clone()));
                }
            }
        }

        Err(anyhow::anyhow!(
            "Command '{}' not found in any plugin",
            command
        ))
    }

    /// Get all slash commands from enabled plugins
    pub fn get_all_commands(&self) -> Vec<(String, String, String)> {
        let mut commands = Vec::new();

        for plugin in self.get_enabled_plugins() {
            for cmd in &plugin.manifest.slash_commands {
                commands.push((
                    plugin.manifest.name.clone(),
                    cmd.name.clone(),
                    cmd.description.clone(),
                ));
            }
        }

        commands
    }

    /// Get plugin directory
    pub fn plugin_dir(&self) -> &PathBuf {
        &self.plugin_dir
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        match Self::new() {
            Ok(manager) => manager,
            Err(e) => {
                tracing::warn!("PluginManager init failed, using empty state: {e}");
                Self {
                    plugins: HashMap::new(),
                    plugin_dir: PathBuf::new(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_plugin(temp_dir: &TempDir, name: &str) -> PathBuf {
        let plugin_dir = temp_dir.path().join(name);
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_path = plugin_dir.join("plugin.toml");
        let manifest = format!(
            r#"
            name = "{}"
            version = "0.1.0"
            description = "Test plugin"
            permissions = []
            entry_point = "lib{}.so"
            "#,
            name, name
        );

        let mut file = std::fs::File::create(&manifest_path).unwrap();
        file.write_all(manifest.as_bytes()).unwrap();

        plugin_dir
    }

    #[test]
    fn test_plugin_manager_new() {
        let manager = PluginManager::new().unwrap();
        assert!(manager.plugin_dir().exists());
    }

    #[test]
    fn test_load_plugin() {
        let temp_dir = TempDir::new().unwrap();
        let plugin_dir = create_test_plugin(&temp_dir, "test-plugin");

        let mut manager = PluginManager::new().unwrap();
        manager.plugin_dir = temp_dir.path().to_path_buf();

        let manifest_path = plugin_dir.join("plugin.toml");
        assert!(manager.load_plugin(&manifest_path).is_ok());

        assert_eq!(manager.get_plugins().len(), 1);
        assert!(manager.get_plugin("test-plugin").is_some());
    }

    #[test]
    fn test_enable_disable_plugin() {
        let temp_dir = TempDir::new().unwrap();
        let plugin_dir = create_test_plugin(&temp_dir, "test-plugin");

        let mut manager = PluginManager::new().unwrap();
        manager.plugin_dir = temp_dir.path().to_path_buf();

        let manifest_path = plugin_dir.join("plugin.toml");
        manager.load_plugin(&manifest_path).unwrap();

        // Plugin is enabled by default
        assert!(manager.get_plugin("test-plugin").unwrap().enabled);

        // Disable
        manager.disable_plugin("test-plugin").unwrap();
        assert!(!manager.get_plugin("test-plugin").unwrap().enabled);

        // Enable
        manager.enable_plugin("test-plugin").unwrap();
        assert!(manager.get_plugin("test-plugin").unwrap().enabled);
    }

    #[test]
    fn test_unload_plugin() {
        let temp_dir = TempDir::new().unwrap();
        let plugin_dir = create_test_plugin(&temp_dir, "test-plugin");

        let mut manager = PluginManager::new().unwrap();
        manager.plugin_dir = temp_dir.path().to_path_buf();

        let manifest_path = plugin_dir.join("plugin.toml");
        manager.load_plugin(&manifest_path).unwrap();

        assert_eq!(manager.get_plugins().len(), 1);

        manager.unload_plugin("test-plugin").unwrap();
        assert_eq!(manager.get_plugins().len(), 0);
    }
}
