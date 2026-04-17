//! Integration tests for request deduplication
//!
//! Tests the full request deduplication flow including:
//! - Request hash computation with various inputs
//! - Cache hit/miss behavior
//! - Cache expiration and cleanup
//! - Configuration changes at runtime

#[cfg(test)]
mod request_dedup_integration {
    use rustycode_orchestra::request_dedup::{
        CachedResponse, DeduplicationConfig, RequestDeduplicator,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn test_dedup_cache_hit_scenario() {
        let dedup = RequestDeduplicator::default();

        // Simulate first request
        let hash = RequestDeduplicator::compute_hash(
            "Tell me about Rust",
            Some("You are a helpful assistant"),
            "claude-3-opus",
        );

        let response = CachedResponse {
            response: "Rust is a systems programming language...".to_string(),
            tokens_used: 150,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Cache the response
        dedup.cache_response(hash.clone(), response).await.unwrap();

        // Check cache stats
        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 1);
        assert!(stats.enabled);

        // Simulate second identical request - should hit cache
        let cached = dedup.get_cached_response(&hash).await.unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().tokens_used, 150);
    }

    #[tokio::test]
    async fn test_dedup_different_messages_different_caches() {
        let dedup = RequestDeduplicator::default();

        let hash1 = RequestDeduplicator::compute_hash(
            "Tell me about Rust",
            Some("You are a helpful assistant"),
            "claude-3-opus",
        );

        let hash2 = RequestDeduplicator::compute_hash(
            "Tell me about Python",
            Some("You are a helpful assistant"),
            "claude-3-opus",
        );

        assert_ne!(hash1, hash2);

        let response1 = CachedResponse {
            response: "Rust is great".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let response2 = CachedResponse {
            response: "Python is great".to_string(),
            tokens_used: 100,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        dedup
            .cache_response(hash1.clone(), response1)
            .await
            .unwrap();
        dedup
            .cache_response(hash2.clone(), response2)
            .await
            .unwrap();

        let cached1 = dedup.get_cached_response(&hash1).await.unwrap();
        let cached2 = dedup.get_cached_response(&hash2).await.unwrap();

        assert_eq!(cached1.unwrap().response, "Rust is great");
        assert_eq!(cached2.unwrap().response, "Python is great");

        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 2);
    }

    #[tokio::test]
    async fn test_dedup_config_changes() {
        let mut dedup = RequestDeduplicator::new(DeduplicationConfig {
            enabled: true,
            dedup_window_secs: 300,
            max_cache_entries: 100,
        });

        let stats = dedup.cache_stats().await;
        assert!(stats.enabled);
        assert_eq!(stats.dedup_window_secs, 300);
        assert_eq!(stats.max_entries, 100);

        // Change config to disable dedup
        dedup.set_config(DeduplicationConfig {
            enabled: false,
            dedup_window_secs: 300,
            max_cache_entries: 100,
        });

        let hash = RequestDeduplicator::compute_hash("test", None, "model");
        let response = CachedResponse {
            response: "test".to_string(),
            tokens_used: 10,
            finish_reason: None,
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Caching should succeed silently
        dedup.cache_response(hash.clone(), response).await.unwrap();

        // But retrieval should return None
        let cached = dedup.get_cached_response(&hash).await.unwrap();
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_cache_operations() {
        let dedup = std::sync::Arc::new(RequestDeduplicator::default());

        let mut handles = vec![];

        // Spawn 20 concurrent operations
        for i in 0..20 {
            let dedup_clone = std::sync::Arc::clone(&dedup);
            let handle = tokio::spawn(async move {
                let hash = RequestDeduplicator::compute_hash(
                    &format!("message-{}", i),
                    Some(&format!("system-{}", i)),
                    &format!("model-{}", i),
                );

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

        // Wait for all and verify all succeeded
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.unwrap();
            assert!(result.is_some(), "Operation {} should succeed", i);
            if let Some(cached) = result {
                assert_eq!(cached.tokens_used as usize, 100 + i);
            }
        }
    }

    #[tokio::test]
    async fn test_realistic_workflow() {
        let dedup = RequestDeduplicator::default();

        // Simulate a conversation where some messages repeat

        // First request about task planning
        let plan_hash = RequestDeduplicator::compute_hash(
            "Plan the implementation of feature X",
            Some("You are an expert software engineer"),
            "claude-3-opus",
        );

        let plan_response = CachedResponse {
            response: "Here's the plan: 1. Design... 2. Implement... 3. Test...".to_string(),
            tokens_used: 500,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        dedup
            .cache_response(plan_hash.clone(), plan_response)
            .await
            .unwrap();

        // First check - should be a cache hit
        let first_check = dedup.get_cached_response(&plan_hash).await.unwrap();
        assert!(first_check.is_some());
        assert_eq!(first_check.unwrap().tokens_used, 500);

        // Simulate duplicate request (e.g., user clicked execute again)
        let second_check = dedup.get_cached_response(&plan_hash).await.unwrap();
        assert!(second_check.is_some());
        assert_eq!(second_check.unwrap().tokens_used, 500);

        // Now request something different
        let code_hash = RequestDeduplicator::compute_hash(
            "Write the code for feature X based on the plan",
            Some("You are an expert software engineer"),
            "claude-3-opus",
        );

        assert_ne!(plan_hash, code_hash);

        let code_response = CachedResponse {
            response: "fn implement_feature() { ... }".to_string(),
            tokens_used: 800,
            finish_reason: Some("stop".to_string()),
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        dedup
            .cache_response(code_hash.clone(), code_response)
            .await
            .unwrap();

        // Verify both are cached
        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 2);

        // Verify we can retrieve both independently
        let plan = dedup.get_cached_response(&plan_hash).await.unwrap();
        let code = dedup.get_cached_response(&code_hash).await.unwrap();

        assert!(plan.is_some());
        assert!(code.is_some());
        assert_eq!(plan.unwrap().tokens_used, 500);
        assert_eq!(code.unwrap().tokens_used, 800);

        // Clear and verify empty
        dedup.clear_cache().await.unwrap();
        let stats = dedup.cache_stats().await;
        assert_eq!(stats.total_entries, 0);
    }
}
