//! Cron Registry — Scheduled autonomous task execution.
//!
//! This module provides scheduling for autonomous operations:
//! - Cron-style scheduled prompts/tasks
//! - Enable/disable scheduling
//! - Run history tracking
//! - Global registry for centralized access
//!
//! Inspired by claw-code's team_cron_registry module.
//!
//! # Architecture
//!
//! ```text
//! CronRegistry → CronEntry { schedule, prompt, enabled, last_run_at, run_count }
//!      │
//!      ├─ create("0 9 * * *", "Run morning tests")
//!      ├─ list(enabled_only=true)
//!      └─ record_run() → updates last_run_at, run_count
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// A scheduled cron entry for autonomous task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    /// Unique cron identifier
    pub cron_id: String,
    /// Cron schedule expression (e.g., "0 9 * * *" for daily at 9am)
    pub schedule: String,
    /// Prompt/task to execute on schedule
    pub prompt: String,
    /// Optional description of what this cron does
    pub description: Option<String>,
    /// Whether this cron is active
    pub enabled: bool,
    /// Creation timestamp (unix epoch seconds)
    pub created_at: u64,
    /// Last update timestamp (unix epoch seconds)
    pub updated_at: u64,
    /// Last run timestamp (unix epoch seconds), if ever run
    pub last_run_at: Option<u64>,
    /// Number of times this cron has executed
    pub run_count: u64,
}

/// Internal registry state
#[derive(Debug, Default)]
struct CronRegistryInner {
    entries: HashMap<String, CronEntry>,
    counter: u64,
}

/// Registry for scheduled autonomous tasks
///
/// Provides cron-style scheduling for automated operations like:
/// - Running tests on a schedule
/// - Checking for merge conflicts periodically
/// - Sending status reports
/// - Cleaning up temporary resources
#[derive(Debug, Clone, Default)]
pub struct CronRegistry {
    inner: Arc<Mutex<CronRegistryInner>>,
}

impl CronRegistry {
    /// Create a new empty cron registry
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new scheduled cron entry
    ///
    /// # Arguments
    ///
    /// * `schedule` - Cron expression (e.g., "0 9 * * *" for daily at 9am)
    /// * `prompt` - The prompt/task to execute on schedule
    /// * `description` - Optional human-readable description
    ///
    /// # Returns
    ///
    /// The newly created CronEntry with `enabled=true`
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_protocol::cron_registry::CronRegistry;
    ///
    /// let registry = CronRegistry::new();
    /// let entry = registry.create(
    ///     "0 9 * * *",
    ///     "Run morning test suite and report results",
    ///     Some("Daily morning tests"),
    /// );
    /// assert!(entry.enabled);
    /// ```
    pub fn create(&self, schedule: &str, prompt: &str, description: Option<&str>) -> CronEntry {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.counter += 1;
        let ts = now_secs();
        let cron_id = format!("cron_{:08x}_{:04x}", ts, inner.counter);

        let entry = CronEntry {
            cron_id: cron_id.clone(),
            schedule: schedule.to_owned(),
            prompt: prompt.to_owned(),
            description: description.map(str::to_owned),
            enabled: true,
            created_at: ts,
            updated_at: ts,
            last_run_at: None,
            run_count: 0,
        };

        inner.entries.insert(cron_id, entry.clone());
        entry
    }

    /// Get a cron entry by ID
    ///
    /// # Returns
    ///
    /// `Some(CronEntry)` if found, `None` otherwise
    #[must_use]
    pub fn get(&self, cron_id: &str) -> Option<CronEntry> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.entries.get(cron_id).cloned()
    }

    /// List all cron entries
    ///
    /// # Arguments
    ///
    /// * `enabled_only` - If true, only return enabled entries
    ///
    /// # Returns
    ///
    /// Vec of CronEntry matching the filter
    #[must_use]
    pub fn list(&self, enabled_only: bool) -> Vec<CronEntry> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .entries
            .values()
            .filter(|e| !enabled_only || e.enabled)
            .cloned()
            .collect()
    }

    /// Delete a cron entry
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to delete
    ///
    /// # Returns
    ///
    /// `Ok(CronEntry)` if deleted, `Err(String)` if not found
    pub fn delete(&self, cron_id: &str) -> Result<CronEntry, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .entries
            .remove(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))
    }

    /// Disable a cron entry without removing it
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to disable
    ///
    /// # Returns
    ///
    /// `Ok(())` if disabled, `Err(String)` if not found
    pub fn disable(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.enabled = false;
        entry.updated_at = now_secs();
        Ok(())
    }

    /// Enable a previously disabled cron entry
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to enable
    ///
    /// # Returns
    ///
    /// `Ok(())` if enabled, `Err(String)` if not found
    pub fn enable(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.enabled = true;
        entry.updated_at = now_secs();
        Ok(())
    }

    /// Record a cron execution
    ///
    /// Updates `last_run_at` and increments `run_count`.
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron that ran
    ///
    /// # Returns
    ///
    /// `Ok(())` if recorded, `Err(String)` if not found
    pub fn record_run(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.last_run_at = Some(now_secs());
        entry.run_count += 1;
        entry.updated_at = now_secs();
        Ok(())
    }

    /// Update the prompt for a cron entry
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to update
    /// * `new_prompt` - New prompt to execute on schedule
    ///
    /// # Returns
    ///
    /// `Ok(CronEntry)` with updated prompt, `Err(String)` if not found
    pub fn update_prompt(&self, cron_id: &str, new_prompt: &str) -> Result<CronEntry, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.prompt = new_prompt.to_owned();
        entry.updated_at = now_secs();
        Ok(entry.clone())
    }

    /// Get count of cron entries
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.entries.len()
    }

    /// Check if registry is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get enabled cron entries
    #[must_use]
    pub fn enabled(&self) -> Vec<CronEntry> {
        self.list(true)
    }
}

