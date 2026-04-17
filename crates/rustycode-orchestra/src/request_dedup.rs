//! Request deduplication and caching for LLM calls
//!
//! This module prevents duplicate LLM requests from being sent to the API when
//! requests are accidentally re-sent or retried. By maintaining a request cache
//! with configurable TTL and bounded size, we can:
//!
//! - Detect duplicate requests based on request hash
//! - Return cached responses for duplicates (same hash within time window)
//! - Reduce API calls and associated costs
//! - Improve user experience with immediate responses
//!
//! # Architecture
//!
//! The deduplication system uses:
//! - **Request Hash**: SHA-256 hash of normalized request (message + system prompt + model)
//! - **Cache Storage**: In-memory HashMap with Arc<RwLock<>> for thread-safety
//! - **TTL (Time-To-Live)**: Configurable deduplication window (default: 5 minutes)
//! - **Bounded Size**: Configurable max entries (default: 100) with LRU-style eviction
//!
//! # Configuration
//!
//! ```rust,no_run,ignore
//! let config = DeduplicationConfig {
//!     enabled: true,
//!     dedup_window_secs: 300,      // 5 minutes
//!     max_cache_entries: 100,
//! };
//! let dedup = RequestDeduplicator::new(config);
//! ```
//!
//! # Usage
//!
//! ```rust,no_run,ignore
//! // Check cache before sending request
//! let hash = compute_request_hash(&messages, &system_prompt, &model)?;
//! if let Some(cached) = dedup.get_cached_response(&hash).await {
//!     return Ok(cached.response);
//! }
//!
//! // Send to API and cache result
//! let response = llm_provider.complete(request).await?;
//! dedup.cache_response(hash, response.clone()).await?;
//! Ok(response)
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Configuration for request deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationConfig {
    /// Enable/disable deduplication feature
    pub enabled: bool,
    /// Time window in seconds for considering requests as duplicates
    pub dedup_window_secs: u64,
    /// Maximum number of cached responses to keep in memory
    pub max_cache_entries: usize,
}

impl Default for DeduplicationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dedup_window_secs: 300, // 5 minutes default
            max_cache_entries: 100,
        }
    }
}

/// Cached LLM response with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    /// The LLM response text
    pub response: String,
    /// Number of tokens used
    pub tokens_used: u32,
    /// Finish reason (e.g., "stop", "max_tokens")
    pub finish_reason: Option<String>,
    /// When this response was cached (unix timestamp)
    pub cached_at: u64,
}

impl CachedResponse {
    /// Check if this cached response has expired
    fn is_expired(&self, dedup_window_secs: u64) -> bool {
        if let Ok(elapsed) = SystemTime::now().duration_since(UNIX_EPOCH) {
            let now = elapsed.as_secs();
            now > self.cached_at + dedup_window_secs
        } else {
            false // If we can't get current time, assume not expired
        }
    }
}

/// Internal cache entry tracking a single request and response
#[derive(Debug, Clone)]
struct CacheEntry {
    response: CachedResponse,
}

