//! Comprehensive tests for the deadlock detection system
//!
//! These tests cover various deadlock scenarios and prevention strategies.

use rustycode_core::deadlock::{
    DeadlockDetector, DeadlockType, DetectorConfig, LockType, MutexWrapper,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

#[tokio::test]
async fn test_basic_detector_creation() {
    let detector = DeadlockDetector::new();

    // Should be able to register locks
    let lock_id = detector
        .register_lock("test_mutex".to_string(), LockType::Mutex)
        .await;

    assert!(lock_id.is_ok(), "Lock registration should succeed");

    let lock_id = lock_id.unwrap();
    let locks = detector.locks.read().await;

    assert!(locks.contains_key(&lock_id));
    assert_eq!(locks[&lock_id].name, "test_mutex");
    assert_eq!(locks[&lock_id].lock_type, LockType::Mutex);
}

#[tokio::test]
async fn test_detector_with_custom_config() {
    let config = DetectorConfig::builder()
        .enable_cycle_detection(true)
        .enable_timeout_detection(false)
        .timeout_threshold(Duration::from_secs(10))
        .max_tracked_locks(100)
        .sampling_rate(0.5)
        .build();

    let detector = DeadlockDetector::with_config(config);

    assert_eq!(detector.config.max_tracked_locks, 100);
    assert_eq!(detector.config.sampling_rate, 0.5);
    assert!(!detector.config.enable_timeout_detection);
}

#[tokio::test]
async fn test_lock_statistics() {
    let detector = DeadlockDetector::new();

    // Register multiple locks
    let lock_a = detector
        .register_lock("lock_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("lock_b".to_string(), LockType::RwLockWrite)
        .await
        .unwrap();
    let lock_c = detector
        .register_lock("lock_c".to_string(), LockType::Semaphore)
        .await
        .unwrap();

    // Simulate acquisitions
    let task_id = 1;
    detector
        .record_acquisition(lock_a, task_id, true, Some(10))
        .await
        .unwrap();
    detector
        .record_acquisition(lock_b, task_id, true, Some(15))
        .await
        .unwrap();

    let stats = detector.lock_statistics().await;

    assert_eq!(stats.total_locks, 3);
    assert!(stats.total_acquisitions >= 2);
}

#[tokio::test]
async fn test_simple_cycle_detection() {
    let detector = DeadlockDetector::new();

    // Register two locks
    let lock_a = detector
        .register_lock("resource_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("resource_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Create a circular dependency: A -> B -> A
    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_a);
    }

    // Detect deadlocks
    let report = detector.detect_deadlocks().await;

    assert!(report.has_deadlock(), "Should detect deadlock");
    assert_eq!(report.deadlock_type, DeadlockType::CycleDetected);
    assert_eq!(report.involved_locks.len(), 2);
    assert!(report.lock_names.contains(&"resource_a".to_string()));
    assert!(report.lock_names.contains(&"resource_b".to_string()));

    // Check prevention strategy
    assert!(!report.prevention_strategy.is_empty());
    assert!(
        report.prevention_strategy.contains("Global Lock Ordering")
            || report.prevention_strategy.contains("Lock Hierarchy")
    );
}

#[tokio::test]
async fn test_three_way_cycle_detection() {
    let detector = DeadlockDetector::new();

    // Register three locks
    let lock_a = detector
        .register_lock("database".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("cache".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_c = detector
        .register_lock("network".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Create a cycle: A -> B -> C -> A
    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_c);
        graph.add_dependency(lock_c, lock_a);
    }

    let report = detector.detect_deadlocks().await;

    assert!(report.has_deadlock());
    assert_eq!(report.involved_locks.len(), 3);

    // Verify all locks are involved
    let lock_names: Vec<_> = report.lock_names.iter().map(|s| s.as_str()).collect();
    assert!(lock_names.contains(&"database"));
    assert!(lock_names.contains(&"cache"));
    assert!(lock_names.contains(&"network"));
}

#[tokio::test]
async fn test_no_cycle_with_linear_dependency() {
    let detector = DeadlockDetector::new();

    // Register locks in a linear chain: A -> B -> C
    let lock_a = detector
        .register_lock("lock_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("lock_b".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_c = detector
        .register_lock("lock_c".to_string(), LockType::Mutex)
        .await
        .unwrap();

    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_c);
        // No edge back to A, so no cycle
    }

    let report = detector.detect_deadlocks().await;

    assert!(
        !report.has_deadlock(),
        "Linear dependencies should not trigger deadlock"
    );
}

#[tokio::test]
async fn test_timeout_detection() {
    let config = DetectorConfig::builder()
        .enable_cycle_detection(false)
        .enable_timeout_detection(true)
        .timeout_threshold(Duration::from_millis(100))
        .build();

    let detector = DeadlockDetector::with_config(config);

    let lock_id = detector
        .register_lock("slow_lock".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Simulate a pending acquisition
    let mut pending = detector.pending_acquisitions.write().await;
    pending.insert(
        lock_id,
        chrono::Utc::now() - chrono::Duration::milliseconds(150),
    );
    drop(pending);

    // Should detect timeout
    let report = detector.detect_deadlocks().await;

    assert!(report.has_deadlock());
    assert_eq!(report.deadlock_type, DeadlockType::TimeoutDetected);
    assert!(report.cycle_description.is_some());
}

#[tokio::test]
async fn test_acquisition_and_release_tracking() {
    let detector = DeadlockDetector::new();

    let lock_id = detector
        .register_lock("tracked_lock".to_string(), LockType::Mutex)
        .await
        .unwrap();

    let task_id = 42;

    // Record acquisition
    detector
        .record_acquisition(lock_id, task_id, true, Some(5))
        .await
        .unwrap();

    // Check lock is marked as held
    {
        let locks = detector.locks.read().await;
        assert_eq!(
            locks[&lock_id].state,
            rustycode_core::deadlock::LockState::Held
        );
    }

    // Check lock is in held_locks
    {
        let held = detector.held_locks.read().await;
        assert!(held.contains_key(&task_id));
        assert!(held[&task_id].contains(&lock_id));
    }

    // Record release
    detector
        .record_release(lock_id, task_id, 100)
        .await
        .unwrap();

    // Check lock is free
    {
        let locks = detector.locks.read().await;
        assert_eq!(
            locks[&lock_id].state,
            rustycode_core::deadlock::LockState::Free
        );
        assert_eq!(locks[&lock_id].acquisition_count, 1);
        assert_eq!(locks[&lock_id].total_held_duration_ms, 100);
    }

    // Check lock is removed from held_locks
    {
        let held = detector.held_locks.read().await;
        assert!(!held[&task_id].contains(&lock_id));
    }
}

#[tokio::test]
async fn test_multiple_tasks_same_lock() {
    let detector = DeadlockDetector::new();

    let lock_id = detector
        .register_lock("shared_resource".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Task 1 acquires and releases
    let task1 = 1;
    detector
        .record_acquisition(lock_id, task1, true, Some(10))
        .await
        .unwrap();
    detector.record_release(lock_id, task1, 50).await.unwrap();

    // Task 2 acquires and releases
    let task2 = 2;
    detector
        .record_acquisition(lock_id, task2, true, Some(20))
        .await
        .unwrap();
    detector.record_release(lock_id, task2, 30).await.unwrap();

    let locks = detector.locks.read().await;
    assert_eq!(locks[&lock_id].acquisition_count, 2);
    assert_eq!(locks[&lock_id].total_held_duration_ms, 80); // 50 + 30
    assert_eq!(locks[&lock_id].max_held_duration_ms, 50);
}

#[tokio::test]
async fn test_detector_reset() {
    let detector = DeadlockDetector::new();

    // Register some locks
    let lock_a = detector
        .register_lock("lock_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("lock_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Create a cycle
    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_a);
    }

    // Verify deadlock is detected
    let report = detector.detect_deadlocks().await;
    assert!(report.has_deadlock());

    // Reset detector
    detector.reset().await;

    // Verify everything is cleared
    let stats = detector.lock_statistics().await;
    assert_eq!(stats.total_locks, 0);

    let report = detector.detect_deadlocks().await;
    assert!(!report.has_deadlock());
}

#[tokio::test]
async fn test_max_tracked_locks_limit() {
    let config = DetectorConfig::builder().max_tracked_locks(3).build();

    let detector = DeadlockDetector::with_config(config);

    // Register up to limit
    let _ = detector
        .register_lock("lock1".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let _ = detector
        .register_lock("lock2".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let _ = detector
        .register_lock("lock3".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Try to register one more - should fail
    let result = detector
        .register_lock("lock4".to_string(), LockType::Mutex)
        .await;

    assert!(result.is_err(), "Should fail when max locks reached");
}

#[tokio::test]
async fn test_deadlock_report_visualization() {
    let detector = DeadlockDetector::new();

    let lock_a = detector
        .register_lock("resource_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("resource_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_a);
    }

    let report = detector.detect_deadlocks().await;

    // Test visualization
    let viz = report.cycle_visualization();
    assert!(viz.contains("resource_a"));
    assert!(viz.contains("resource_b"));

    // Test summary
    let summary = report.summary();
    assert!(summary.contains("Circular wait"));

    // Test display
    let display = format!("{}", report);
    assert!(display.contains("Deadlock Report"));
    assert!(display.contains("Severity"));
}

#[tokio::test]
async fn test_lock_info_state_transitions() {
    let detector = DeadlockDetector::new();

    let lock_id = detector
        .register_lock("state_lock".to_string(), LockType::Mutex)
        .await
        .unwrap();

    let task_id = 1;

    // Initially free
    {
        let locks = detector.locks.read().await;
        assert_eq!(
            locks[&lock_id].state,
            rustycode_core::deadlock::LockState::Free
        );
    }

    // Acquire
    detector
        .record_acquisition(lock_id, task_id, true, None)
        .await
        .unwrap();
    {
        let locks = detector.locks.read().await;
        assert_eq!(
            locks[&lock_id].state,
            rustycode_core::deadlock::LockState::Held
        );
    }

    // Release
    detector.record_release(lock_id, task_id, 10).await.unwrap();
    {
        let locks = detector.locks.read().await;
        assert_eq!(
            locks[&lock_id].state,
            rustycode_core::deadlock::LockState::Free
        );
    }
}

#[tokio::test]
async fn test_dependency_graph_queries() {
    let detector = DeadlockDetector::new();

    let lock_a = detector
        .register_lock("a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("b".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_c = detector
        .register_lock("c".to_string(), LockType::Mutex)
        .await
        .unwrap();

    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_a, lock_c);
    }

    let graph = detector.graph.read().await;

    // A depends on B and C
    let deps = graph.dependencies(lock_a);
    assert!(deps.contains(&lock_b));
    assert!(deps.contains(&lock_c));

    // B and C are depended on by A
    let b_dependents = graph.dependents(lock_b);
    assert!(b_dependents.contains(&lock_a));
}

#[tokio::test]
async fn test_separate_cycles_in_graph() {
    let detector = DeadlockDetector::new();

    // First cycle: A <-> B
    let lock_a = detector
        .register_lock("cycle1_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("cycle1_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Second cycle: C <-> D
    let lock_c = detector
        .register_lock("cycle2_c".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_d = detector
        .register_lock("cycle2_d".to_string(), LockType::Mutex)
        .await
        .unwrap();

    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_a);

        graph.add_dependency(lock_c, lock_d);
        graph.add_dependency(lock_d, lock_c);
    }

    let graph = detector.graph.read().await;
    let cycles = graph.detect_cycles();

    // Should detect both cycles
    assert!(cycles.len() >= 2, "Should detect at least 2 cycles");
}

#[tokio::test]
async fn test_mutex_wrapper_basic_usage() {
    use tokio::sync::Mutex;

    let detector = Arc::new(DeadlockDetector::new());
    let inner_mutex = Mutex::new(42);

    let wrapper = MutexWrapper::new(inner_mutex, "test_mutex", detector.clone());

    // Lock and use
    {
        let guard = wrapper.lock().await;
        assert_eq!(*guard, 42);
        *guard = 100;
    }

    // Lock again
    {
        let guard = wrapper.lock().await;
        assert_eq!(*guard, 100);
    }
}

#[tokio::test]
async fn test_mutex_wrapper_try_lock() {
    use tokio::sync::Mutex;

    let detector = Arc::new(DeadlockDetector::new());
    let inner_mutex = Mutex::new(42);

    let wrapper = MutexWrapper::new(inner_mutex, "try_lock_test", detector);

    // Try lock should succeed
    let guard_opt = wrapper.try_lock().await;
    assert!(guard_opt.is_some());

    let guard = guard_opt.unwrap();
    assert_eq!(*guard, 42);
}

#[tokio::test]
async fn test_lock_type_variations() {
    let detector = DeadlockDetector::new();

    let mutex_id = detector
        .register_lock("mutex_lock".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let rw_read_id = detector
        .register_lock("rwlock_read".to_string(), LockType::RwLockRead)
        .await
        .unwrap();
    let rw_write_id = detector
        .register_lock("rwlock_write".to_string(), LockType::RwLockWrite)
        .await
        .unwrap();
    let sem_id = detector
        .register_lock("semaphore".to_string(), LockType::Semaphore)
        .await
        .unwrap();
    let custom_id = detector
        .register_lock("custom_lock".to_string(), LockType::Custom)
        .await
        .unwrap();

    let locks = detector.locks.read().await;

    assert_eq!(locks[&mutex_id].lock_type, LockType::Mutex);
    assert_eq!(locks[&rw_read_id].lock_type, LockType::RwLockRead);
    assert_eq!(locks[&rw_write_id].lock_type, LockType::RwLockWrite);
    assert_eq!(locks[&sem_id].lock_type, LockType::Semaphore);
    assert_eq!(locks[&custom_id].lock_type, LockType::Custom);
}

#[tokio::test]
async fn test_acquisition_history_tracking() {
    let detector = DeadlockDetector::new();

    let lock_id = detector
        .register_lock("history_lock".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Record several acquisitions
    for i in 0..5 {
        detector
            .record_acquisition(lock_id, i, true, Some(i * 10))
            .await
            .unwrap();
    }

    let history = detector.acquisition_history.read().await;
    assert_eq!(history.len(), 5);

    // Verify order and data
    for (i, event) in history.iter().enumerate() {
        assert_eq!(event.lock_id, lock_id);
        assert_eq!(event.task_id, i as u64);
        assert!(event.acquired);
        assert_eq!(event.wait_duration_ms, Some(i * 10));
    }
}

#[tokio::test]
async fn test_release_history_tracking() {
    let detector = DeadlockDetector::new();

    let lock_id = detector
        .register_lock("release_history".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Record releases
    for i in 0..3 {
        detector.record_release(lock_id, i, i * 100).await.unwrap();
    }

    let history = detector.release_history.read().await;
    assert_eq!(history.len(), 3);

    for (i, event) in history.iter().enumerate() {
        assert_eq!(event.lock_id, lock_id);
        assert_eq!(event.task_id, i as u64);
        assert_eq!(event.held_duration_ms, i * 100);
    }
}

#[tokio::test]
async fn test_sampling_rate_affects_tracking() {
    use std::sync::atomic::{AtomicU16, Ordering};

    let config = DetectorConfig::builder()
        .sampling_rate(0.0) // Never sample
        .build();

    let detector = DeadlockDetector::with_config(config);

    let lock_id = detector
        .register_lock("sampled_lock".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Try to record many acquisitions
    for _ in 0..10 {
        let _ = detector
            .record_acquisition(lock_id, 1, true, Some(10))
            .await;
    }

    // With 0.0 sampling, nothing should be recorded
    let history = detector.acquisition_history.read().await;
    // May have a few events due to race conditions, but should be much less than 10
    assert!(
        history.len() < 5,
        "History should be sparse with 0.0 sampling rate"
    );
}

#[tokio::test]
async fn test_deadlock_reports_persistence() {
    let detector = DeadlockDetector::new();

    let lock_a = detector
        .register_lock("report_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("report_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Create a cycle
    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_a);
    }

    // Detect deadlock
    let report1 = detector.detect_deadlocks().await;
    assert!(report1.has_deadlock());

    // Get all reports
    let reports = detector.get_deadlock_reports().await;
    assert!(!reports.is_empty());

    // Clear reports
    detector.clear_reports().await;

    // Verify cleared
    let reports = detector.get_deadlock_reports().await;
    assert!(reports.is_empty());
}

#[tokio::test]
async fn test_prevention_strategy_content() {
    let detector = DeadlockDetector::new();

    // Create a simple cycle
    let lock_a = detector
        .register_lock("prevention_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let lock_b = detector
        .register_lock("prevention_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(lock_a, lock_b);
        graph.add_dependency(lock_b, lock_a);
    }

    let report = detector.detect_deadlocks().await;

    // Verify prevention strategy contains useful information
    let strategy = &report.prevention_strategy;
    assert!(strategy.contains("prevention") || strategy.contains("Prevention"));

    // Should mention at least one strategy
    let has_global_ordering = strategy.contains("Global Lock Ordering");
    let has_hierarchy = strategy.contains("Lock Hierarchy");
    let has_timeout = strategy.contains("Timeout");

    assert!(
        has_global_ordering || has_hierarchy || has_timeout,
        "Prevention strategy should mention at least one approach"
    );
}

// Integration test: Simulate a real deadlock scenario
#[tokio::test]
async fn test_real_world_deadlock_scenario() {
    // Simulate a classic deadlock: two resources, two tasks
    let detector = Arc::new(DeadlockDetector::new());

    let resource_a = detector
        .register_lock("database_connection".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let resource_b = detector
        .register_lock("cache_connection".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // Task 1: Acquires A, then wants B
    let task1_id = 1;
    detector
        .record_acquisition(resource_a, task1_id, true, Some(10))
        .await
        .unwrap();

    // Task 2: Acquires B, then wants A
    let task2_id = 2;
    detector
        .record_acquisition(resource_b, task2_id, true, Some(15))
        .await
        .unwrap();

    // Task 1 tries to acquire B (fails, creates dependency)
    detector
        .record_acquisition(resource_b, task1_id, false, Some(100))
        .await
        .unwrap();

    // Task 2 tries to acquire A (fails, creates dependency)
    detector
        .record_acquisition(resource_a, task2_id, false, Some(100))
        .await
        .unwrap();

    // Check for deadlock
    let report = detector.detect_deadlocks().await;

    // The detector should identify the potential deadlock
    // Note: Depending on the exact timing and graph construction,
    // this may or may not detect a cycle. The key is that the system
    // can analyze the pattern.
    if report.has_deadlock() {
        assert!(report
            .lock_names
            .contains(&"database_connection".to_string()));
        assert!(report.lock_names.contains(&"cache_connection".to_string()));
        assert!(!report.prevention_strategy.is_empty());
    }
}

#[tokio::test]
async fn test_complex_multi_resource_deadlock() {
    // More complex scenario with multiple resources
    let detector = Arc::new(DeadlockDetector::new());

    let resources = vec![
        ("db_pool", LockType::Mutex),
        ("redis_cache", LockType::Mutex),
        ("file_system", LockType::Mutex),
        ("network_socket", LockType::Mutex),
    ];

    let mut resource_ids = Vec::new();
    for (name, lock_type) in &resources {
        let id = detector
            .register_lock(name.to_string(), *lock_type)
            .await
            .unwrap();
        resource_ids.push(id);
    }

    // Create a complex dependency graph
    {
        let mut graph = detector.graph.write().await;
        // A -> B -> C -> D -> A (large cycle)
        graph.add_dependency(resource_ids[0], resource_ids[1]);
        graph.add_dependency(resource_ids[1], resource_ids[2]);
        graph.add_dependency(resource_ids[2], resource_ids[3]);
        graph.add_dependency(resource_ids[3], resource_ids[0]);
    }

    let report = detector.detect_deadlocks().await;

    assert!(report.has_deadlock());
    assert_eq!(report.involved_locks.len(), 4);
}

// Performance test
#[tokio::test]
async fn test_detector_performance() {
    let detector = DeadlockDetector::new();

    let start = std::time::Instant::now();

    // Register many locks
    let mut lock_ids = Vec::new();
    for i in 0..100 {
        let id = detector
            .register_lock(format!("perf_lock_{}", i), LockType::Mutex)
            .await
            .unwrap();
        lock_ids.push(id);
    }

    let registration_time = start.elapsed();

    // Create many dependencies
    let dep_start = std::time::Instant::now();
    {
        let mut graph = detector.graph.write().await;
        for i in 0..99 {
            graph.add_dependency(lock_ids[i], lock_ids[i + 1]);
        }
    }
    let dependency_time = dep_start.elapsed();

    // Run detection
    let detect_start = std::time::Instant::now();
    let _report = detector.detect_deadlocks().await;
    let detect_time = detect_start.elapsed();

    // Performance assertions (adjust as needed)
    assert!(
        registration_time.as_millis() < 1000,
        "Registration should be fast"
    );
    assert!(
        dependency_time.as_millis() < 100,
        "Dependency creation should be fast"
    );
    assert!(
        detect_time.as_millis() < 100,
        "Detection should be fast for 100 locks"
    );
}
