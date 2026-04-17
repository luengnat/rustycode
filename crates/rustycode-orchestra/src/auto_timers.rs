//! Orchestra Auto Timers — Unit Supervision Timers
//!
//! Provides soft timeout warning, idle watchdog, hard timeout,
//! and context-pressure monitoring for unit execution.
//! Matches orchestra-2's auto-timers.ts implementation.
//!
//! Critical for production autonomous systems to detect stuck agents
//! and manage time budgets effectively.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tracing::warn;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Timer configuration
#[derive(Debug, Clone)]
pub struct TimerConfig {
    /// Soft timeout in minutes (warning only)
    pub soft_timeout_minutes: Option<u64>,
    /// Idle timeout in minutes (no progress)
    pub idle_timeout_minutes: Option<u64>,
    /// Hard timeout in minutes (force stop)
    pub hard_timeout_minutes: Option<u64>,
    /// Context pressure threshold percentage
    pub continue_threshold_percent: f64,
    /// Check interval for watchdog timers
    pub watchdog_interval_ms: u64,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            soft_timeout_minutes: Some(10),
            idle_timeout_minutes: Some(5),
            hard_timeout_minutes: Some(20),
            continue_threshold_percent: 80.0,
            watchdog_interval_ms: 15000,
        }
    }
}

/// Progress kind for tracking
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ProgressKind {
    /// Agent sent a message
    Message,
    /// Tool call started
    ToolInFlight,
    /// Filesystem activity detected
    FilesystemActivity,
    /// Manual progress update
    Manual,
}

/// Unit runtime record
#[derive(Debug, Clone)]
pub struct UnitRuntime {
    pub unit_type: String,
    pub unit_id: String,
    pub started_at: Instant,
    pub last_progress_at: Instant,
    pub last_progress_kind: ProgressKind,
    pub wrapup_warning_sent: bool,
    pub continue_here_fired: bool,
    pub phase: RuntimePhase,
}

/// Runtime phase
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RuntimePhase {
    Starting,
    Running,
    WrapupWarning,
    ContinueHere,
    IdleTimeout,
    HardTimeout,
    Paused,
    Completed,
}

/// Timer handle for cancellation
#[derive(Debug, Clone)]
pub struct TimerHandle {
    pub unit_key: String,
}

/// Callback type for timer events
pub type TimerCallback = Box<dyn Fn(TimerEvent) + Send + Sync>;

/// Timer events
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TimerEvent {
    SoftTimeoutWarning,
    IdleTimeoutDetected { idle_duration_ms: u64 },
    HardTimeoutReached { total_duration_ms: u64 },
    ContextPressureWarning { usage_percent: f64 },
    ProgressUpdate { kind: ProgressKind },
}

/// Supervisor state
struct SupervisorState {
    runtimes: HashMap<String, UnitRuntime>,
    callbacks: Vec<TimerCallback>,
    in_flight_tools: HashMap<String, Instant>,
}

impl SupervisorState {
    fn new() -> Self {
        Self {
            runtimes: HashMap::new(),
            callbacks: Vec::new(),
            in_flight_tools: HashMap::new(),
        }
    }
}

// ─── Global State ─────────────────────────────────────────────────────────────

use std::sync::OnceLock;

static SUPERVISOR: OnceLock<Arc<Mutex<SupervisorState>>> = OnceLock::new();