/// Thread-safe request deduplicator
pub struct RequestDeduplicator {
    config: DeduplicationConfig,
    /// In-memory cache: hash -> cached response
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl RequestDeduplicator {
    /// Create a new request deduplicator with given configuration
    pub fn new(config: DeduplicationConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Compute SHA-256 hash of a request
    ///
    /// Hashes the concatenation of:
    /// - System prompt (if present)
    /// - User messages
    /// - Model name
    ///
    /// This creates a unique fingerprint for identical requests.
    pub fn compute_hash(messages: &str, system_prompt: Option<&str>, model: &str) -> String {
        let mut hasher = Sha256::new();

        if let Some(system) = system_prompt {
            hasher.update(system.as_bytes());
            hasher.update(b"\n---\n");
        }

        hasher.update(messages.as_bytes());
        hasher.update(b"\n---\n");
        hasher.update(model.as_bytes());

        format!("{:x}", hasher.finalize())
    }

    /// Check if a response is cached for the given request hash
    ///
    /// Returns:
    /// - `Ok(Some(response))` if a valid (non-expired) cached response exists
    /// - `Ok(None)` if no cache entry exists or it has expired
    /// - `Err` if deduplication is disabled
    pub async fn get_cached_response(
        &self,
        request_hash: &str,
    ) -> anyhow::Result<Option<CachedResponse>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let cache = self.cache.read().await;

        if let Some(entry) = cache.get(request_hash) {
            if !entry.response.is_expired(self.config.dedup_window_secs) {
                debug!(
                    hash = %request_hash,
                    tokens = entry.response.tokens_used,
                    "Cache hit: returning cached response"
                );
                return Ok(Some(entry.response.clone()));
            } else {
                debug!(
                    hash = %request_hash,
                    "Cache entry expired: treating as cache miss"
                );
                return Ok(None);
            }
        }

        debug!(hash = %request_hash, "Cache miss: no entry found");
        Ok(None)
    }

    /// Cache a response for a given request hash
    ///
    /// Stores the response with current timestamp. If cache is at max capacity,
    /// the oldest entry is evicted.
    pub async fn cache_response(
        &self,
        request_hash: String,
        response: CachedResponse,
    ) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut cache = self.cache.write().await;

        // Evict oldest entry if at capacity
        if cache.len() >= self.config.max_cache_entries {
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.response.cached_at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest_key);
                debug!(
                    old_hash = %oldest_key,
                    capacity = self.config.max_cache_entries,
                    "Evicted oldest cache entry due to capacity limit"
                );
            }
        }

        let entry = CacheEntry {
            response: response.clone(),
        };

        cache.insert(request_hash.clone(), entry);

        info!(
            hash = %request_hash,
            tokens = response.tokens_used,
            cache_size = cache.len(),
            "Cached LLM response"
        );

        Ok(())
    }

    /// Clear all cached responses
    pub async fn clear_cache(&self) -> anyhow::Result<()> {
        self.cache.write().await.clear();
        info!("Cleared all cached responses");
        Ok(())
    }

    /// Remove expired entries from cache
    ///
    /// Scans the entire cache and removes entries that have exceeded
    /// the deduplication window.
    pub async fn cleanup_expired(&self) -> anyhow::Result<usize> {
        let mut cache = self.cache.write().await;
        let initial_size = cache.len();

        cache.retain(|hash, entry| {
            if entry.response.is_expired(self.config.dedup_window_secs) {
                debug!(hash = %hash, "Removing expired cache entry");
                false
            } else {
                true
            }
        });

        let removed = initial_size - cache.len();
        if removed > 0 {
            info!(
                removed = removed,
                remaining = cache.len(),
                "Cleaned up expired cache entries"
            );
        }

        Ok(removed)
    }

    /// Get current cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            total_entries: cache.len(),
            max_entries: self.config.max_cache_entries,
            enabled: self.config.enabled,
            dedup_window_secs: self.config.dedup_window_secs,
        }
    }

    /// Set configuration at runtime
    pub fn set_config(&mut self, config: DeduplicationConfig) {
        self.config = config;
    }
}

impl Default for RequestDeduplicator {
    /// Create a request deduplicator with default configuration
    fn default() -> Self {
        Self::new(DeduplicationConfig::default())
    }
}

