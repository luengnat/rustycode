use std::thread;

/// Returns true if the current thread appears to be the terminal (UI) thread.
///
/// Heuristic: the terminal UI runs on the main thread (where enable_raw_mode() and
/// event::poll/read are invoked). Tools and background tasks should not run on
/// that thread. This helper uses `thread::current().name()` and OS-specific
/// checks if needed. For now we treat the thread named "main" as the terminal
/// thread when running the TUI.
pub fn is_terminal_thread() -> bool {
    matches!(thread::current().name(), Some(name) if name == "main")
}

/// Assert that the current operation is not running on the terminal thread.
/// Panics with a helpful message when violated.
pub fn assert_not_terminal_thread(op: &str) {
    if is_terminal_thread() {
        panic!("Operation '{}' must not run on the terminal/UI thread", op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_terminal_thread_on_named_main() {
        // In test context, thread name varies by test runner.
        // This test documents the behavior: "main" thread is terminal.
        let thread_name = std::thread::current().name().map(|s| s.to_string());
        // Test threads are NOT named "main", so this should be false
        // unless running in a special context
        if thread_name.as_deref() == Some("main") {
            assert!(is_terminal_thread());
        } else {
            assert!(!is_terminal_thread());
        }
    }

    #[test]
    fn test_assert_not_terminal_thread_does_not_panic() {
        // Test threads are not named "main", so this should not panic
        assert_not_terminal_thread("test_operation");
    }

    #[test]
    fn test_is_terminal_thread_returns_bool() {
        // Just verify it returns a bool without panicking
        let _result: bool = is_terminal_thread();
    }

    // --- Named thread behavior ---

    #[test]
    fn test_named_main_thread_is_terminal() {
        let child = std::thread::Builder::new()
            .name("main".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(child.join().expect("thread panicked"));
    }

    #[test]
    fn test_named_worker_thread_is_not_terminal() {
        let child = std::thread::Builder::new()
            .name("worker".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(!child.join().expect("thread panicked"));
    }

    #[test]
    fn test_unnamed_thread_is_not_terminal() {
        let child = std::thread::spawn(is_terminal_thread);
        assert!(!child.join().expect("thread panicked"));
    }

    // --- assert_not_terminal_thread ---

    #[test]
    fn test_assert_not_terminal_worker_thread() {
        let child = std::thread::Builder::new()
            .name("tool-exec".to_string())
            .spawn(|| {
                assert_not_terminal_thread("tool_execution");
            })
            .expect("spawn failed");
        child.join().expect("thread should not panic");
    }

    #[test]
    fn test_assert_not_terminal_panics_on_main_thread() {
        let child = std::thread::Builder::new()
            .name("main".to_string())
            .spawn(|| {
                assert_not_terminal_thread("dangerous_op");
            })
            .expect("spawn failed");
        let result = child.join();
        // The thread named "main" should panic because assert_not_terminal_thread
        // detects it as the terminal thread
        assert!(result.is_err(), "should have panicked on main-named thread");
    }

    #[test]
    fn test_assert_not_terminal_various_ops() {
        // Test with different operation names — none should panic on worker threads
        for op in &[
            "file_read",
            "bash_exec",
            "llm_call",
            "git_operation",
            "mcp_tool",
        ] {
            let op_str = *op;
            let child = std::thread::spawn(move || {
                assert_not_terminal_thread(op_str);
            });
            child.join().expect("should not panic for worker thread");
        }
    }

    // --- Thread name inspection ---

    #[test]
    fn test_current_thread_name_accessible() {
        // Verify that thread::current().name() is accessible in test context
        let current = std::thread::current();
        let name = current.name();
        // Test runner threads typically have names like "test::test_name" or similar
        // Just verify we can call it without panic
        let _ = name;
    }

    #[test]
    fn test_multiple_threads_different_names() {
        let handles: Vec<_> = (0..5)
            .map(|i| {
                std::thread::Builder::new()
                    .name(format!("worker-{}", i))
                    .spawn(move || {
                        let is_term = is_terminal_thread();
                        let name = std::thread::current().name().map(|s| s.to_string());
                        (is_term, name)
                    })
                    .expect("spawn failed")
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let (is_term, name) = handle.join().expect("thread panicked");
            assert!(!is_term, "worker-{} should not be terminal thread", i);
            assert_eq!(name, Some(format!("worker-{}", i)));
        }
    }
}
