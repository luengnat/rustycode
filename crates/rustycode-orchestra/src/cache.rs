//! Orchestra Cache Invalidation — Unified Cache Management
//!
//! Provides centralized cache invalidation for all Orchestra runtime caches.
//! Matches orchestra-2's cache.ts implementation.
//!
//! Critical for production autonomous systems to prevent stale reads
//! after file writes, state changes, or any operation that modifies .orchestra/ contents.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Cache identifier
pub type CacheId = &'static str;

/// Function that clears a specific cache
pub type CacheClearFn = Box<dyn Fn() -> Result<()> + Send + Sync>;

/// Cache registry that manages all registered caches
struct CacheRegistry {
    /// Map of cache ID to clear function
    caches: HashMap<CacheId, CacheClearFn>,
}

impl CacheRegistry {
    fn new() -> Self {
        Self {
            caches: HashMap::new(),
        }
    }

    fn register(&mut self, id: CacheId, clear_fn: CacheClearFn) {
        self.caches.insert(id, clear_fn);
    }

    fn clear(&mut self, id: CacheId) -> Result<()> {
        if let Some(clear_fn) = self.caches.get(id) {
            clear_fn()?;
        }
        Ok(())
    }

    fn clear_all(&mut self) -> Result<()> {
        let mut errors = Vec::new();

        for (id, clear_fn) in &self.caches {
            if let Err(e) = clear_fn() {
                errors.push(format!("{}: {}", id, e));
            }
        }

        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to clear some caches: {}",
                errors.join(", ")
            ));
        }

        Ok(())
    }
}

// ─── Global State ─────────────────────────────────────────────────────────────

/// Global cache registry (thread-safe, lazily initialized)
static REGISTRY: OnceLock<Arc<Mutex<CacheRegistry>>> = OnceLock::new();

/// Get or initialize the global cache registry
fn registry() -> Arc<Mutex<CacheRegistry>> {
    REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(CacheRegistry::new())))
        .clone()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Register a cache with a clear function
///
/// # Arguments
/// * `id` - Unique identifier for this cache
/// * `clear_fn` - Function that clears the cache
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::cache::*;
///
/// register_cache("state_cache", Box::new(|| {
///     // Clear state cache
///     STATE_CACHE.lock().unwrap_or_else(|e| e.into_inner()).clear();
///     Ok(())
/// }));
/// ```
pub fn register_cache(id: CacheId, clear_fn: CacheClearFn) {
    let reg = registry();
    let mut registry_lock = reg.lock().unwrap_or_else(|e| e.into_inner());
    registry_lock.register(id, clear_fn);
}

/// Invalidate a specific cache by ID
///
/// # Arguments
/// * `id` - Cache identifier to invalidate
///
/// # Example
/// ```rust,no_run
/// invalidate_cache("state_cache")?;
/// ```
pub fn invalidate_cache(id: CacheId) -> Result<()> {
    let reg = registry();
    let mut registry_lock = reg.lock().unwrap_or_else(|e| e.into_inner());
    registry_lock.clear(id)
}

/// Invalidate all registered caches
///
/// Call this after file writes, milestone transitions, merge reconciliation,
/// or any operation that changes .orchestra/ contents on disk.
///
/// # Why This Matters
/// Forgetting to clear any single cache causes stale reads:
/// - State cache shows old milestone
/// - Path cache shows deleted files
/// - Parse cache shows old content
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::cache::*;
///
/// // After writing state to disk
/// atomic_write(&state_path, &state_content)?;
///
/// // Invalidate all caches to prevent stale reads
/// invalidate_all_caches()?;
/// ```
pub fn invalidate_all_caches() -> Result<()> {
    let reg = registry();
    let mut registry_lock = reg.lock().unwrap_or_else(|e| e.into_inner());
    registry_lock.clear_all()
}

/// Get the number of registered caches
///
/// Useful for debugging and testing.
///
/// # Example
/// ```rust,no_run
/// let count = cache_count();
/// println!("Registered {} caches", count);
/// ```
pub fn cache_count() -> usize {
    let reg = registry();
    let registry_lock = reg.lock().unwrap_or_else(|e| e.into_inner());
    registry_lock.caches.len()
}

/// Check if a cache is registered
///
/// # Arguments
/// * `id` - Cache identifier to check
///
/// # Example
/// ```rust,no_run
/// if is_cache_registered("state_cache") {
///     println!("State cache is registered");
/// }
/// ```
pub fn is_cache_registered(id: CacheId) -> bool {
    let reg = registry();
    let registry_lock = reg.lock().unwrap_or_else(|e| e.into_inner());
    registry_lock.caches.contains_key(id)
}

