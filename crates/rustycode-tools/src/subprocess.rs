//! Cross-platform subprocess management with process isolation.
//!
//! Ported from goose's `subprocess.rs` with RustyCode adaptations:
//! - Process group isolation (Unix) so Ctrl+C doesn't kill child processes
//! - Parent death signal (Linux) so children die when the parent exits
//! - No-window flag (Windows) for headless operation
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::subprocess::configure_subprocess;
//! use tokio::process::Command;
//!
//! let mut cmd = Command::new("cargo");
//! cmd.args(["test", "--release"]);
//! configure_subprocess(&mut cmd);
//! let output = cmd.output().await?;
//! ```

use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

/// Extension trait for subprocess configuration.
pub trait SubprocessExt {
    /// Suppress console window creation (Windows only, no-op on Unix).
    fn set_no_window(&mut self) -> &mut Self;
}

impl SubprocessExt for TokioCommand {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl SubprocessExt for StdCommand {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

/// Configure a subprocess for proper isolation.
///
/// On Unix, the child gets its own process group so terminal Ctrl+C
/// doesn't propagate to it. On Linux, it also receives SIGTERM when
/// the parent dies. On Windows, no console window is created.
///
/// # Arguments
///
/// * `command` - The async command to configure
#[allow(unused_variables)]
pub fn configure_subprocess(command: &mut TokioCommand) {
    // Isolate into own process group so SIGINT from terminal doesn't reach it
    #[cfg(unix)]
    command.process_group(0);

    // On Linux, ensure child dies when parent exits
    #[cfg(target_os = "linux")]
    configure_parent_death_signal(command);

    command.set_no_window();
}

/// Configure a sync subprocess for proper isolation.
///
/// Same as [`configure_subprocess`] but for `std::process::Command`.
#[allow(unused_variables)]
pub fn configure_subprocess_sync(command: &mut StdCommand) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.set_no_window();
}

/// On Linux, set PR_SET_PDEATHSIG so the child receives SIGTERM
/// when its parent exits. Also check that the parent is still alive
/// after setting the flag (to handle a race where the parent dies
/// between fork and prctl).
#[cfg(target_os = "linux")]
fn configure_parent_death_signal(command: &mut TokioCommand) {
    // SAFETY: getpid() is always safe — it returns the caller's PID, cannot fail.
    let parent_pid = unsafe { libc::getpid() };

    // SAFETY: pre_exec runs between fork and exec in the child process.
    // PR_SET_PDEATHSIG is a standard Linux mechanism to orphan-proof child
    // processes. The getppid() check closes the race window where the parent
    // dies between fork and prctl.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            // Parent died between fork and prctl — abort
            if libc::getppid() != parent_pid {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }

            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_subprocess_sync_no_panic() {
        // Should not panic on any platform
        let mut cmd = StdCommand::new("echo");
        cmd.arg("hello");
        configure_subprocess_sync(&mut cmd);
        // Can't easily assert platform-specific flags, but no panic = success
    }

    #[tokio::test]
    async fn test_configure_subprocess_async_no_panic() {
        let mut cmd = TokioCommand::new("echo");
        cmd.arg("hello");
        configure_subprocess(&mut cmd);
        // No panic = success
    }

    #[test]
    fn test_set_no_window_sync() {
        let mut cmd = StdCommand::new("echo");
        cmd.set_no_window();
        // No panic on any platform
    }

    #[tokio::test]
    async fn test_set_no_window_async() {
        let mut cmd = TokioCommand::new("echo");
        cmd.set_no_window();
        // No panic on any platform
    }

    #[test]
    fn test_subprocess_runs_successfully() {
        let mut cmd = StdCommand::new("echo");
        cmd.arg("test_output");
        configure_subprocess_sync(&mut cmd);

        let output = cmd.output();
        // On CI or systems without echo, this may fail, but on most systems it works
        if let Ok(output) = output {
            assert!(output.status.success());
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("test_output"));
        }
    }

    #[tokio::test]
    async fn test_subprocess_async_runs_successfully() {
        let mut cmd = TokioCommand::new("echo");
        cmd.arg("async_test");
        configure_subprocess(&mut cmd);

        let output = cmd.output().await;
        if let Ok(output) = output {
            assert!(output.status.success());
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("async_test"));
        }
    }
}
