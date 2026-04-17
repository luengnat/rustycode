//! Orchestra Auto Budget — Budget Alert Level Tracking and Enforcement
//!
//! Budget alert level tracking and enforcement for auto-mode:
//! * Alert level calculation (0, 75, 80, 90, 100)
//! * Alert level change detection
//! * Budget enforcement action determination
//! * Pure functions with no module state
//!
//! Critical for cost control in autonomous development.

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Budget alert level thresholds
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum BudgetAlertLevel {
    /// No alert (under 75%)
    None = 0,

    /// Warning at 75%
    Warning75 = 75,

    /// Warning at 80%
    Warning80 = 80,

    /// Warning at 90%
    Warning90 = 90,

    /// Critical at 100%
    Critical = 100,
}

impl BudgetAlertLevel {
    /// Get the numeric threshold value
    pub fn value(&self) -> u8 {
        match self {
            BudgetAlertLevel::None => 0,
            BudgetAlertLevel::Warning75 => 75,
            BudgetAlertLevel::Warning80 => 80,
            BudgetAlertLevel::Warning90 => 90,
            BudgetAlertLevel::Critical => 100,
        }
    }

    /// Check if this is a critical alert
    pub fn is_critical(&self) -> bool {
        matches!(self, BudgetAlertLevel::Critical)
    }

    /// Check if this is any warning level
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            BudgetAlertLevel::Warning75 | BudgetAlertLevel::Warning80 | BudgetAlertLevel::Warning90
        )
    }
}

/// Budget enforcement mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum BudgetEnforcementMode {
    /// Only warn when budget exceeded
    #[serde(rename = "warn")]
    Warn,

    /// Pause execution when budget exceeded
    #[serde(rename = "pause")]
    Pause,

    /// Halt execution immediately when budget exceeded
    #[serde(rename = "halt")]
    Halt,
}