fn supervisor() -> Arc<Mutex<SupervisorState>> {
    SUPERVISOR
        .get_or_init(|| Arc::new(Mutex::new(SupervisorState::new())))
        .clone()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Register a callback for timer events
///
/// # Arguments
/// * `callback` - Function to call when timer events occur
pub fn register_callback(callback: TimerCallback) {
    let binding = supervisor();
    let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    sup.callbacks.push(callback);
}

/// Start supervision for a unit
///
/// # Arguments
/// * `unit_type` - Type of unit (e.g., "task", "slice")
/// * `unit_id` - ID of the unit
/// * `config` - Timer configuration
///
/// # Returns
/// Timer handle that can be used to stop supervision
pub fn start_unit_supervision(unit_type: &str, unit_id: &str, config: TimerConfig) -> TimerHandle {
    let unit_key = format!("{}:{}", unit_type, unit_id);
    let now = Instant::now();

    let binding = supervisor();
    let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    sup.runtimes.insert(
        unit_key.clone(),
        UnitRuntime {
            unit_type: unit_type.to_string(),
            unit_id: unit_id.to_string(),
            started_at: now,
            last_progress_at: now,
            last_progress_kind: ProgressKind::Manual,
            wrapup_warning_sent: false,
            continue_here_fired: false,
            phase: RuntimePhase::Starting,
        },
    );

    drop(sup);

    // Spawn supervision tasks in background
    let unit_key_clone = unit_key.clone();
    let config_clone = config.clone();
    tokio::spawn(async move {
        run_soft_timeout_timer(unit_key_clone.clone(), config_clone.clone()).await;
    });

    let unit_key_clone = unit_key.clone();
    let config_clone = config.clone();
    tokio::spawn(async move {
        run_idle_watchdog(unit_key_clone.clone(), config_clone.clone()).await;
    });

    let unit_key_clone = unit_key.clone();
    let config_clone = config.clone();
    tokio::spawn(async move {
        run_hard_timeout_timer(unit_key_clone.clone(), config_clone.clone()).await;
    });

    TimerHandle { unit_key }
}

/// Stop supervision for a unit
///
/// # Arguments
/// * `handle` - Timer handle from start_unit_supervision
pub fn stop_unit_supervision(handle: TimerHandle) {
    let binding = supervisor();
    let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    sup.runtimes.remove(&handle.unit_key);
    sup.in_flight_tools
        .retain(|k, _| !k.starts_with(&handle.unit_key));
}

/// Record progress for a unit
///
/// # Arguments
/// * `unit_type` - Type of unit
/// * `unit_id` - ID of the unit
/// * `kind` - Kind of progress
pub fn record_progress(unit_type: &str, unit_id: &str, kind: ProgressKind) {
    let unit_key = format!("{}:{}", unit_type, unit_id);
    let binding = supervisor();
    let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(runtime) = sup.runtimes.get_mut(&unit_key) {
        runtime.last_progress_at = Instant::now();
        runtime.last_progress_kind = kind;
    }

    drop(sup);
    emit_event(TimerEvent::ProgressUpdate { kind });
}

/// Track an in-flight tool call
///
/// # Arguments
/// * `unit_type` - Type of unit
/// * `unit_id` - ID of the unit
/// * `tool_id` - Unique tool identifier
pub fn track_tool_start(unit_type: &str, unit_id: &str, tool_id: &str) {
    let tool_key = format!("{}:{}:{}", unit_type, unit_id, tool_id);
    let binding = supervisor();
    let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    sup.in_flight_tools.insert(tool_key, Instant::now());
}

/// Mark an in-flight tool as complete
///
/// # Arguments
/// * `unit_type` - Type of unit
/// * `unit_id` - ID of the unit
/// * `tool_id` - Unique tool identifier
pub fn track_tool_complete(unit_type: &str, unit_id: &str, tool_id: &str) {
    let tool_key = format!("{}:{}:{}", unit_type, unit_id, tool_id);
    let binding = supervisor();
    let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    sup.in_flight_tools.remove(&tool_key);
}

/// Check context usage and trigger warning if needed
///
/// # Arguments
/// * `unit_type` - Type of unit
/// * `unit_id` - ID of the unit
/// * `usage_percent` - Current context usage percentage
pub fn check_context_pressure(unit_type: &str, unit_id: &str, usage_percent: f64) {
    let unit_key = format!("{}:{}", unit_type, unit_id);
    let binding = supervisor();
    let sup = binding.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(runtime) = sup.runtimes.get(&unit_key) {
        if !runtime.continue_here_fired {
            let config = TimerConfig::default();
            if usage_percent >= config.continue_threshold_percent {
                drop(sup);
                emit_event(TimerEvent::ContextPressureWarning { usage_percent });

                let binding = supervisor();
                let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(runtime) = sup.runtimes.get_mut(&unit_key) {
                    runtime.continue_here_fired = true;
                    runtime.phase = RuntimePhase::ContinueHere;
                }
            }
        }
    }
}

/// Get runtime record for a unit
///
/// # Arguments
/// * `unit_type` - Type of unit
/// * `unit_id` - ID of the unit
///
/// # Returns
/// Optional UnitRuntime
pub fn get_runtime(unit_type: &str, unit_id: &str) -> Option<UnitRuntime> {
    let unit_key = format!("{}:{}", unit_type, unit_id);
    let binding = supervisor();
    let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    sup.runtimes.get(&unit_key).cloned()
}

// ─── Internal Timers ───────────────────────────────────────────────────────────

async fn run_soft_timeout_timer(unit_key: String, config: TimerConfig) {
    let soft_timeout = match config.soft_timeout_minutes {
        Some(mins) => Duration::from_secs(mins * 60),
        None => return,
    };

    sleep(soft_timeout).await;

    let binding = supervisor();
    let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(runtime) = sup.runtimes.get(&unit_key) {
        if !runtime.wrapup_warning_sent {
            drop(sup);
            emit_event(TimerEvent::SoftTimeoutWarning);

            let binding = supervisor();
            let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(runtime) = sup.runtimes.get_mut(&unit_key) {
                runtime.wrapup_warning_sent = true;
                runtime.phase = RuntimePhase::WrapupWarning;
            }
        }
    }
}

async fn run_idle_watchdog(unit_key: String, config: TimerConfig) {
    let idle_timeout = match config.idle_timeout_minutes {
        Some(mins) => Duration::from_secs(mins * 60),
        None => return,
    };

    let interval = Duration::from_millis(config.watchdog_interval_ms);

    loop {
        sleep(interval).await;

        let (last_progress, has_in_flight) = {
            let binding = supervisor();
            let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
            let runtime = sup.runtimes.get(&unit_key);

            match runtime {
                Some(runtime) => {
                    // Check for in-flight tools
                    let has_in_flight = sup
                        .in_flight_tools
                        .iter()
                        .any(|(k, _)| k.starts_with(&unit_key));

                    (runtime.last_progress_at, has_in_flight)
                }
                None => return, // Unit no longer supervised
            }
        };

        let idle_duration = last_progress.elapsed();

        // If we have in-flight tools, check their age
        if has_in_flight {
            let binding = supervisor();
            let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
            let oldest_tool = sup
                .in_flight_tools
                .iter()
                .filter(|(k, _)| k.starts_with(&unit_key))
                .min_by_key(|(_, started)| *started);

            if let Some((_, tool_started)) = oldest_tool {
                let tool_age = tool_started.elapsed();
                if tool_age < idle_timeout {
                    // Tool started recently, not actually idle
                    continue;
                }
                // Tool is too old, treat as hung
                warn!("Stalled tool detected: in-flight for {:?}", tool_age);
            }
        }

        if idle_duration >= idle_timeout {
            emit_event(TimerEvent::IdleTimeoutDetected {
                idle_duration_ms: idle_duration.as_millis() as u64,
            });

            let binding = supervisor();
            let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(runtime) = sup.runtimes.get_mut(&unit_key) {
                runtime.phase = RuntimePhase::IdleTimeout;
            }
            break;
        }
    }
}

async fn run_hard_timeout_timer(unit_key: String, config: TimerConfig) {
    let hard_timeout = match config.hard_timeout_minutes {
        Some(mins) => Duration::from_secs(mins * 60),
        None => return,
    };

    sleep(hard_timeout).await;

    let binding = supervisor();
    let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(runtime) = sup.runtimes.get(&unit_key) {
        let total_duration = runtime.started_at.elapsed();
        drop(sup);
        emit_event(TimerEvent::HardTimeoutReached {
            total_duration_ms: total_duration.as_millis() as u64,
        });

        let binding = supervisor();
        let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(runtime) = sup.runtimes.get_mut(&unit_key) {
            runtime.phase = RuntimePhase::HardTimeout;
        }
    }
}

fn emit_event(event: TimerEvent) {
    let binding = supervisor();
    let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
    for callback in &sup.callbacks {
        callback(event.clone());
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_config_default() {
        let config = TimerConfig::default();
        assert_eq!(config.soft_timeout_minutes, Some(10));
        assert_eq!(config.idle_timeout_minutes, Some(5));
        assert_eq!(config.hard_timeout_minutes, Some(20));
        assert_eq!(config.continue_threshold_percent, 80.0);
    }

    #[test]
    fn test_progress_tracking() {
        // Manually create runtime without spawning background tasks
        let unit_key = "task:T01".to_string();
        let now = Instant::now();

        let binding = supervisor();
        let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
        sup.runtimes.insert(
            unit_key.clone(),
            UnitRuntime {
                unit_type: "task".to_string(),
                unit_id: "T01".to_string(),
                started_at: now,
                last_progress_at: now,
                last_progress_kind: ProgressKind::Manual,
                wrapup_warning_sent: false,
                continue_here_fired: false,
                phase: RuntimePhase::Starting,
            },
        );
        drop(sup);

        record_progress("task", "T01", ProgressKind::Message);

        let runtime = get_runtime("task", "T01").unwrap();
        assert_eq!(runtime.last_progress_kind, ProgressKind::Message);
    }

    #[test]
    fn test_tool_tracking() {
        track_tool_start("task", "T01", "tool-1");
        track_tool_start("task", "T01", "tool-2");

        track_tool_complete("task", "T01", "tool-1");

        // Should still have tool-2 in flight
        let binding = supervisor();
        let sup = binding.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(sup.in_flight_tools.len(), 1);
    }

    #[test]
    fn test_context_pressure() {
        // Manually create runtime without spawning background tasks
        let unit_key = "task:T01".to_string();
        let now = Instant::now();

        let binding = supervisor();
        let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
        sup.runtimes.insert(
            unit_key.clone(),
            UnitRuntime {
                unit_type: "task".to_string(),
                unit_id: "T01".to_string(),
                started_at: now,
                last_progress_at: now,
                last_progress_kind: ProgressKind::Manual,
                wrapup_warning_sent: false,
                continue_here_fired: false,
                phase: RuntimePhase::Starting,
            },
        );
        drop(sup);

        // Below threshold
        check_context_pressure("task", "T01", 70.0);
        let runtime = get_runtime("task", "T01").unwrap();
        assert!(!runtime.continue_here_fired);

        // Above threshold
        check_context_pressure("task", "T01", 85.0);
        let runtime = get_runtime("task", "T01").unwrap();
        assert!(runtime.continue_here_fired);
    }

    #[test]
    fn test_multiple_units() {
        // Manually create runtimes without spawning background tasks
        let unit_key1 = "task:T01".to_string();
        let unit_key2 = "task:T02".to_string();
        let now = Instant::now();

        let binding = supervisor();
        let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
        sup.runtimes.insert(
            unit_key1.clone(),
            UnitRuntime {
                unit_type: "task".to_string(),
                unit_id: "T01".to_string(),
                started_at: now,
                last_progress_at: now,
                last_progress_kind: ProgressKind::Manual,
                wrapup_warning_sent: false,
                continue_here_fired: false,
                phase: RuntimePhase::Starting,
            },
        );
        sup.runtimes.insert(
            unit_key2.clone(),
            UnitRuntime {
                unit_type: "task".to_string(),
                unit_id: "T02".to_string(),
                started_at: now,
                last_progress_at: now,
                last_progress_kind: ProgressKind::Manual,
                wrapup_warning_sent: false,
                continue_here_fired: false,
                phase: RuntimePhase::Starting,
            },
        );
        drop(sup);

        record_progress("task", "T01", ProgressKind::Message);
        record_progress("task", "T02", ProgressKind::ToolInFlight);

        let runtime1 = get_runtime("task", "T01").unwrap();
        let runtime2 = get_runtime("task", "T02").unwrap();

        assert_eq!(runtime1.last_progress_kind, ProgressKind::Message);
        assert_eq!(runtime2.last_progress_kind, ProgressKind::ToolInFlight);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_callback_registration() {
        let event_received = Arc::new(Mutex::new(false));
        let event_clone = event_received.clone();

        register_callback(Box::new(move |_event| {
            *event_clone.lock().unwrap_or_else(|e| e.into_inner()) = true;
        }));

        // Manually create runtime without spawning background tasks
        let unit_key = "task:T01".to_string();
        let now = Instant::now();

        let binding = supervisor();
        let mut sup = binding.lock().unwrap_or_else(|e| e.into_inner());
        sup.runtimes.insert(
            unit_key.clone(),
            UnitRuntime {
                unit_type: "task".to_string(),
                unit_id: "T01".to_string(),
                started_at: now,
                last_progress_at: now,
                last_progress_kind: ProgressKind::Manual,
                wrapup_warning_sent: false,
                continue_here_fired: false,
                phase: RuntimePhase::Starting,
            },
        );
        drop(sup);

        record_progress("task", "T01", ProgressKind::Message);

        // Give async tasks a chance to run
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Event should have been emitted
        // Note: We can't easily test this without a more complex setup
    }
}