/// Cache statistics for monitoring
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub max_entries: usize,
    pub enabled: bool,
    pub dedup_window_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Test request hashing

    #[test]
    fn compute_hash_same_input_same_output() {
        let hash1 = RequestDeduplicator::compute_hash("Hello", Some("system"), "gpt-4");
        let hash2 = RequestDeduplicator::compute_hash("Hello", Some("system"), "gpt-4");
        assert_eq!(hash1, hash2, "Same inputs should produce same hash");
    }

    #[test]
    fn compute_hash_different_messages_different_hashes() {
        let hash1 = RequestDeduplicator::compute_hash("Hello", Some("system"), "gpt-4");
        let hash2 = RequestDeduplicator::compute_hash("Goodbye", Some("system"), "gpt-4");
        assert_ne!(
            hash1, hash2,
            "Different messages should produce different hashes"
        );
    }

    #[test]
    fn compute_hash_different_system_prompts_different_hashes() {
        let hash1 = RequestDeduplicator::compute_hash("Hello", Some("system1"), "gpt-4");
        let hash2 = RequestDeduplicator::compute_hash("Hello", Some("system2"), "gpt-4");
        assert_ne!(
            hash1, hash2,
            "Different system prompts should produce different hashes"
        );
    }

    #[test]
    fn compute_hash_different_models_different_hashes() {
        let hash1 = RequestDeduplicator::compute_hash("Hello", Some("system"), "gpt-4");
        let hash2 = RequestDeduplicator::compute_hash("Hello", Some("system"), "gpt-3.5");
        assert_ne!(
            hash1, hash2,
            "Different models should produce different hashes"
        );
    }

    #[test]
    fn compute_hash_no_system_prompt() {
        let hash1 = RequestDeduplicator::compute_hash("Hello", None, "gpt-4");
        let hash2 = RequestDeduplicator::compute_hash("Hello", None, "gpt-4");
        assert_eq!(
            hash1, hash2,
            "Hash should be consistent without system prompt"
        );
    }

    #[test]
    fn compute_hash_is_stable_sha256() {
        let hash =
            RequestDeduplicator::compute_hash("test message", Some("test system"), "test-model");
        // SHA-256 produces 64-character hex strings
        assert_eq!(hash.len(), 64, "Hash should be 64-character SHA-256 hex");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should contain only hex digits"
        );
    }

    // Test cache entry expiration

    #[test]
    fn cached_response_not_expired_immediately() {
        let response = CachedResponse {
            response: "test response".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        assert!(
            !response.is_expired(300),
            "Fresh response should not be expired"
        );
    }

    #[test]
    fn cached_response_expires_after_window() {
        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 400; // 400 seconds ago

        let response = CachedResponse {
            response: "test response".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: old_timestamp,
        };

        assert!(
            response.is_expired(300),
            "Old response (400s ago) should be expired with 300s window"
        );
    }

    // Test deduplicator with async operations

    #[tokio::test]
    async fn cache_get_returns_none_when_empty() {
        let dedup = RequestDeduplicator::default();
        let result = dedup.get_cached_response("nonexistent").await.unwrap();
        assert!(result.is_none(), "Empty cache should return None");
    }

    #[tokio::test]
    async fn cache_hit_after_insert() {
        let dedup = RequestDeduplicator::default();
        let hash = "test-hash".to_string();
        let response = CachedResponse {
            response: "test response".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        dedup
            .cache_response(hash.clone(), response.clone())
            .await
            .unwrap();

        let cached = dedup.get_cached_response(&hash).await.unwrap();
        assert!(cached.is_some(), "Should return cached response");
        assert_eq!(cached.unwrap().response, "test response");
    }

    #[tokio::test]
    async fn cache_miss_after_expiration() {
        let dedup = RequestDeduplicator {
            config: DeduplicationConfig {
                enabled: true,
                dedup_window_secs: 1, // Very short window for testing
                max_cache_entries: 100,
            },
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        let hash = "test-hash".to_string();
        let response = CachedResponse {
            response: "test response".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 10, // Cached 10 seconds ago
        };

        dedup.cache_response(hash.clone(), response).await.unwrap();

        // Wait a bit to ensure expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        let cached = dedup.get_cached_response(&hash).await.unwrap();
        assert!(cached.is_none(), "Expired response should not be returned");
    }

    #[tokio::test]
    async fn disabled_dedup_returns_none() {
        let dedup = RequestDeduplicator {
            config: DeduplicationConfig {
                enabled: false,
                dedup_window_secs: 300,
                max_cache_entries: 100,
            },
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        let hash = "test-hash".to_string();
        let response = CachedResponse {
            response: "test response".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Cache should succeed silently
        dedup.cache_response(hash.clone(), response).await.unwrap();

        // But get should return None
        let cached = dedup.get_cached_response(&hash).await.unwrap();
        assert!(cached.is_none(), "Disabled dedup should return None");
    }

    #[tokio::test]
    async fn cache_respects_max_capacity() {
        let config = DeduplicationConfig {
            enabled: true,
            dedup_window_secs: 300,
            max_cache_entries: 3,
        };
        let dedup = RequestDeduplicator::new(config);

        // Insert 3 items (at capacity)
        for i in 0..3 {
            let hash = format!("hash-{}", i);
            let response = CachedResponse {
                response: format!("response-{}", i),
                tokens_used: 100 + i as u32,
                finish_reason: Some("stop".to_string()),
                cached_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + i as u64, // Stagger timestamps
            };
            dedup.cache_response(hash, response).await.unwrap();
        }

        // Insert 4th item - should evict oldest
        let hash4 = "hash-4".to_string();
        let response4 = CachedResponse {
            response: "response-4".to_string(),
            tokens_used: 104,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        dedup
            .cache_response(hash4.clone(), response4)
            .await
            .unwrap();

        // Check that we still have 3 items
        let stats = dedup.cache_stats().await;
        assert_eq!(
            stats.total_entries, 3,
            "Cache should not exceed max capacity"
        );

        // Newest item should be present
        let result = dedup.get_cached_response(&hash4).await.unwrap();
        assert!(result.is_some(), "Newest item should be in cache");
    }

    #[tokio::test]
    async fn clear_cache_removes_all() {
        let dedup = RequestDeduplicator::default();

        for i in 0..5 {
            let hash = format!("hash-{}", i);
            let response = CachedResponse {
                response: format!("response-{}", i),
                tokens_used: 100,
                finish_reason: Some("stop".to_string()),
                cached_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            dedup.cache_response(hash, response).await.unwrap();
        }

        dedup.clear_cache().await.unwrap();

        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 0, "Cache should be empty after clear");
    }

    #[tokio::test]
    async fn cleanup_expired_removes_stale_entries() {
        let dedup = RequestDeduplicator {
            config: DeduplicationConfig {
                enabled: true,
                dedup_window_secs: 300,
                max_cache_entries: 100,
            },
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Add fresh response
        let hash_fresh = "hash-fresh".to_string();
        let response_fresh = CachedResponse {
            response: "fresh".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        dedup
            .cache_response(hash_fresh.clone(), response_fresh)
            .await
            .unwrap();

        // Add old response
        let hash_old = "hash-old".to_string();
        let response_old = CachedResponse {
            response: "old".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 400, // 400 seconds ago
        };
        dedup
            .cache_response(hash_old.clone(), response_old)
            .await
            .unwrap();

        // Cleanup should remove only the old one
        let removed = dedup.cleanup_expired().await.unwrap();
        assert_eq!(removed, 1, "Should remove 1 expired entry");

        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 1, "Should have 1 entry remaining");

        // Fresh should still be there
        let fresh = dedup.get_cached_response(&hash_fresh).await.unwrap();
        assert!(fresh.is_some(), "Fresh entry should still be cached");
    }

    #[tokio::test]
    async fn concurrent_access_is_thread_safe() {
        let dedup = Arc::new(RequestDeduplicator::default());

        let mut handles = vec![];

        // Spawn 10 concurrent tasks
        for i in 0..10 {
            let dedup_clone = Arc::clone(&dedup);
            let handle = tokio::spawn(async move {
                let hash = format!("hash-{}", i);
                let response = CachedResponse {
                    response: format!("response-{}", i),
                    tokens_used: 100 + i as u32,
                    finish_reason: Some("stop".to_string()),
                    cached_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                dedup_clone
                    .cache_response(hash.clone(), response)
                    .await
                    .unwrap();
                dedup_clone.get_cached_response(&hash).await.unwrap()
            });

            handles.push(handle);
        }

        // Wait for all to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some(), "Concurrent access should succeed");
        }

        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 10, "All 10 items should be cached");
    }
}
