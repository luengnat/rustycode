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
            allowed_tools_planning: vec![
                "read".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
                "edit_file".to_string(),
            ],
            allowed_tools_implementation: vec![
                "read".to_string(),
                "edit_file".to_string(),
                "write".to_string(),
                "bash".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
            ],
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

    /// Get current execution phase as a string
    pub fn current_phase(&self) -> &'static str {
        let pm = self.plan_mode.lock().unwrap();
        match pm.current_phase() {
            crate::plan_mode::ExecutionPhase::Planning => "planning",
            crate::plan_mode::ExecutionPhase::Implementation => "implementation",
        }
    }

    /// Generate a plan for the given task description
    pub async fn generate_plan(
        &self,
        task_desc: &str,
    ) -> Result<crate::plan_mode::Plan, Box<dyn std::error::Error>> {
        use chrono::Utc;
        use uuid::Uuid;

        // In a real implementation, this would call the LLM to generate a plan.
        // For now, return a mock plan for testing.
        let plan = crate::plan_mode::Plan {
            id: Uuid::new_v4().to_string(),
            summary: format!("Plan for: {}", task_desc),
            approach: "Analyze, plan, and implement changes".to_string(),
            files_to_modify: vec![crate::plan_mode::FilePlan {
                path: "src/main.rs".to_string(),
                action: crate::plan_mode::FileAction::Modify,
                reason: task_desc.to_string(),
            }],
            commands_to_run: vec![crate::plan_mode::CommandPlan {
                command: "cargo test".to_string(),
                reason: "Verify changes".to_string(),
            }],
            estimated_tokens: crate::plan_mode::TokenEstimate {
                input: 500,
                output: 1000,
            },
            estimated_cost_usd: 0.05,
            risks: vec![],
            success_criteria: vec!["Code compiles".to_string(), "Tests pass".to_string()],
            created_at: Utc::now().to_rfc3339(),
        };

        // Submit the plan to plan_mode
        let mut pm = self.plan_mode.lock().unwrap();
        pm.submit_plan(plan.clone());

        Ok(plan)
    }

    /// Approve a plan and transition to implementation phase
    pub async fn approve_plan(
        &self,
        plan: &crate::plan_mode::Plan,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut pm = self.plan_mode.lock().unwrap();
        pm.approve_plan(&plan.id)?;
        Ok(())
    }

    /// Reject a plan and stay in planning phase
    pub async fn reject_plan(
        &self,
        _plan: &crate::plan_mode::Plan,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut pm = self.plan_mode.lock().unwrap();
        pm.reject();
        Ok(())
    }

    /// Execute a task (plan generation + approval + execution)
    pub async fn execute_task(
        &self,
        task_desc: &str,
    ) -> Result<AutoTaskResult, Box<dyn std::error::Error>> {
        // Generate plan
        let plan = self.generate_plan(task_desc).await?;

        // Check if in planning phase
        let phase = {
            let pm = self.plan_mode.lock().unwrap();
            pm.current_phase()
        };

        if phase == crate::plan_mode::ExecutionPhase::Planning {
            // Return error if not in implementation phase
            return Err("Plan approval required before task execution".into());
        }

        // Execute the plan
        self.execute_plan(&plan).await
    }

    /// Execute a plan through the full pipeline
    pub async fn execute_plan(
        &self,
        plan: &crate::plan_mode::Plan,
    ) -> Result<AutoTaskResult, Box<dyn std::error::Error>> {
        let mut total_cost = 0.0;
        let mut files_modified = 0;

        // Simulate executing each file modification
        for file_plan in &plan.files_to_modify {
            tracing::info!(
                "Executing plan for file: {} (action: {})",
                file_plan.path,
                file_plan.action
            );

            // Simulate cost for this file (in real impl, would use executor)
            let file_cost = 0.02;
            total_cost += file_cost;
            files_modified += 1;
        }

        Ok(AutoTaskResult {
            success: true,
            cost: total_cost + plan.estimated_cost_usd,
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
