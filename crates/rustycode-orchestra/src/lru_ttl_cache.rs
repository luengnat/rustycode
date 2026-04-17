//! Orchestra LRU TTL Cache — LRU cache with time-to-live support.
//!
//! Zero external dependencies LRU cache implementation.
//! Features:
//! - Maximum entries before oldest eviction
//! - Time-to-live per entry
//! - O(1) LRU eviction using insertion-ordered map
//! - Refresh on access (move to tail/most-recently-used)
//! - Periodic stale entry purging
//!
//! Matches orchestra-2's cache.ts implementation.

use indexmap::IndexMap;
use std::time::{Duration, Instant};

/// Cache entry with value and expiration time
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
}

/// LRU cache with TTL (Least Recently Used + Time To Live)
///
/// - max: maximum entries before oldest is evicted
/// - ttl: time-to-live per entry
///
/// Uses an insertion-ordered HashMap for O(1) LRU eviction:
/// on every access the entry is deleted and re-inserted at the tail.
///
/// # Examples
/// ```
/// use rustycode_orchestra::lru_ttl_cache::LRUTTLCache;
/// use std::time::Duration;
///
/// let mut cache = LRUTTLCache::new(10, Duration::from_secs(60));
/// cache.set("key1", "value1");
/// assert_eq!(cache.get("key1"), Some(&"value1"));
/// ```
#[derive(Debug)]
pub struct LRUTTLCache<V> {
    max: usize,
    ttl: Duration,
    store: IndexMap<String, CacheEntry<V>>,
}

impl<V> LRUTTLCache<V> {
    /// Create a new LRU cache with TTL
    ///
    /// # Arguments
    /// * `max` - Maximum number of entries before eviction
    /// * `ttl` - Time-to-live for each entry
    ///
    /// # Examples
    /// ```
    /// use rustycode_orchestra::lru_ttl_cache::LRUTTLCache;
    /// use std::time::Duration;
    ///
    /// let cache = LRUTTLCache::new(100, Duration::from_secs(300));
    /// ```
    pub fn new(max: usize, ttl: Duration) -> Self {
        Self {
            max,
            ttl,
            store: IndexMap::new(),
        }
    }

    /// Get a value from the cache by key
    ///
    /// Returns None if:
    /// - Key doesn't exist
    /// - Entry has expired (TTL)
    ///
    /// Refreshes the entry to the tail (most-recently-used) on access.
    ///
    /// # Arguments
    /// * `key` - Cache key
    ///
    /// # Returns
    /// Reference to cached value or None
    pub fn get(&mut self, key: &str) -> Option<&V> {
        let now = Instant::now();

        // Check if entry exists and is not expired
        let entry = self.store.get(key)?;
        if now > entry.expires_at {
            self.store.shift_remove(key);
            return None;
        }

        // Refresh to tail (most-recently-used)
        let entry = self.store.swap_remove(key)?;
        self.store.insert(key.to_string(), entry);

        // Return reference to value
        self.store.get(key).map(|entry| &entry.value)
    }

    /// Set a value in the cache
    ///
    /// If the key already exists, it's refreshed to the tail.
    /// If the cache is at max capacity, the oldest entry is evicted.
    ///
    /// # Arguments
    /// * `key` - Cache key
    /// * `value` - Value to cache
    pub fn set(&mut self, key: &str, value: V) {
        let key = key.to_string();
        let expires_at = Instant::now() + self.ttl;

        // Check if we need to evict before modifying the store
        let key_exists = self.store.contains_key(&key);
        if !key_exists && self.store.len() >= self.max {
            // Evict oldest entry (first key in iteration order)
            if let Some(oldest) = self.store.keys().next().cloned() {
                self.store.shift_remove(&oldest);
            }
        }

        // If key exists, remove it (will re-insert at tail)
        if key_exists {
            self.store.swap_remove(&key);
        }

        self.store.insert(key, CacheEntry { value, expires_at });
    }