// ── Global Registry Accessor ────────────────────────────────────────────────────────

use std::sync::OnceLock;

/// Global cron registry accessor for centralized state management.
///
/// This follows the claw-code pattern of using OnceLock for global registries,
/// enabling any part of the codebase to access shared state without threading
/// Arc<Registry> through every layer.
///
/// # Example
///
/// ```
/// use rustycode_protocol::cron_registry::global_cron_registry;
/// let registry = global_cron_registry();
/// let entry = registry.create("0 * * * *", "Hourly status check", None);
/// ```
pub fn global_cron_registry() -> &'static CronRegistry {
    static REGISTRY: OnceLock<CronRegistry> = OnceLock::new();
    REGISTRY.get_or_init(CronRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_create_and_get() {
        let registry = CronRegistry::new();

        let entry = registry.create("0 9 * * *", "Run morning tests", Some("Daily tests"));

        assert!(entry.cron_id.starts_with("cron_"));
        assert_eq!(entry.schedule, "0 9 * * *");
        assert_eq!(entry.prompt, "Run morning tests");
        assert_eq!(entry.description, Some("Daily tests".to_string()));
        assert!(entry.enabled);
        assert_eq!(entry.run_count, 0);
        assert!(entry.last_run_at.is_none());

        // Retrieve by ID
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.cron_id, entry.cron_id);
    }

    #[test]
    fn test_cron_list_with_filter() {
        let registry = CronRegistry::new();

        let entry1 = registry.create("0 9 * * *", "Morning task", None);
        let entry2 = registry.create("0 17 * * *", "Evening task", None);

        // All entries
        let all = registry.list(false);
        assert_eq!(all.len(), 2);

        // Enabled only (all are enabled)
        let enabled = registry.list(true);
        assert_eq!(enabled.len(), 2);

        // Disable one
        registry.disable(&entry1.cron_id).unwrap();

        // Now only 1 enabled
        let enabled = registry.list(true);
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].cron_id, entry2.cron_id);

        // All still 2
        let all = registry.list(false);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_cron_enable_disable() {
        let registry = CronRegistry::new();
        let entry = registry.create("*/5 * * * *", "Every 5 min task", None);

        assert!(entry.enabled);

        // Disable
        registry.disable(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert!(!retrieved.enabled);

        // Re-enable
        registry.enable(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert!(retrieved.enabled);
    }

    #[test]
    fn test_cron_record_run() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Hourly task", None);

        assert_eq!(entry.run_count, 0);
        assert!(entry.last_run_at.is_none());

        // Record first run
        registry.record_run(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.run_count, 1);
        assert!(retrieved.last_run_at.is_some());

        // Record second run
        registry.record_run(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.run_count, 2);
    }

    #[test]
    fn test_cron_update_prompt() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Original prompt", None);

        registry
            .update_prompt(&entry.cron_id, "Updated prompt")
            .unwrap();

        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.prompt, "Updated prompt");
    }

    #[test]
    fn test_cron_delete() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "To be deleted", None);

        // Delete
        let deleted = registry.delete(&entry.cron_id).unwrap();
        assert_eq!(deleted.cron_id, entry.cron_id);

        // Should be gone
        assert!(registry.get(&entry.cron_id).is_none());

        // Delete non-existent
        let result = registry.delete(&entry.cron_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_cron_len_and_is_empty() {
        let registry = CronRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.create("0 * * * *", "Task 1", None);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.create("0 9 * * *", "Task 2", None);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_cron_not_found_errors() {
        let registry = CronRegistry::new();

        assert!(registry.get("nonexistent").is_none());
        assert!(registry.disable("nonexistent").is_err());
        assert!(registry.enable("nonexistent").is_err());
        assert!(registry.record_run("nonexistent").is_err());
        assert!(registry.update_prompt("nonexistent", "new").is_err());
        assert!(registry.delete("nonexistent").is_err());
    }

    #[test]
    fn test_global_registry() {
        // First call initializes
        let registry1 = global_cron_registry();
        let entry = registry1.create("0 * * * *", "Test cron", None);

        // Second call returns same registry
        let registry2 = global_cron_registry();
        let retrieved = registry2.get(&entry.cron_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().cron_id, entry.cron_id);
    }

    #[test]
    fn test_cron_id_format() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Test", None);

        // Cron ID should start with "cron_"
        assert!(entry.cron_id.starts_with("cron_"));

        // Should have timestamp and counter parts
        let parts: Vec<&str> = entry.cron_id.split('_').collect();
        assert_eq!(parts.len(), 3); // "cron", timestamp, counter
    }

    #[test]
    fn test_enabled_helper() {
        let registry = CronRegistry::new();

        let entry1 = registry.create("0 9 * * *", "Morning", None);
        let entry2 = registry.create("0 17 * * *", "Evening", None);

        registry.disable(&entry1.cron_id).unwrap();

        // enabled() should only return enabled entries
        let enabled = registry.enabled();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].cron_id, entry2.cron_id);
    }
}