/// Unregister a cache
///
/// # Arguments
/// * `id` - Cache identifier to unregister
///
/// # Example
/// ```rust,no_run
/// unregister_cache("old_cache")?;
/// ```
pub fn unregister_cache(id: CacheId) {
    let reg = registry();
    let mut registry_lock = reg.lock().unwrap_or_else(|e| e.into_inner());
    registry_lock.caches.remove(id);
}

// ─── Built-in Caches ───────────────────────────────────────────────────────────

/// State cache - stores derived state from ROADMAP.md and PLAN.md
///
/// This is the most critical cache - it stores the current milestone,
/// slice, and task information. Forgetting to clear it causes the agent
/// to work on stale tasks.
pub mod state_cache {
    use super::*;
    use std::sync::Mutex;

    static STATE_CACHE: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();

    pub fn get() -> Option<serde_json::Value> {
        STATE_CACHE
            .get()
            .and_then(|cache| cache.lock().ok())
            .and_then(|v| v.clone())
    }

    pub fn set(value: serde_json::Value) {
        if let Ok(mut cache) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
            *cache = Some(value);
        }
    }

    pub fn clear() -> Result<()> {
        if let Ok(mut cache) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
            *cache = None;
        }
        Ok(())
    }

    pub fn init() {
        // Register the state cache with the global registry
        register_cache(
            "state_cache",
            Box::new(|| {
                clear()?;
                tracing::debug!("State cache invalidated");
                Ok(())
            }),
        );
    }
}

/// Path cache - stores directory listing results
///
/// Caches readdir() results to avoid hitting the filesystem repeatedly.
/// Forgetting to clear it causes the agent to not see new files.
pub mod path_cache {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static PATH_CACHE: OnceLock<Mutex<HashMap<PathBuf, Vec<PathBuf>>>> = OnceLock::new();

    pub fn get(dir: &PathBuf) -> Option<Vec<PathBuf>> {
        PATH_CACHE
            .get()
            .and_then(|cache| cache.lock().ok())
            .and_then(|map| map.get(dir).cloned())
    }

    pub fn set(dir: PathBuf, entries: Vec<PathBuf>) {
        if let Ok(mut cache) = PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
            cache.insert(dir, entries);
        }
    }

    pub fn clear() -> Result<()> {
        if let Ok(mut cache) = PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
            cache.clear();
        }
        Ok(())
    }

    pub fn init() {
        // Register the path cache with the global registry
        register_cache(
            "path_cache",
            Box::new(|| {
                clear()?;
                tracing::debug!("Path cache invalidated");
                Ok(())
            }),
        );
    }
}

/// Parse cache - stores parsed markdown files
///
/// Caches parsed ROADMAP.md, PLAN.md, and other markdown files.
/// Forgetting to clear it causes the agent to see old task descriptions.
pub mod parse_cache {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Clone)]
    pub struct ParsedFile {
        pub content: String,
        pub parsed: serde_json::Value,
        pub modified_time: std::time::SystemTime,
    }

    static PARSE_CACHE: OnceLock<Mutex<HashMap<PathBuf, ParsedFile>>> = OnceLock::new();

    pub fn get(path: &PathBuf) -> Option<ParsedFile> {
        PARSE_CACHE
            .get()
            .and_then(|cache| cache.lock().ok())
            .and_then(|map| map.get(path).cloned())
    }

    pub fn set(path: PathBuf, parsed: ParsedFile) {
        if let Ok(mut cache) = PARSE_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            cache.insert(path, parsed);
        }
    }

    pub fn clear() -> Result<()> {
        if let Ok(mut cache) = PARSE_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            cache.clear();
        }
        Ok(())
    }

    pub fn init() {
        // Register the parse cache with the global registry
        register_cache(
            "parse_cache",
            Box::new(|| {
                clear()?;
                tracing::debug!("Parse cache invalidated");
                Ok(())
            }),
        );
    }
}