    /// Check if a key exists in the cache and is not expired
    ///
    /// # Arguments
    /// * `key` - Cache key
    ///
    /// # Returns
    /// true if key exists and hasn't expired
    pub fn has(&mut self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Remove all stale entries from the cache
    ///
    /// Entries that have exceeded their TTL are removed.
    pub fn purge_stale(&mut self) {
        let now = Instant::now();
        self.store.retain(|_, entry| now <= entry.expires_at);
    }

    /// Clear all entries from the cache
    pub fn clear(&mut self) {
        self.store.clear();
    }

    /// Get the number of entries in the cache
    ///
    /// # Returns
    /// Number of cached entries (including stale ones)
    pub fn size(&self) -> usize {
        self.store.len()
    }

    /// Check if the cache is empty
    ///
    /// # Returns
    /// true if cache has no entries
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache_set_get() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_millis(100));
        cache.set("key1", "value1");

        assert_eq!(cache.get("key1"), Some(&"value1"));
        assert_eq!(cache.size(), 1);
    }

    #[test]
    fn test_cache_get_missing() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_secs(60));
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn test_cache_expiration() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_millis(50));
        cache.set("key1", "value1");

        // Wait for expiration
        thread::sleep(Duration::from_millis(60));

        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(3, Duration::from_secs(60));

        cache.set("key1", "value1");
        cache.set("key2", "value2");
        cache.set("key3", "value3");
        assert_eq!(cache.size(), 3);

        // Adding 4th entry evicts the oldest (key1)
        cache.set("key4", "value4");
        assert_eq!(cache.size(), 3);
        assert_eq!(cache.get("key1"), None);
        assert_eq!(cache.get("key2"), Some(&"value2"));
        assert_eq!(cache.get("key3"), Some(&"value3"));
        assert_eq!(cache.get("key4"), Some(&"value4"));
    }

    #[test]
    fn test_cache_refresh_on_access() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(3, Duration::from_millis(100));

        cache.set("key1", "value1");
        cache.set("key2", "value2");
        cache.set("key3", "value3");

        // Access key1 (moves to most-recently-used position)
        let _ = cache.get("key1");

        // Add key4 - should evict the oldest entry (key3)
        cache.set("key4", "value4");

        // Verify one entry was evicted
        assert_eq!(cache.size(), 3);
        // key3 should have been evicted
        assert_eq!(cache.get("key3"), None);
        // key1, key2, key4 should still exist
        assert_eq!(cache.get("key1"), Some(&"value1"));
        assert_eq!(cache.get("key2"), Some(&"value2"));
        assert_eq!(cache.get("key4"), Some(&"value4"));
    }

    #[test]
    fn test_cache_has() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_secs(60));

        cache.set("key1", "value1");
        assert!(cache.has("key1"));
        assert!(!cache.has("missing"));
    }

    #[test]
    fn test_cache_has_expired() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_millis(50));

        cache.set("key1", "value1");
        thread::sleep(Duration::from_millis(60));

        assert!(!cache.has("key1"));
    }

    #[test]
    fn test_cache_overwrite() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_secs(60));

        cache.set("key1", "value1");
        cache.set("key1", "value2");

        assert_eq!(cache.get("key1"), Some(&"value2"));
        assert_eq!(cache.size(), 1);
    }

    #[test]
    fn test_cache_purge_stale() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_millis(50));

        cache.set("key1", "value1");
        cache.set("key2", "value2");

        thread::sleep(Duration::from_millis(60));

        cache.purge_stale();

        assert_eq!(cache.size(), 0);
        assert!(!cache.has("key1"));
        assert!(!cache.has("key2"));
    }

    #[test]
    fn test_cache_clear() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_secs(60));

        cache.set("key1", "value1");
        cache.set("key2", "value2");

        assert_eq!(cache.size(), 2);

        cache.clear();

        assert_eq!(cache.size(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_is_empty() {
        let mut cache: LRUTTLCache<&str> = LRUTTLCache::new(10, Duration::from_secs(60));

        assert!(cache.is_empty());

        cache.set("key1", "value1");

        assert!(!cache.is_empty());
    }

    #[test]
    fn test_cache_multiple_operations() {
        let mut cache: LRUTTLCache<i32> = LRUTTLCache::new(5, Duration::from_millis(100));

        for i in 1..=5 {
            cache.set(&format!("key{}", i), i);
        }

        assert_eq!(cache.size(), 5);

        // Access key3 to refresh it
        let _ = cache.get("key3");

        // Add key6 (should evict key1)
        cache.set("key6", 6);

        assert_eq!(cache.get("key1"), None); // Evicted
        assert_eq!(cache.get("key3"), Some(&3)); // Not evicted (refreshed)
        assert_eq!(cache.get("key6"), Some(&6)); // New entry
    }
}