/// Budget enforcement action
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum BudgetEnforcementAction {
    /// No action needed
    None,

    /// Issue warning
    Warn,

    /// Pause execution
    Pause,

    /// Halt execution immediately
    Halt,
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Get the budget alert level for a given budget percentage
///
/// # Arguments
/// * `budget_pct` - Budget usage as a percentage (0.0 to 1.0+)
///
/// # Returns
/// Corresponding alert level
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_budget::*;
///
/// assert_eq!(get_budget_alert_level(0.5), BudgetAlertLevel::None);
/// assert_eq!(get_budget_alert_level(0.76), BudgetAlertLevel::Warning80);
/// assert_eq!(get_budget_alert_level(0.95), BudgetAlertLevel::Critical);
/// ```
pub fn get_budget_alert_level(budget_pct: f64) -> BudgetAlertLevel {
    if budget_pct >= 1.0 {
        BudgetAlertLevel::Critical
    } else if budget_pct >= 0.90 {
        BudgetAlertLevel::Warning90
    } else if budget_pct >= 0.80 {
        BudgetAlertLevel::Warning80
    } else if budget_pct >= 0.75 {
        BudgetAlertLevel::Warning75
    } else {
        BudgetAlertLevel::None
    }
}

/// Get the new budget alert level if it has increased
///
/// # Arguments
/// * `previous_level` - Previous alert level
/// * `budget_pct` - Current budget usage as a percentage
///
/// # Returns
/// New alert level if increased, None otherwise
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_budget::*;
///
/// // No change - stays at 75%
/// let result = get_new_budget_alert_level(BudgetAlertLevel::Warning75, 0.76);
/// assert!(result.is_none());
///
/// // Increase from 75 to 80
/// let result = get_new_budget_alert_level(BudgetAlertLevel::Warning75, 0.81);
/// assert_eq!(result, Some(BudgetAlertLevel::Warning80));
/// ```
pub fn get_new_budget_alert_level(
    previous_level: BudgetAlertLevel,
    budget_pct: f64,
) -> Option<BudgetAlertLevel> {
    let current_level = get_budget_alert_level(budget_pct);

    // Only return new level if it's higher than previous
    if current_level == BudgetAlertLevel::None {
        return None;
    }

    if current_level.value() > previous_level.value() {
        Some(current_level)
    } else {
        None
    }
}

/// Get the budget enforcement action for a given budget percentage
///
/// # Arguments
/// * `enforcement` - Budget enforcement mode
/// * `budget_pct` - Budget usage as a percentage
///
/// # Returns
/// Appropriate enforcement action
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_budget::*;
///
/// // Under budget
/// assert_eq!(
///     get_budget_enforcement_action(BudgetEnforcementMode::Halt, 0.5),
///     BudgetEnforcementAction::None
/// );
///
/// // Over budget with warn mode
/// assert_eq!(
///     get_budget_enforcement_action(BudgetEnforcementMode::Warn, 1.1),
///     BudgetEnforcementAction::Warn
/// );
///
/// // Over budget with pause mode
/// assert_eq!(
///     get_budget_enforcement_action(BudgetEnforcementMode::Pause, 1.1),
///     BudgetEnforcementAction::Pause
/// );
///
/// // Over budget with halt mode
/// assert_eq!(
///     get_budget_enforcement_action(BudgetEnforcementMode::Halt, 1.1),
///     BudgetEnforcementAction::Halt
/// );
/// ```
pub fn get_budget_enforcement_action(
    enforcement: BudgetEnforcementMode,
    budget_pct: f64,
) -> BudgetEnforcementAction {
    if budget_pct < 1.0 {
        BudgetEnforcementAction::None
    } else {
        match enforcement {
            BudgetEnforcementMode::Halt => BudgetEnforcementAction::Halt,
            BudgetEnforcementMode::Pause => BudgetEnforcementAction::Pause,
            BudgetEnforcementMode::Warn => BudgetEnforcementAction::Warn,
        }
    }
}

/// Format budget alert level for display
///
/// # Arguments
/// * `level` - Alert level
///
/// # Returns
/// Formatted string representation
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_budget::*;
///
/// assert_eq!(format_alert_level(BudgetAlertLevel::None), "OK");
/// assert_eq!(format_alert_level(BudgetAlertLevel::Warning75), "75% used");
/// assert_eq!(format_alert_level(BudgetAlertLevel::Critical), "100% used");
/// ```
pub fn format_alert_level(level: BudgetAlertLevel) -> String {
    match level {
        BudgetAlertLevel::None => "OK".to_string(),
        BudgetAlertLevel::Warning75 => "75% used".to_string(),
        BudgetAlertLevel::Warning80 => "80% used".to_string(),
        BudgetAlertLevel::Warning90 => "90% used".to_string(),
        BudgetAlertLevel::Critical => "100% used".to_string(),
    }
}

/// Format budget enforcement action for display
///
/// # Arguments
/// * `action` - Enforcement action
///
/// # Returns
/// Formatted string representation
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_budget::*;
///
/// assert_eq!(format_enforcement_action(BudgetEnforcementAction::None), "No action");
/// assert_eq!(format_enforcement_action(BudgetEnforcementAction::Warn), "Warning");
/// assert_eq!(format_enforcement_action(BudgetEnforcementAction::Pause), "Pause");
/// assert_eq!(format_enforcement_action(BudgetEnforcementAction::Halt), "HALT");
/// ```
pub fn format_enforcement_action(action: BudgetEnforcementAction) -> String {
    match action {
        BudgetEnforcementAction::None => "No action".to_string(),
        BudgetEnforcementAction::Warn => "Warning".to_string(),
        BudgetEnforcementAction::Pause => "Pause".to_string(),
        BudgetEnforcementAction::Halt => "HALT".to_string(),
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_budget_alert_level_none() {
        assert_eq!(get_budget_alert_level(0.0), BudgetAlertLevel::None);
        assert_eq!(get_budget_alert_level(0.5), BudgetAlertLevel::None);
        assert_eq!(get_budget_alert_level(0.74), BudgetAlertLevel::None);
    }

    #[test]
    fn test_get_budget_alert_level_75() {
        assert_eq!(get_budget_alert_level(0.75), BudgetAlertLevel::Warning75);
        assert_eq!(get_budget_alert_level(0.76), BudgetAlertLevel::Warning75);
        assert_eq!(get_budget_alert_level(0.79), BudgetAlertLevel::Warning75);
    }

    #[test]
    fn test_get_budget_alert_level_80() {
        assert_eq!(get_budget_alert_level(0.80), BudgetAlertLevel::Warning80);
        assert_eq!(get_budget_alert_level(0.85), BudgetAlertLevel::Warning80);
        assert_eq!(get_budget_alert_level(0.89), BudgetAlertLevel::Warning80);
    }

    #[test]
    fn test_get_budget_alert_level_90() {
        assert_eq!(get_budget_alert_level(0.90), BudgetAlertLevel::Warning90);
        assert_eq!(get_budget_alert_level(0.95), BudgetAlertLevel::Warning90);
        assert_eq!(get_budget_alert_level(0.99), BudgetAlertLevel::Warning90);
    }

    #[test]
    fn test_get_budget_alert_level_100() {
        assert_eq!(get_budget_alert_level(1.0), BudgetAlertLevel::Critical);
        assert_eq!(get_budget_alert_level(1.1), BudgetAlertLevel::Critical);
        assert_eq!(get_budget_alert_level(2.0), BudgetAlertLevel::Critical);
    }

    #[test]
    fn test_get_new_budget_alert_level_no_change() {
        let result = get_new_budget_alert_level(BudgetAlertLevel::None, 0.5);
        assert!(result.is_none());

        let result = get_new_budget_alert_level(BudgetAlertLevel::Warning75, 0.76);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_new_budget_alert_level_increase() {
        let result = get_new_budget_alert_level(BudgetAlertLevel::None, 0.75);
        assert_eq!(result, Some(BudgetAlertLevel::Warning75));

        let result = get_new_budget_alert_level(BudgetAlertLevel::Warning75, 0.80);
        assert_eq!(result, Some(BudgetAlertLevel::Warning80));

        let result = get_new_budget_alert_level(BudgetAlertLevel::Warning80, 0.90);
        assert_eq!(result, Some(BudgetAlertLevel::Warning90));

        let result = get_new_budget_alert_level(BudgetAlertLevel::Warning90, 1.0);
        assert_eq!(result, Some(BudgetAlertLevel::Critical));
    }

    #[test]
    fn test_get_new_budget_alert_level_decrease() {
        // Decreasing budget doesn't trigger alert
        let result = get_new_budget_alert_level(BudgetAlertLevel::Warning90, 0.80);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_budget_enforcement_action_none() {
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Warn, 0.5),
            BudgetEnforcementAction::None
        );
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Pause, 0.99),
            BudgetEnforcementAction::None
        );
    }

    #[test]
    fn test_get_budget_enforcement_action_warn() {
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Warn, 1.0),
            BudgetEnforcementAction::Warn
        );
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Warn, 1.5),
            BudgetEnforcementAction::Warn
        );
    }

    #[test]
    fn test_get_budget_enforcement_action_pause() {
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Pause, 1.0),
            BudgetEnforcementAction::Pause
        );
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Pause, 1.5),
            BudgetEnforcementAction::Pause
        );
    }

    #[test]
    fn test_get_budget_enforcement_action_halt() {
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Halt, 1.0),
            BudgetEnforcementAction::Halt
        );
        assert_eq!(
            get_budget_enforcement_action(BudgetEnforcementMode::Halt, 1.5),
            BudgetEnforcementAction::Halt
        );
    }

    #[test]
    fn test_budget_alert_level_value() {
        assert_eq!(BudgetAlertLevel::None.value(), 0);
        assert_eq!(BudgetAlertLevel::Warning75.value(), 75);
        assert_eq!(BudgetAlertLevel::Warning80.value(), 80);
        assert_eq!(BudgetAlertLevel::Warning90.value(), 90);
        assert_eq!(BudgetAlertLevel::Critical.value(), 100);
    }

    #[test]
    fn test_budget_alert_level_is_critical() {
        assert!(!BudgetAlertLevel::None.is_critical());
        assert!(!BudgetAlertLevel::Warning75.is_critical());
        assert!(!BudgetAlertLevel::Warning80.is_critical());
        assert!(!BudgetAlertLevel::Warning90.is_critical());
        assert!(BudgetAlertLevel::Critical.is_critical());
    }

    #[test]
    fn test_budget_alert_level_is_warning() {
        assert!(!BudgetAlertLevel::None.is_warning());
        assert!(BudgetAlertLevel::Warning75.is_warning());
        assert!(BudgetAlertLevel::Warning80.is_warning());
        assert!(BudgetAlertLevel::Warning90.is_warning());
        assert!(!BudgetAlertLevel::Critical.is_warning());
    }

    #[test]
    fn test_format_alert_level() {
        assert_eq!(format_alert_level(BudgetAlertLevel::None), "OK");
        assert_eq!(format_alert_level(BudgetAlertLevel::Warning75), "75% used");
        assert_eq!(format_alert_level(BudgetAlertLevel::Warning80), "80% used");
        assert_eq!(format_alert_level(BudgetAlertLevel::Warning90), "90% used");
        assert_eq!(format_alert_level(BudgetAlertLevel::Critical), "100% used");
    }

    #[test]
    fn test_format_enforcement_action() {
        assert_eq!(
            format_enforcement_action(BudgetEnforcementAction::None),
            "No action"
        );
        assert_eq!(
            format_enforcement_action(BudgetEnforcementAction::Warn),
            "Warning"
        );
        assert_eq!(
            format_enforcement_action(BudgetEnforcementAction::Pause),
            "Pause"
        );
        assert_eq!(
            format_enforcement_action(BudgetEnforcementAction::Halt),
            "HALT"
        );
    }
}
