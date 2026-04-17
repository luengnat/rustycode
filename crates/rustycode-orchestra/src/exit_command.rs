//! Exit Command — Graceful shutdown command for Orchestra
//!
//! Registers an "exit" command that stops auto-mode (if running)
//! and initiates graceful shutdown, cleaning up locks and activity state.

use crate::error::Result;
use std::sync::Arc;

/// Callback function type for stopping auto-mode
pub type StopAutoFn = Arc<dyn Fn() -> Result<()> + Send + Sync>;

/// Exit command dependencies
#[derive(Default)]
pub struct ExitDeps {
    /// Optional callback to stop auto-mode before shutdown
    pub stop_auto: Option<StopAutoFn>,
}

/// Register the exit command with the command registry
///
/// # Arguments
/// * `deps` - Optional dependencies including stop_auto callback
///
/// # Returns
/// Result indicating success or failure
///
/// # Example
/// ```
/// use rustycode_orchestra::exit_command::*;
///
/// // Register with dependencies
/// let deps = ExitDeps {
///     stop_auto: Some(Arc::new(|| {
///         println!("Stopping auto-mode...");
///         Ok(())
///     })),
/// };
/// register_exit_command(deps)?;
/// ```
pub fn register_exit_command(deps: ExitDeps) -> Result<()> {
    // The actual command registration would happen here in a real implementation
    // For now, this is a placeholder that demonstrates the structure

    // In the full implementation, this would register with a command registry
    // The handler would:
    // 1. Call stop_auto if provided
    // 2. Initiate graceful shutdown

    // Store deps for later use by the command handler
    if let Some(_stop_auto) = deps.stop_auto {
        // Would be stored in a command registry
    }

    Ok(())
}

/// Execute the exit command
///
/// # Arguments
/// * `deps` - Dependencies including stop_auto callback
///
/// # Returns
/// Result indicating success or failure
///
/// # Example
/// ```
/// use rustycode_orchestra::exit_command::*;
///
/// let deps = ExitDeps {
///     stop_auto: Some(Arc::new(|| {
///         println!("Stopping auto-mode...");
///         Ok(())
///     })),
/// };
///
/// execute_exit(deps)?;
/// ```
pub fn execute_exit(deps: ExitDeps) -> Result<()> {
    // Stop auto-mode first so locks and activity state are cleaned up
    if let Some(stop_auto) = deps.stop_auto {
        stop_auto()?;
    }

    // Initiate graceful shutdown
    // In a real implementation, this would call ctx.shutdown()
    // For now, we just return Ok to indicate success

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OrchestraV2Error;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_register_exit_command_no_deps() {
        let deps = ExitDeps::default();
        let result = register_exit_command(deps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_exit_command_with_deps() {
        let stop_auto = Arc::new(|| Ok(()));
        let deps = ExitDeps {
            stop_auto: Some(stop_auto),
        };
        let result = register_exit_command(deps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_exit_no_stop_auto() {
        let deps = ExitDeps::default();
        let result = execute_exit(deps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_exit_with_stop_auto() {
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_clone = stopped.clone();

        let stop_auto = Arc::new(move || {
            stopped_clone.store(true, Ordering::SeqCst);
            Ok(())
        });

        let deps = ExitDeps {
            stop_auto: Some(stop_auto),
        };

        let result = execute_exit(deps);
        assert!(result.is_ok());
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn test_execute_exit_stop_auto_error() {
        let stop_auto = Arc::new(|| Err(OrchestraV2Error::AutoMode("Test error".to_string())));

        let deps = ExitDeps {
            stop_auto: Some(stop_auto),
        };

        let result = execute_exit(deps);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrchestraV2Error::AutoMode(_)));
    }

    #[test]
    fn test_exit_deps_default() {
        let deps = ExitDeps::default();
        assert!(deps.stop_auto.is_none());
    }

    #[test]
    fn test_stop_auto_fn_type() {
        // This test just verifies the type compiles correctly
        fn takes_stop_auto(stop_auto: StopAutoFn) -> Result<()> {
            stop_auto()
        }

        let stop_auto = Arc::new(|| Ok(()));
        let result = takes_stop_auto(stop_auto);
        assert!(result.is_ok());
    }
}
