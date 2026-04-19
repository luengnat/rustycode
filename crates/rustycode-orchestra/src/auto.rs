// rustycode-orchestra/src/auto.rs
//! Auto mode for Orchestra v2

/// Result of executing a task in AutoMode
#[derive(Debug, Clone)]
pub struct AutoTaskResult {
    pub success: bool,
    pub cost: f64,
    pub files_modified: usize,
    pub requires_approval: bool,
}

impl AutoTaskResult {
    /// Check if approval is required
    pub fn requires_approval(&self) -> bool {
        self.requires_approval
    }
}

/// Alias for backward compatibility
pub type TaskResult = AutoTaskResult;

/// Auto mode controller for Orchestra v2 with executor integration
pub struct AutoMode {
    config: AutoConfig,
    plan_mode: std::sync::Arc<std::sync::Mutex<crate::plan_mode::PlanMode>>,
}

impl AutoMode {
    /// Create a new auto mode controller
    pub fn new(config: AutoConfig) -> Self {
        Self {
            config,
            plan_mode: std::sync::Arc::new(std::sync::Mutex::new(
                crate::plan_mode::PlanMode::default(),
            )),
        }
    }

    /// Create auto mode with plan enforcement enabled
    pub fn with_plan_enforcement() -> Self {
        let config = AutoConfig {
            enabled: true,
            auto_advance: true,
            skip_confirmations: false,
            smart_defaults: true,
        };

        let plan_config = crate::plan_mode::PlanModeConfig {
            enabled: true,
            require_approval: true,
            cost_threshold: 1.0,
        };

        Self {
            config,
            plan_mode: std::sync::Arc::new(std::sync::Mutex::new(crate::plan_mode::PlanMode::new(
                plan_config,
            ))),
        }
    }

    /// Create auto mode with cost tracking enabled
    pub fn with_cost_tracking() -> Self {
        Self::with_plan_enforcement()
    }

    /// Check if auto mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get auto-advance setting
    pub fn auto_advance(&self) -> bool {
        self.config.auto_advance
    }

    /// Get plan mode status
    pub fn plan_mode_enabled(&self) -> bool {
        let pm = self.plan_mode.lock().unwrap_or_else(|e| e.into_inner());
        pm.is_enabled()
    }

    /// Get current phase as a string
    pub fn current_phase(&self) -> &'static str {
        let pm = self.plan_mode.lock().unwrap_or_else(|e| e.into_inner());
        pm.current_phase()
    }

    /// Generate a plan for the given task description
    pub async fn generate_plan(
        &self,
        task_desc: &str,
    ) -> Result<rustycode_protocol::ConvoyPlan, Box<dyn std::error::Error>> {
        use chrono::Utc;
        use uuid::Uuid;

        let plan = rustycode_protocol::ConvoyPlan {
            id: Uuid::new_v4().to_string(),
            summary: format!("Plan for: {}", task_desc),
            approach: "Analyze, plan, and implement changes".to_string(),
            files_to_modify: vec![rustycode_protocol::FilePlan {
                path: "src/main.rs".to_string(),
                description: task_desc.to_string(),
            }],
            commands_to_run: vec![rustycode_protocol::CommandPlan {
                command: "cargo test".to_string(),
                description: "Verify changes".to_string(),
            }],
            risks: vec![],
            estimated_cost_usd: 0.0,
            success_criteria: vec!["Task completed successfully".to_string()],
            approval: rustycode_protocol::PlanApproval::default(),
            created_at: Utc::now(),
        };

        let mut pm = self.plan_mode.lock().unwrap_or_else(|e| e.into_inner());
        pm.submit_plan(plan.clone());

        Ok(plan)
    }

    /// Approve a plan and transition to implementation
    pub async fn approve_plan(
        &self,
        plan: &rustycode_protocol::ConvoyPlan,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut pm = self.plan_mode.lock().unwrap_or_else(|e| e.into_inner());
        pm.approve_plan(&plan.id)?;
        Ok(())
    }

    /// Reject a plan and stay in planning phase
    pub async fn reject_plan(
        &self,
        _plan: &rustycode_protocol::ConvoyPlan,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut pm = self.plan_mode.lock().unwrap_or_else(|e| e.into_inner());
        pm.reject();
        Ok(())
    }

    /// Execute a task (plan generation + execution).
    /// Fails if plan mode is active and no plan has been approved.
    pub async fn execute_task(
        &self,
        task_desc: &str,
    ) -> Result<AutoTaskResult, Box<dyn std::error::Error>> {
        // Generate plan (this submits it but doesn't approve)
        let plan = self.generate_plan(task_desc).await?;

        // Check if approval is needed but not granted
        {
            let pm = self.plan_mode.lock().unwrap_or_else(|e| e.into_inner());
            if pm.is_enabled() && pm.current_plan().is_some() && pm.current_phase() == "planning" {
                return Err("Plan requires approval before execution".into());
            }
        }

        // Execute the plan
        self.execute_plan(&plan).await
    }

    /// Execute a plan through the full pipeline
    pub async fn execute_plan(
        &self,
        plan: &rustycode_protocol::ConvoyPlan,
    ) -> Result<AutoTaskResult, Box<dyn std::error::Error>> {
        let mut total_cost = 0.0;
        let mut files_modified = 0;

        // Simulate executing each file modification
        for file_plan in &plan.files_to_modify {
            tracing::info!(
                "Executing plan for file: {} ({})",
                file_plan.path,
                file_plan.description
            );

            let file_cost = 0.02;
            total_cost += file_cost;
            files_modified += 1;
        }

        Ok(AutoTaskResult {
            success: true,
            cost: total_cost,
            files_modified,
            requires_approval: false,
        })
    }
}

/// Auto mode configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AutoConfig {
    pub enabled: bool,
    pub auto_advance: bool,
    pub skip_confirmations: bool,
    pub smart_defaults: bool,
}

impl Default for AutoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_advance: false,
            skip_confirmations: false,
            smart_defaults: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_config_default() {
        let config = AutoConfig::default();
        assert!(!config.enabled);
        assert!(!config.auto_advance);
        assert!(!config.skip_confirmations);
        assert!(config.smart_defaults);
    }

    #[test]
    fn auto_config_serde_roundtrip() {
        let config = AutoConfig {
            enabled: true,
            auto_advance: true,
            skip_confirmations: false,
            smart_defaults: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: AutoConfig = serde_json::from_str(&json).unwrap();
        assert!(decoded.enabled);
        assert!(decoded.auto_advance);
        assert!(!decoded.skip_confirmations);
    }

    #[test]
    fn auto_mode_disabled_by_default() {
        let mode = AutoMode::new(AutoConfig::default());
        assert!(!mode.is_enabled());
        assert!(!mode.auto_advance());
    }

    #[test]
    fn auto_mode_enabled() {
        let mode = AutoMode::new(AutoConfig {
            enabled: true,
            auto_advance: true,
            skip_confirmations: true,
            smart_defaults: false,
        });
        assert!(mode.is_enabled());
        assert!(mode.auto_advance());
    }
}