/// Initialize all built-in caches
///
/// Call this at application startup to register the standard caches.
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::cache::*;
///
/// fn main() {
///     init_builtin_caches();
///     // ... application code ...
/// }
/// ```
pub fn init_builtin_caches() {
    state_cache::init();
    path_cache::init();
    parse_cache::init();
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_cache_registration() {
        // Register a test cache
        register_cache("test_cache", Box::new(|| Ok(())));

        // Verify it's registered
        assert!(is_cache_registered("test_cache"));

        // Unregister it
        unregister_cache("test_cache");
        assert!(!is_cache_registered("test_cache"));
    }

    #[test]
    fn test_invalidate_single_cache() {
        let cleared = Arc::new(Mutex::new(false));
        let cleared_clone = cleared.clone();

        register_cache(
            "single_test",
            Box::new(move || {
                *cleared_clone.lock().unwrap_or_else(|e| e.into_inner()) = true;
                Ok(())
            }),
        );

        invalidate_cache("single_test").unwrap();

        assert!(*cleared.lock().unwrap_or_else(|e| e.into_inner()));

        unregister_cache("single_test");
    }

    #[test]
    fn test_invalidate_all_caches() {
        // Clean up any leftover caches from parallel test execution
        // (tests share global state via register_cache)
        for id in &["failing_cache", "test_cache", "single_test"] {
            if is_cache_registered(id) {
                unregister_cache(id);
            }
        }

        let cleared1 = Arc::new(Mutex::new(false));
        let cleared2 = Arc::new(Mutex::new(false));

        let cleared1_clone = cleared1.clone();
        let cleared2_clone = cleared2.clone();

        register_cache(
            "cache1",
            Box::new(move || {
                *cleared1_clone.lock().unwrap_or_else(|e| e.into_inner()) = true;
                Ok(())
            }),
        );

        register_cache(
            "cache2",
            Box::new(move || {
                *cleared2_clone.lock().unwrap_or_else(|e| e.into_inner()) = true;
                Ok(())
            }),
        );

        invalidate_all_caches().unwrap();

        assert!(*cleared1.lock().unwrap_or_else(|e| e.into_inner()));
        assert!(*cleared2.lock().unwrap_or_else(|e| e.into_inner()));

        unregister_cache("cache1");
        unregister_cache("cache2");
    }

    #[test]
    fn test_state_cache() {
        state_cache::init();

        // Set a value
        let value = serde_json::json!({"status": "active"});
        state_cache::set(value.clone());

        // Get it back
        let retrieved = state_cache::get().unwrap();
        assert_eq!(retrieved, value);

        // Clear it
        state_cache::clear().unwrap();
        assert!(state_cache::get().is_none());
    }

    #[test]
    fn test_path_cache() {
        path_cache::init();

        // Set a value
        let dir = PathBuf::from("/test/dir");
        let entries = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
        path_cache::set(dir.clone(), entries.clone());

        // Get it back
        let retrieved = path_cache::get(&dir).unwrap();
        assert_eq!(retrieved, entries);

        // Clear it
        path_cache::clear().unwrap();
        assert!(path_cache::get(&dir).is_none());
    }

    #[test]
    fn test_parse_cache() {
        parse_cache::init();

        // Set a value
        let path = PathBuf::from("/test/file.md");
        let parsed = parse_cache::ParsedFile {
            content: "# Test".to_string(),
            parsed: serde_json::json!({"title": "Test"}),
            modified_time: std::time::SystemTime::now(),
        };
        parse_cache::set(path.clone(), parsed.clone());

        // Get it back
        let retrieved = parse_cache::get(&path).unwrap();
        assert_eq!(retrieved.content, parsed.content);

        // Clear it
        parse_cache::clear().unwrap();
        assert!(parse_cache::get(&path).is_none());
    }

    #[test]
    fn test_builtin_caches() {
        // Clean up any leftover caches from parallel test execution
        for id in &[
            "failing_cache",
            "test_cache",
            "single_test",
            "cache1",
            "cache2",
        ] {
            if is_cache_registered(id) {
                unregister_cache(id);
            }
        }

        init_builtin_caches();

        // All three built-in caches should be registered
        assert!(is_cache_registered("state_cache"));
        assert!(is_cache_registered("path_cache"));
        assert!(is_cache_registered("parse_cache"));

        // Invalidate all (safe now — no failing caches registered)
        if let Err(e) = invalidate_all_caches() {
            let msg = e.to_string();
            assert!(
                !msg.contains("state_cache")
                    && !msg.contains("path_cache")
                    && !msg.contains("parse_cache"),
                "Built-in cache invalidation failed: {}",
                msg
            );
        }
    }

    #[test]
    fn test_cache_clear_error_handling() {
        // Clean up leftover caches from parallel test execution
        for id in &[
            "failing_cache",
            "test_cache",
            "single_test",
            "cache1",
            "cache2",
        ] {
            if is_cache_registered(id) {
                unregister_cache(id);
            }
        }

        let cleared = Arc::new(Mutex::new(false));
        let cleared_clone = cleared.clone();

        // Register a cache that fails
        register_cache(
            "failing_cache",
            Box::new(move || {
                *cleared_clone.lock().unwrap_or_else(|e| e.into_inner()) = true;
                Err(anyhow::anyhow!("Cache clear failed"))
            }),
        );

        // invalidate_all_caches should return error but still clear other caches
        let result = invalidate_all_caches();

        // The failing cache was attempted
        assert!(result.is_err());
        assert!(*cleared.lock().unwrap_or_else(|e| e.into_inner()));

        unregister_cache("failing_cache");
    }
}
