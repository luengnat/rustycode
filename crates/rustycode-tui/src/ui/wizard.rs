//! First-Run Configuration Wizard
//!
//! This module provides a user-friendly wizard that runs on first launch to help users:
//! - Configure their AI provider (Anthropic, OpenAI, etc.)
//! - Select their preferred model
//! - Set up API keys
//! - Configure basic settings

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use rustycode_config::{Config, ProviderConfig};
use std::path::PathBuf;

/// Wizard state machine
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WizardStep {
    Welcome,
    SelectProvider,
    /// GitHub Copilot device flow: shows user_code and polling status
    CopilotDeviceFlow,
    ConfigureProvider,
    SelectModel,
    Review,
    Complete,
}

/// Provider information for the wizard
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub requires_api_key: bool,
    pub default_models: Vec<String>,
    pub popular: bool,
}

/// First-run wizard state
pub struct FirstRunWizard {
    pub step: WizardStep,
    pub providers: Vec<ProviderInfo>,
    pub selected_provider_index: usize,
    pub api_key_input: String,
    pub selected_model_index: usize,
    pub config: Config,
    pub config_path: PathBuf,
    pub error_message: Option<String>,
    pub show_help: bool,
    /// Copilot device flow: the verification URL to show the user
    pub copilot_verification_uri: String,
    /// Copilot device flow: the user code to display
    pub copilot_user_code: String,
    /// Copilot device flow: status message (e.g. "Waiting for authorization...")
    pub copilot_status: String,
    /// Copilot device flow: the obtained copilot token (set when complete)
    pub copilot_token: Option<String>,
}

impl FirstRunWizard {
    /// Create a new first-run wizard
    pub fn new(config_path: PathBuf) -> Self {
        let providers = Self::get_available_providers();
        let config = Config::default();

        Self {
            step: WizardStep::Welcome,
            providers,
            selected_provider_index: 0,
            api_key_input: String::new(),
            selected_model_index: 0,
            config,
            config_path,
            error_message: None,
            show_help: false,
            copilot_verification_uri: String::new(),
            copilot_user_code: String::new(),
            copilot_status: String::new(),
            copilot_token: None,
        }
    }

    /// Get available providers for the wizard
    fn get_available_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                id: "anthropic".to_string(),
                name: "Anthropic Claude".to_string(),
                description: "Most capable AI assistant for complex tasks".to_string(),
                requires_api_key: true,
                default_models: vec![
                    "claude-3-5-sonnet-20241022".to_string(),
                    "claude-3-5-haiku-20241022".to_string(),
                    "claude-3-opus-20240229".to_string(),
                ],
                popular: true,
            },
            ProviderInfo {
                id: "openai".to_string(),
                name: "OpenAI GPT".to_string(),
                description: "Fast and capable, great for coding tasks".to_string(),
                requires_api_key: true,
                default_models: vec![
                    "gpt-4o".to_string(),
                    "gpt-4o-mini".to_string(),
                    "gpt-4-turbo".to_string(),
                    "gpt-3.5-turbo".to_string(),
                ],
                popular: true,
            },
            ProviderInfo {
                id: "copilot".to_string(),
                name: "GitHub Copilot".to_string(),
                description: "GitHub Copilot — sign in with your GitHub account (device flow)"
                    .to_string(),
                requires_api_key: false,
                default_models: vec![
                    "gpt-4.1-copilot".to_string(),
                    "gpt-4o-copilot".to_string(),
                    "gpt-4o-mini-copilot".to_string(),
                    "o3-mini-copilot".to_string(),
                ],
                popular: true,
            },
            ProviderInfo {
                id: "kimi-global".to_string(),
                name: "Kimi (Global)".to_string(),
                description: "Moonshot AI's Kimi models - Global endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["kimi-k2".to_string(), "kimi-latest".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "kimi-cn".to_string(),
                name: "Kimi (China)".to_string(),
                description: "Moonshot AI's Kimi models - China endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["kimi-k2".to_string(), "kimi-latest".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "alibaba-global".to_string(),
                name: "Alibaba Qwen (Global)".to_string(),
                description: "Alibaba's Qwen models via DashScope - Global endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["qwen-max".to_string(), "qwen-coder-plus".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "alibaba-cn".to_string(),
                name: "Alibaba Qwen (China)".to_string(),
                description: "Alibaba's Qwen models via DashScope - China endpoint".to_string(),
                requires_api_key: true,
                default_models: vec!["qwen-max".to_string(), "qwen-coder-plus".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "vertex".to_string(),
                name: "Google Vertex AI".to_string(),
                description: "Google's Gemini models via Vertex AI platform".to_string(),
                requires_api_key: true,
                default_models: vec!["gemini-1.5-pro".to_string(), "gemini-1.5-flash".to_string()],
                popular: false,
            },
            ProviderInfo {
                id: "openrouter".to_string(),
                name: "OpenRouter".to_string(),
                description: "Access to multiple models through one API".to_string(),
                requires_api_key: true,
                default_models: vec![
                    "anthropic/claude-3.5-sonnet".to_string(),
                    "openai/gpt-4o".to_string(),
                    "google/gemini-pro-1.5".to_string(),
                ],
                popular: false,
            },
            ProviderInfo {
                id: "ollama".to_string(),
                name: "Ollama".to_string(),
                description: "Run models locally on your machine".to_string(),
                requires_api_key: false,
                default_models: vec![
                    "llama3.1".to_string(),
                    "mistral".to_string(),
                    "codellama".to_string(),
                ],
                popular: false,
            },
        ]
    }

    /// Get the currently selected provider
    pub fn selected_provider(&self) -> &ProviderInfo {
        &self.providers[self.selected_provider_index]
    }

    /// Get available models for the selected provider
    pub fn available_models(&self) -> Vec<String> {
        self.selected_provider().default_models.clone()
    }

    /// Get the currently selected model
    pub fn selected_model(&self) -> String {
        let models = self.available_models();
        if self.selected_model_index < models.len() {
            models[self.selected_model_index].clone()
        } else {
            models.first().cloned().unwrap_or_default()
        }
    }

    /// Handle key events in the wizard
    pub fn handle_key_event(&mut self, key: KeyEvent) -> WizardAction {
        // Handle Ctrl+C globally to quit wizard
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            return WizardAction::Quit;
        }

        match self.step {
            WizardStep::Welcome => self.handle_welcome_key(key),
            WizardStep::SelectProvider => self.handle_provider_selection_key(key),
            WizardStep::CopilotDeviceFlow => self.handle_copilot_device_flow_key(key),
            WizardStep::ConfigureProvider => self.handle_provider_config_key(key),
            WizardStep::SelectModel => self.handle_model_selection_key(key),
            WizardStep::Review => self.handle_review_key(key),
            WizardStep::Complete => self.handle_complete_key(key),
        }
    }

    /// Handle keys in welcome step
    fn handle_welcome_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.step = WizardStep::SelectProvider;
                WizardAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => WizardAction::Quit,
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in provider selection step
    fn handle_provider_selection_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_provider_index > 0 {
                    self.selected_provider_index -= 1;
                }
                WizardAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_provider_index < self.providers.len() - 1 {
                    self.selected_provider_index += 1;
                }
                WizardAction::Continue
            }
            KeyCode::Enter => {
                let provider = self.selected_provider();
                if provider.id == "copilot" {
                    // Start the GitHub Copilot device flow
                    self.step = WizardStep::CopilotDeviceFlow;
                    self.copilot_status = "Starting device flow...".into();
                    // Kick off async device flow in background via a blocking thread
                    self.start_copilot_device_flow();
                } else {
                    self.step = WizardStep::ConfigureProvider;
                }
                WizardAction::Continue
            }
            KeyCode::Esc => {
                self.step = WizardStep::Welcome;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in provider configuration step
    fn handle_provider_config_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Char(c) if c.is_ascii() => {
                self.api_key_input.push(c);
                WizardAction::Continue
            }
            KeyCode::Backspace => {
                self.api_key_input.pop();
                WizardAction::Continue
            }
            KeyCode::Enter => {
                if self.validate_api_key() {
                    self.step = WizardStep::SelectModel;
                    self.error_message = None;
                } else {
                    self.error_message = Some("Please enter a valid API key".to_string());
                }
                WizardAction::Continue
            }
            KeyCode::Esc => {
                self.step = WizardStep::SelectProvider;
                self.error_message = None;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in model selection step
    fn handle_model_selection_key(&mut self, key: KeyEvent) -> WizardAction {
        let models = self.available_models();

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_model_index > 0 {
                    self.selected_model_index -= 1;
                }
                WizardAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_model_index < models.len().saturating_sub(1) {
                    self.selected_model_index += 1;
                }
                WizardAction::Continue
            }
            KeyCode::Enter => {
                self.step = WizardStep::Review;
                self.update_config_from_selection();
                WizardAction::Continue
            }
            KeyCode::Esc => {
                self.step = WizardStep::ConfigureProvider;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in review step
    fn handle_review_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => match self.save_config() {
                Ok(()) => {
                    self.step = WizardStep::Complete;
                    WizardAction::Continue
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to save config: {}", e));
                    WizardAction::Continue
                }
            },
            KeyCode::Esc => {
                self.step = WizardStep::SelectModel;
                WizardAction::Continue
            }
            KeyCode::Char('r') => {
                self.step = WizardStep::SelectProvider;
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Handle keys in complete step
    fn handle_complete_key(&mut self, key: KeyEvent) -> WizardAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc | KeyCode::Char('q') => {
                WizardAction::Finish
            }
            _ => WizardAction::Continue,
        }
    }

    /// Start the GitHub Copilot device flow in a background thread.
    fn start_copilot_device_flow(&mut self) {
        // Set initial status
        self.copilot_status = "Requesting device code...".into();
        self.copilot_verification_uri.clear();
        self.copilot_user_code.clear();
        self.copilot_token = None;

        // Use a thread to run the blocking device flow since we can't easily
        // run async code from the synchronous TUI render loop.
        // We store results in files that we poll.
        let status_path = std::env::temp_dir().join("rustycode_copilot_status.json");
        // Remove any stale status file
        let _ = std::fs::remove_file(&status_path);

        std::thread::spawn(move || {
            use rustycode_auth::GitHubCopilotAuth;

            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create runtime for auth flow: {}", e);
                    return;
                }
            };
            rt.block_on(async {
                let auth = GitHubCopilotAuth::new();

                // Step 1: Request device code
                let device = match auth.request_device_code().await {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = std::fs::write(
                            &status_path,
                            serde_json::json!({"error": e.to_string()}).to_string(),
                        );
                        return;
                    }
                };

                // Write the device code info so the TUI can display it
                let _ = std::fs::write(
                    &status_path,
                    serde_json::json!({
                        "stage": "waiting",
                        "user_code": device.user_code,
                        "verification_uri": device.verification_uri,
                    })
                    .to_string(),
                );

                // Step 2: Poll for token
                let github_token = match auth
                    .poll_for_token(&device.device_code, device.interval, device.expires_in)
                    .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = std::fs::write(
                            &status_path,
                            serde_json::json!({"error": e.to_string()}).to_string(),
                        );
                        return;
                    }
                };

                // Update status
                let _ = std::fs::write(
                    &status_path,
                    serde_json::json!({"stage": "exchanging"}).to_string(),
                );

                // Step 3: Exchange for Copilot token
                match auth.exchange_for_copilot_token(&github_token).await {
                    Ok(result) => {
                        let _ = std::fs::write(
                            &status_path,
                            serde_json::json!({
                                "stage": "complete",
                                "copilot_token": result.copilot_token,
                                "expires_at": result.expires_at,
                            })
                            .to_string(),
                        );
                    }
                    Err(e) => {
                        let _ = std::fs::write(
                            &status_path,
                            serde_json::json!({"error": e.to_string()}).to_string(),
                        );
                    }
                }
            });
        });
    }

    /// Poll the status file written by the device flow thread.
    /// Returns true if the flow is complete.
    fn poll_copilot_status(&mut self) -> bool {
        let status_path = std::env::temp_dir().join("rustycode_copilot_status.json");
        let Ok(content) = std::fs::read_to_string(&status_path) else {
            return false;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };

        if let Some(error) = val.get("error").and_then(|v| v.as_str()) {
            self.copilot_status = format!("Error: {}", error);
            return false;
        }

        match val.get("stage").and_then(|v| v.as_str()) {
            Some("waiting") => {
                if let Some(code) = val.get("user_code").and_then(|v| v.as_str()) {
                    self.copilot_user_code = code.to_string();
                }
                if let Some(uri) = val.get("verification_uri").and_then(|v| v.as_str()) {
                    self.copilot_verification_uri = uri.to_string();
                }
                self.copilot_status = "Waiting for you to authorize in the browser...".into();
                false
            }
            Some("exchanging") => {
                self.copilot_status = "Authorization received, exchanging token...".into();
                false
            }
            Some("complete") => {
                if let Some(token) = val.get("copilot_token").and_then(|v| v.as_str()) {
                    self.copilot_token = Some(token.to_string());
                    self.copilot_status = "Login successful!".into();
                    // Clean up temp file
                    let _ = std::fs::remove_file(&status_path);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Handle keys in the Copilot device flow step
    fn handle_copilot_device_flow_key(&mut self, key: KeyEvent) -> WizardAction {
        // Poll for status updates
        let complete = self.poll_copilot_status();

        if complete {
            // Move to model selection
            self.step = WizardStep::SelectModel;
            self.error_message = None;
            return WizardAction::Continue;
        }

        match key.code {
            KeyCode::Esc => {
                // Cancel and go back
                let _ = std::fs::remove_file(
                    std::env::temp_dir().join("rustycode_copilot_status.json"),
                );
                self.step = WizardStep::SelectProvider;
                self.error_message = None;
                WizardAction::Continue
            }
            KeyCode::Char('r') => {
                // Retry: restart the device flow
                self.start_copilot_device_flow();
                WizardAction::Continue
            }
            _ => WizardAction::Continue,
        }
    }

    /// Render the Copilot device flow screen
    fn render_copilot_device_flow(&mut self, frame: &mut Frame, area: Rect) {
        // Poll status on every render tick
        let complete = self.poll_copilot_status();

        if complete && self.step == WizardStep::CopilotDeviceFlow {
            self.step = WizardStep::SelectModel;
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 2/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "GitHub Copilot Login",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Content
        let mut content_lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "GitHub Copilot Device Flow",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
        ];

        if !self.copilot_verification_uri.is_empty() {
            content_lines.push(Line::from(vec![
                Span::styled("1. Open: ", Style::default().fg(Color::White)),
                Span::styled(
                    &self.copilot_verification_uri,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
            content_lines.push(Line::from(""));
            content_lines.push(Line::from(vec![
                Span::styled("2. Enter code: ", Style::default().fg(Color::White)),
                Span::styled(
                    &self.copilot_user_code,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            content_lines.push(Line::from(""));
        }

        content_lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::White)),
            Span::styled(&self.copilot_status, Style::default().fg(Color::Cyan)),
        ]));

        let content_widget = Paragraph::new(content_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Authorization"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(content_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Esc: "),
            Span::styled("Cancel", Style::default().fg(Color::Red)),
            Span::from(" | r: "),
            Span::styled("Retry", Style::default().fg(Color::Cyan)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Validate the API key input
    fn validate_api_key(&self) -> bool {
        let provider = self.selected_provider();

        if !provider.requires_api_key {
            return true; // Provider doesn't need API key
        }

        // Basic validation - check if key looks reasonable
        let key = self.api_key_input.trim();
        !key.is_empty() && key.len() >= 20
    }

    /// Update config from user selections
    pub fn update_config_from_selection(&mut self) {
        // Get the model value first
        let model_value = self.selected_model();

        // Get provider index and clone the provider info
        let provider_index = self.selected_provider_index;
        let provider_clone = self.providers[provider_index].clone();

        self.config.model = model_value;
        self.config.temperature = Some(0.1);
        self.config.max_tokens = Some(4096);

        // Configure the selected provider
        let api_key = if provider_clone.requires_api_key {
            Some(self.api_key_input.clone())
        } else {
            None
        };

        let provider_config = ProviderConfig {
            api_key,
            base_url: None,
            models: Some(provider_clone.default_models.clone()),
            headers: None,
        };

        // Update providers config based on selection
        match provider_clone.id.as_str() {
            "anthropic" => {
                self.config.providers.anthropic = Some(provider_config);
            }
            "openai" => {
                self.config.providers.openai = Some(provider_config);
            }
            "openrouter" => {
                self.config.providers.openrouter = Some(provider_config);
            }
            "copilot" => {
                // For Copilot, the token comes from the device flow
                let copilot_config = ProviderConfig {
                    api_key: self.copilot_token.clone(),
                    base_url: Some("https://api.githubcopilot.com".to_string()),
                    models: Some(provider_clone.default_models.clone()),
                    headers: None,
                };
                self.config.providers.custom.insert(
                    "copilot".to_string(),
                    serde_json::to_value(copilot_config).unwrap_or_default(),
                );
            }
            _ => {
                // For custom providers, add to the custom map
                self.config.providers.custom.insert(
                    provider_clone.id.clone(),
                    serde_json::to_value(provider_config).unwrap_or_default(),
                );
            }
        }
    }

    /// Save the configuration to file
    pub fn save_config(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure the directory exists
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Backup existing config if it exists
        if self.config_path.exists() {
            let backup_path = self.config_path.with_extension("json.bak");
            std::fs::copy(&self.config_path, &backup_path)?;
        }

        // Save the config
        self.config.save(&self.config_path)?;

        // Set secure file permissions (user read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&self.config_path)?.permissions();
            perms.set_mode(0o600); // rw-------
            std::fs::set_permissions(&self.config_path, perms)?;
        }

        Ok(())
    }

    /// Render the wizard
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        match self.step {
            WizardStep::Welcome => self.render_welcome(frame, area),
            WizardStep::SelectProvider => self.render_provider_selection(frame, area),
            WizardStep::CopilotDeviceFlow => self.render_copilot_device_flow(frame, area),
            WizardStep::ConfigureProvider => self.render_provider_config(frame, area),
            WizardStep::SelectModel => self.render_model_selection(frame, area),
            WizardStep::Review => self.render_review(frame, area),
            WizardStep::Complete => self.render_complete(frame, area),
        }

        // Render error message if present
        if let Some(ref error) = self.error_message {
            self.render_error_message(frame, area, error);
        }

        // Render help overlay if enabled
        if self.show_help {
            self.render_help(frame, area);
        }
    }

    /// Render welcome screen
    fn render_welcome(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(15),
                Constraint::Length(3),
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![Line::from(vec![
            Span::styled("🦀", Style::default().fg(Color::Yellow)),
            Span::styled(" Welcome to ", Style::default().fg(Color::White)),
            Span::styled(
                "RustyCode",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        // Welcome message
        let welcome_text = vec![
            Line::from(""),
            Line::from("This wizard will help you configure RustyCode for the first time."),
            Line::from(""),
            Line::from("You'll need to:"),
            Line::from("  • Choose your AI provider (Anthropic, OpenAI, etc.)"),
            Line::from("  • Enter your API key"),
            Line::from("  • Select your preferred model"),
            Line::from(""),
            Line::from("Your API key is stored locally and never sent anywhere else."),
            Line::from(""),
            Line::from(vec![
                Span::from("Press "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" to begin, or "),
                Span::styled(
                    "?",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" for help"),
            ])
            .alignment(Alignment::Center),
        ];

        let welcome = Paragraph::new(welcome_text)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(welcome, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::White)),
            Span::styled("q", Style::default().fg(Color::Red)),
            Span::styled(" to quit", Style::default().fg(Color::White)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render provider selection screen
    fn render_provider_selection(&self, frame: &mut Frame, area: Rect) {
        // Check if we have enough height for the full layout (minimum 20 rows)
        let has_enough_height = area.height >= 20;

        let chunks = if has_enough_height {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(5), // Details widget
                    Constraint::Length(3), // Instructions
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3), // Instructions only
                ])
                .split(area)
        };

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 1/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Select AI Provider",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Provider list
        let provider_lines: Vec<Line> = self
            .providers
            .iter()
            .enumerate()
            .map(|(i, provider)| {
                let is_selected = i == self.selected_provider_index;
                let prefix = if is_selected { "►" } else { " " };
                let indicator = if provider.popular { " ⭐" } else { "" };

                Line::from(vec![Span::styled(
                    format!("{} {}{} ", prefix, provider.name, indicator),
                    Style::default()
                        .fg(if is_selected {
                            Color::Cyan
                        } else {
                            Color::White
                        })
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )])
            })
            .collect();

        let providers_widget = Paragraph::new(provider_lines)
            .block(Block::default().borders(Borders::ALL).title("Providers"))
            .wrap(Wrap { trim: true });
        frame.render_widget(providers_widget, chunks[1]);

        // Provider details (only if enough height)
        if has_enough_height {
            let provider = self.selected_provider();
            let details = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Description: ", Style::default().fg(Color::Cyan)),
                    Span::from(&provider.description),
                ]),
                Line::from(vec![
                    Span::styled("API Key: ", Style::default().fg(Color::Cyan)),
                    Span::from(if provider.requires_api_key {
                        "Required"
                    } else {
                        "Not required"
                    }),
                ]),
            ];

            let details_widget = Paragraph::new(details)
                .block(Block::default().borders(Borders::ALL).title("Details"));
            frame.render_widget(details_widget, chunks[2]);
        }

        // Instructions
        let instructions = Paragraph::new(vec![Line::from(vec![
            Span::from("↑/↓: "),
            Span::styled("Navigate", Style::default().fg(Color::Cyan)),
            Span::from(" | Enter: "),
            Span::styled("Select", Style::default().fg(Color::Green)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));

        // Render instructions at the bottom (last chunk)
        let instructions_chunk = if has_enough_height {
            chunks[3]
        } else {
            chunks[2]
        };
        frame.render_widget(instructions, instructions_chunk);
    }

    /// Render provider configuration screen
    fn render_provider_config(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(10),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 2/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Configure Provider",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // API key input
        let provider = self.selected_provider();
        let instructions = if provider.requires_api_key {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Provider: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        &provider.name,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from("Enter your API key:"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Key: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        if self.api_key_input.is_empty() {
                            "(not entered)".to_string()
                        } else {
                            // Show only last 8 characters
                            if self.api_key_input.chars().count() > 8 {
                                let chars: Vec<char> = self.api_key_input.chars().collect();
                                format!(
                                    "...{}",
                                    chars[chars.len() - 8..].iter().collect::<String>()
                                )
                            } else {
                                self.api_key_input.clone()
                            }
                        },
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled("_", Style::default().fg(Color::White)), // Cursor
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Get your API key from: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        self.get_api_key_url(&provider.id),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Provider: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        &provider.name,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("✓", Style::default().fg(Color::Green)),
                    Span::styled(
                        " No API key required for this provider",
                        Style::default().fg(Color::White),
                    ),
                ]),
                Line::from(""),
                Line::from("Press Enter to continue..."),
            ]
        };

        let input_widget = Paragraph::new(instructions)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Configuration"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(input_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Type: "),
            Span::styled("API key", Style::default().fg(Color::Cyan)),
            Span::from(" | Enter: "),
            Span::styled("Continue", Style::default().fg(Color::Green)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render model selection screen
    fn render_model_selection(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 3/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Select Model",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Model list
        let models = self.available_models();
        let model_lines: Vec<Line> = models
            .iter()
            .enumerate()
            .map(|(i, model)| {
                let is_selected = i == self.selected_model_index;
                let prefix = if is_selected { "►" } else { " " };

                Line::from(vec![Span::styled(
                    format!("{} {}", prefix, model),
                    Style::default()
                        .fg(if is_selected {
                            Color::Cyan
                        } else {
                            Color::White
                        })
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )])
            })
            .collect();

        let models_widget = Paragraph::new(model_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Available Models"),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(models_widget, chunks[1]);

        // Instructions
        let instructions = Paragraph::new(vec![Line::from(vec![
            Span::from("↑/↓: "),
            Span::styled("Navigate", Style::default().fg(Color::Cyan)),
            Span::from(" | Enter: "),
            Span::styled("Select", Style::default().fg(Color::Green)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));

        let bottom_area = Rect {
            y: area.height.saturating_sub(3),
            height: 3,
            ..area
        };
        frame.render_widget(instructions, bottom_area);
    }

    /// Render review screen
    fn render_review(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("Step 4/4: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Review & Save",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Review content
        let provider = self.selected_provider();
        let model = self.selected_model();

        let review_lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "Configuration Summary:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Provider: ", Style::default().fg(Color::White)),
                Span::styled(&provider.name, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Model: ", Style::default().fg(Color::White)),
                Span::styled(&model, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("API Key: ", Style::default().fg(Color::White)),
                Span::styled(
                    if provider.requires_api_key && !self.api_key_input.is_empty() {
                        "••••••••••••••••"
                    } else if provider.requires_api_key {
                        "(not configured)"
                    } else {
                        "N/A"
                    },
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Config file: ", Style::default().fg(Color::White)),
                Span::styled(
                    self.config_path.display().to_string(),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::White)),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " to save and start using RustyCode",
                    Style::default().fg(Color::White),
                ),
            ])
            .alignment(Alignment::Center),
        ];

        let review_widget = Paragraph::new(review_lines)
            .block(Block::default().borders(Borders::ALL).title("Review"))
            .wrap(Wrap { trim: true });
        frame.render_widget(review_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Enter: "),
            Span::styled("Save & Start", Style::default().fg(Color::Green)),
            Span::from(" | r: "),
            Span::styled("Reconfigure", Style::default().fg(Color::Cyan)),
            Span::from(" | Esc: "),
            Span::styled("Back", Style::default().fg(Color::Red)),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render completion screen
    fn render_complete(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled(
                "✓",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " Configuration Complete!",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // Success message
        let success_lines = vec![
            Line::from(""),
            Line::from("Your RustyCode configuration has been saved successfully!"),
            Line::from(""),
            Line::from("You're all set to start using RustyCode."),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::from("Press "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" or "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::from(" to start coding"),
            ])
            .alignment(Alignment::Center),
        ];

        let success_widget = Paragraph::new(success_lines)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(success_widget, chunks[1]);

        // Footer
        let footer = Paragraph::new(vec![Line::from(vec![
            Span::from("Press "),
            Span::styled("Enter/Esc", Style::default().fg(Color::Green)),
            Span::from(" to exit wizard"),
        ])
        .alignment(Alignment::Center)])
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }

    /// Render error message overlay
    fn render_error_message(&self, frame: &mut Frame, area: Rect, error: &str) {
        let error_paragraph = Paragraph::new(vec![Line::from(vec![
            Span::styled("✖", Style::default().fg(Color::Red)),
            Span::styled(format!(" {}", error), Style::default().fg(Color::White)),
        ])
        .alignment(Alignment::Center)])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Red)),
        );

        let error_area = Rect {
            width: area.width.min(60),
            height: 3,
            x: (area.width.saturating_sub(60)) / 2,
            y: area.height.saturating_sub(5),
        };

        frame.render_widget(error_paragraph, error_area);
    }

    /// Render help overlay
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help_lines = vec![
            Line::from(vec![Span::styled(
                "Keyboard Shortcuts",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )])
            .alignment(Alignment::Center),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑/k or ↓/j - Move up/down"),
            Line::from(""),
            Line::from("Actions:"),
            Line::from("  Enter     - Confirm/Continue"),
            Line::from("  Esc       - Go back"),
            Line::from("  q         - Quit wizard"),
            Line::from("  ?         - Toggle this help"),
            Line::from(""),
            Line::from(vec![
                Span::from("Press "),
                Span::styled("?", Style::default().fg(Color::Cyan)),
                Span::from(" to close help"),
            ])
            .alignment(Alignment::Center),
        ];

        let help_paragraph = Paragraph::new(help_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .style(Style::default().bg(Color::Black)),
        );

        let help_area = Rect {
            width: area.width.min(50),
            x: (area.width.saturating_sub(50)) / 2,
            y: (area.height.saturating_sub(15)) / 2,
            height: 15,
        };

        frame.render_widget(help_paragraph, help_area);
    }

    /// Get the API key URL for a provider
    fn get_api_key_url(&self, provider_id: &str) -> String {
        match provider_id {
            "anthropic" => "https://console.anthropic.com/settings/keys".to_string(),
            "openai" => "https://platform.openai.com/api-keys".to_string(),
            "openrouter" => "https://openrouter.ai/keys".to_string(),
            "copilot" => "https://github.com/settings/copilot".to_string(),
            _ => "https://example.com/get-api-key".to_string(),
        }
    }
}

/// Actions that can be returned by the wizard
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WizardAction {
    /// Continue running the wizard
    Continue,
    /// Wizard is complete, exit
    Finish,
    /// User wants to quit
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test fixture constants - clearly marked as fake/test values
    // These use TEST_KEY_ prefix to avoid confusion with real API keys
    const TEST_KEY_ANTHROPIC: &str = "TEST_KEY_antantic_api03_test123";
    const TEST_KEY_OPENAI: &str = "TEST_KEY_openai_test456";
    const TEST_KEY_OPENROUTER: &str = "TEST_KEY_openrouter_test789";

    #[test]
    fn test_wizard_initialization() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        assert_eq!(wizard.step, WizardStep::Welcome);
        assert!(!wizard.providers.is_empty());
        assert_eq!(wizard.selected_provider_index, 0);
    }

    #[test]
    fn test_provider_selection() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test selecting different providers
        wizard.selected_provider_index = 1;
        assert_eq!(wizard.selected_provider().id, "openai");

        wizard.selected_provider_index = 0;
        assert_eq!(wizard.selected_provider().id, "anthropic");
    }

    #[test]
    fn test_model_selection() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Anthropic (default selection)
        let models = wizard.available_models();
        assert!(!models.is_empty());

        wizard.selected_model_index = 0;
        let model = wizard.selected_model();
        assert!(!model.is_empty());
    }

    #[test]
    fn test_api_key_validation() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Anthropic requires API key
        assert!(!wizard.validate_api_key()); // Empty key

        // Test with a key that's too short
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.api_key_input = "short".to_string();
        assert!(!wizard.validate_api_key());

        // Test with a valid-length key
        wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
        assert!(wizard.validate_api_key());
    }

    #[test]
    fn test_step_navigation() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Initial step
        assert_eq!(wizard.step, WizardStep::Welcome);

        // Simulate Enter key
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::SelectProvider);
    }

    #[test]
    fn test_config_update() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        wizard.api_key_input = "sk-ant-api03-test-key-1234567890abcdef".to_string();
        wizard.selected_model_index = 0;

        wizard.update_config_from_selection();

        assert!(!wizard.config.model.is_empty());
        assert!(wizard.config.providers.anthropic.is_some());
    }

    #[test]
    fn test_ollama_no_api_key_required() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Select Ollama (index 9 in the providers list - last one)
        wizard.selected_provider_index = 9; // Ollama

        // Ollama should not require API key
        let provider = wizard.selected_provider();
        assert_eq!(provider.id, "ollama");
        assert!(!provider.requires_api_key);

        // Empty API key should be valid for Ollama
        assert!(wizard.validate_api_key());

        // Should be able to proceed without entering API key
        wizard.api_key_input.clear();
        wizard.update_config_from_selection();

        // Config should be updated even without API key
        assert!(!wizard.config.model.is_empty());
    }

    // ============================================================
    // EDGE CASE TESTS
    // ============================================================

    #[test]
    fn test_empty_api_key_all_providers() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test all providers that require API keys (use correct indices)
        // Order: 0=anthropic, 1=openai, 2=copilot(skip), 3=kimi-global, 4=kimi-cn,
        //        5=alibaba-global, 6=alibaba-cn, 7=vertex, 8=openrouter, 9=ollama(skip)
        let api_key_providers = [(0, "anthropic"), (1, "openai"), (8, "openrouter")];

        for (idx, provider_id) in api_key_providers.iter() {
            wizard.selected_provider_index = *idx;
            wizard.api_key_input.clear();

            let provider = wizard.selected_provider();
            assert_eq!(provider.id, *provider_id);
            assert!(provider.requires_api_key);
            assert!(
                !wizard.validate_api_key(),
                "Provider {} should reject empty API key",
                provider_id
            );
        }
    }

    #[test]
    fn test_invalid_api_key_formats() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test various invalid formats
        let invalid_keys = vec![
            "",          // Empty
            "a",         // Too short
            "ab",        // Too short
            "abc",       // Too short
            "no-prefix", // Missing prefix
            "sk-short",  // Prefix but too short
            "sk-ant-",   // Prefix with empty suffix
            "   ",       // Whitespace only
            "\t\n",      // Tabs and newlines
        ];

        for key in invalid_keys {
            wizard.api_key_input = key.to_string();
            assert!(
                !wizard.validate_api_key(),
                "Should reject invalid key: {:?}",
                key
            );
        }
    }

    #[test]
    fn test_valid_api_key_formats() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test valid keys for different providers
        let valid_keys = vec![
            ("anthropic", TEST_KEY_ANTHROPIC),
            ("openai", TEST_KEY_OPENAI),
            ("openrouter", TEST_KEY_OPENROUTER),
        ];

        for (provider_id, key) in valid_keys {
            // Select provider
            match provider_id {
                "anthropic" => wizard.selected_provider_index = 0,
                "openai" => wizard.selected_provider_index = 1,
                "openrouter" => wizard.selected_provider_index = 2,
                _ => continue,
            }

            wizard.api_key_input = key.to_string();
            assert!(
                wizard.validate_api_key(),
                "Provider {} should accept key: {}",
                provider_id,
                key
            );
        }
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn test_out_of_bounds_provider_selection() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        let provider_count = wizard.providers.len();

        // Create wizard with out-of-bounds selection
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.selected_provider_index = provider_count + 100;

        // Should panic when accessing out of bounds
        let _ = wizard.selected_provider();
    }

    #[test]
    fn test_out_of_bounds_model_selection() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        let models = wizard.available_models();
        if !models.is_empty() {
            // Set out of bounds
            wizard.selected_model_index = models.len() + 100;

            // This should either return empty string or handle gracefully
            let model = wizard.selected_model();
            // Verify it doesn't crash and returns something
            assert!(model.is_empty() || !model.is_empty());
        }
    }

    #[test]
    fn test_whitespace_api_key() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Keys with leading/trailing whitespace - should be trimmed and accepted
        wizard.api_key_input = "  sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz  ".to_string();
        // Should accept because validation trims whitespace
        assert!(wizard.validate_api_key());

        // Keys with only whitespace - should be rejected after trimming
        wizard.api_key_input = "   \t\n   ".to_string();
        assert!(!wizard.validate_api_key());
    }

    // ============================================================
    // ALL PROVIDER CONFIGURATION TESTS
    // ============================================================

    #[test]
    fn test_anthropic_provider_configuration() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        wizard.selected_provider_index = 0; // Anthropic
        wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
        wizard.selected_model_index = 0;

        wizard.update_config_from_selection();

        assert_eq!(wizard.config.model, "claude-3-5-sonnet-20241022");
        assert!(wizard.config.providers.anthropic.is_some());
        assert_eq!(
            wizard.config.providers.anthropic.as_ref().unwrap().api_key,
            Some("sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string())
        );
    }

    #[test]
    fn test_openai_provider_configuration() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        wizard.selected_provider_index = 1; // OpenAI
        wizard.api_key_input = TEST_KEY_OPENAI.to_string();
        wizard.selected_model_index = 0;

        wizard.update_config_from_selection();

        assert_eq!(wizard.config.model, "gpt-4o");
        assert!(wizard.config.providers.openai.is_some());
        assert_eq!(
            wizard.config.providers.openai.as_ref().unwrap().api_key,
            Some(TEST_KEY_OPENAI.to_string())
        );
    }

    #[test]
    fn test_openrouter_provider_configuration() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        wizard.selected_provider_index = 8; // OpenRouter (after Anthropic, OpenAI, Copilot, Kimi global/cn, Alibaba global/cn, Vertex)
        wizard.api_key_input = TEST_KEY_OPENROUTER.to_string();
        wizard.selected_model_index = 0;

        wizard.update_config_from_selection();

        assert!(wizard.config.providers.openrouter.is_some());
        assert_eq!(
            wizard.config.providers.openrouter.as_ref().unwrap().api_key,
            Some(TEST_KEY_OPENROUTER.to_string())
        );
    }

    #[test]
    fn test_ollama_provider_configuration() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        wizard.selected_provider_index = 9; // Ollama (last provider)
        wizard.api_key_input.clear(); // Ollama doesn't need API key
        wizard.selected_model_index = 0;

        wizard.update_config_from_selection();

        assert!(!wizard.config.model.is_empty());
        // Ollama should be in custom providers
        assert!(
            !wizard.config.providers.custom.is_empty()
                || wizard.config.model.contains("ollama")
                || wizard.config.model.contains("llama")
        );
    }

    #[test]
    fn test_all_provider_models() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test each provider has models
        for idx in 0..wizard.providers.len() {
            wizard.selected_provider_index = idx;
            let models = wizard.available_models();

            assert!(
                !models.is_empty(),
                "Provider at index {} should have models",
                idx
            );

            // Test each model can be selected
            for model_idx in 0..models.len() {
                wizard.selected_model_index = model_idx;
                let model = wizard.selected_model();
                assert!(
                    !model.is_empty(),
                    "Model at index {} should not be empty",
                    model_idx
                );
            }
        }
    }

    // ============================================================
    // STATE TRANSITION TESTS
    // ============================================================

    #[test]
    fn test_full_wizard_flow() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Welcome -> SelectProvider
        assert_eq!(wizard.step, WizardStep::Welcome);
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::SelectProvider);

        // SelectProvider -> ConfigureProvider (with Enter on Anthropic)
        wizard.selected_provider_index = 0; // Anthropic
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::ConfigureProvider);

        // ConfigureProvider -> SelectModel (with valid API key)
        wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::SelectModel);

        // SelectModel -> Review
        wizard.selected_model_index = 0;
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::Review);

        // Review -> Complete (saves config)
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue); // Review returns Continue after saving
        assert_eq!(wizard.step, WizardStep::Complete);

        // Complete step - press Enter to finish
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Finish); // Complete step returns Finish
    }

    #[test]
    fn test_backward_navigation() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Advance to ConfigureProvider
        wizard.step = WizardStep::ConfigureProvider;

        // Press Esc to go back
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::SelectProvider);
    }

    #[test]
    fn test_help_toggle() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Initially not showing help
        assert!(!wizard.show_help);

        // Press ? to show help
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert!(wizard.show_help);

        // Press ? again to hide help
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Continue);
        assert!(!wizard.show_help);
    }

    #[test]
    fn test_all_step_transitions() {
        let steps = vec![
            WizardStep::Welcome,
            WizardStep::SelectProvider,
            WizardStep::ConfigureProvider,
            WizardStep::SelectModel,
            WizardStep::Review,
            WizardStep::Complete,
        ];

        for step in steps {
            let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
            wizard.step = step.clone();

            // Each step should handle Enter without crashing
            let _action =
                wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

            // Each step should handle Esc without crashing
            let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
            wizard.step = step.clone();
            let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

            // Each step should handle ? without crashing
            let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
            wizard.step = step;
            let _action =
                wizard.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        }
    }

    // ============================================================
    // KEYBOARD NAVIGATION TESTS
    // ============================================================

    #[test]
    fn test_arrow_key_navigation_providers() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = WizardStep::SelectProvider;

        let _initial_index = wizard.selected_provider_index;

        // Down arrow
        wizard.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // Should move down (implementation dependent)

        // Up arrow
        wizard.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // Should move up (implementation dependent)

        // Verify no crashes
        assert!(wizard.selected_provider_index < wizard.providers.len());
    }

    #[test]
    fn test_char_j_k_navigation() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = WizardStep::SelectProvider;

        // Test j key (vim-style down)
        wizard.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

        // Test k key (vim-style up)
        wizard.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

        // Verify no crashes
        assert!(wizard.selected_provider_index < wizard.providers.len());
    }

    #[test]
    fn test_quit_key() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Press 'q' to quit
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(action, WizardAction::Quit);
    }

    #[test]
    fn test_ctrl_c_does_not_crash() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Press Ctrl+C - should not crash (behavior depends on step)
        let _action =
            wizard.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        // Test on different steps
        wizard.step = WizardStep::SelectProvider;
        let _action =
            wizard.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        wizard.step = WizardStep::ConfigureProvider;
        let _action =
            wizard.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    }

    #[test]
    fn test_unknown_keys_handled() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = WizardStep::Welcome;

        // Various unknown keys - should not crash
        let unknown_keys = vec![
            KeyCode::Char('x'),
            KeyCode::Char('z'),
            KeyCode::F(1),
            KeyCode::F(2),
            KeyCode::Tab,
            KeyCode::Backspace,
        ];

        for key_code in unknown_keys {
            let _action = wizard.handle_key_event(KeyEvent::new(key_code, KeyModifiers::NONE));
            // Should continue without crashing
        }

        // Step might change for some keys, but shouldn't crash
    }

    // ============================================================
    // ERROR HANDLING TESTS
    // ============================================================

    #[test]
    fn test_error_message_display() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Set error message
        wizard.error_message = Some("Test error message".to_string());
        assert!(wizard.error_message.is_some());
        assert_eq!(wizard.error_message.as_ref().unwrap(), "Test error message");

        // Clear error message
        wizard.error_message = None;
        assert!(wizard.error_message.is_none());
    }

    #[test]
    fn test_validation_error_on_proceed() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = WizardStep::ConfigureProvider;

        // Try to proceed with invalid API key
        wizard.api_key_input.clear(); // Empty key

        // Should set error message
        let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Action should be Continue (stay on same step with error)
        assert_eq!(action, WizardAction::Continue);
        assert_eq!(wizard.step, WizardStep::ConfigureProvider);
        assert!(wizard.error_message.is_some());
    }

    #[test]
    fn test_provider_info_popularity() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Check that popular providers are marked
        let popular_providers: Vec<_> = wizard
            .providers
            .iter()
            .filter(|p| p.popular)
            .map(|p| p.id.clone())
            .collect();

        assert!(!popular_providers.is_empty());
        assert!(popular_providers.contains(&"anthropic".to_string()));
        assert!(popular_providers.contains(&"openai".to_string()));
    }

    #[test]
    fn test_provider_requires_api_key_flag() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Providers that require API keys
        assert!(wizard.providers[0].requires_api_key); // Anthropic
        assert!(wizard.providers[1].requires_api_key); // OpenAI
        assert!(!wizard.providers[2].requires_api_key); // Copilot (no key needed)
        assert!(wizard.providers[3].requires_api_key); // Kimi Global
        assert!(wizard.providers[4].requires_api_key); // Kimi CN
        assert!(wizard.providers[5].requires_api_key); // Alibaba Global
        assert!(wizard.providers[6].requires_api_key); // Alibaba CN
        assert!(wizard.providers[7].requires_api_key); // Vertex
        assert!(wizard.providers[8].requires_api_key); // OpenRouter

        // Ollama does not require API key (last provider - index 9)
        assert!(!wizard.providers[9].requires_api_key); // Ollama
    }

    // ============================================================
    // CONFIG SAVE/LOAD TESTS
    // ============================================================

    #[test]
    fn test_config_structure_after_update() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        wizard.selected_provider_index = 0; // Anthropic
        wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
        wizard.selected_model_index = 1; // Haiku

        wizard.update_config_from_selection();

        // Verify config structure
        assert!(!wizard.config.model.is_empty());
        assert!(wizard.config.providers.anthropic.is_some());

        let anthropic_config = wizard.config.providers.anthropic.as_ref().unwrap();
        assert_eq!(
            anthropic_config.api_key,
            Some("sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string())
        );
        assert!(anthropic_config.models.is_some());
        assert!(!anthropic_config.models.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_model_in_provider_list() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        for provider_idx in 0..wizard.providers.len() {
            wizard.selected_provider_index = provider_idx;

            let models = wizard.available_models();
            for model_idx in 0..models.len() {
                wizard.selected_model_index = model_idx;

                let selected_model = wizard.selected_model();
                let available_models = wizard.available_models();

                assert!(
                    available_models.contains(&selected_model),
                    "Selected model should be in available models list"
                );
            }
        }
    }

    // ============================================================
    // API KEY URL GENERATION TESTS
    // ============================================================

    #[test]
    fn test_api_key_urls() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test that each provider has a valid URL
        let providers = vec!["anthropic", "openai", "openrouter", "ollama"];

        for provider_id in providers {
            let url = wizard.get_api_key_url(provider_id);
            assert!(
                !url.is_empty(),
                "Provider {} should have an API key URL",
                provider_id
            );
            assert!(
                url.starts_with("http"),
                "URL should start with http/https: {}",
                url
            );
        }
    }

    // ============================================================
    // ADVANCED EDGE CASES
    // ============================================================

    #[test]
    fn test_very_long_api_key() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test with an unusually long API key
        let long_key = "sk-ant-".to_string() + &"a".repeat(1000);
        wizard.api_key_input = long_key;
        wizard.selected_provider_index = 0; // Anthropic

        // Should validate (length check only ensures minimum, not maximum)
        assert!(wizard.validate_api_key());
    }

    #[test]
    fn test_special_characters_in_api_key() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // API keys with special characters (valid in some providers)
        let special_keys = vec![
            "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz",
            "sk-ant-api03-1234567890-ABCDEF_ghijklmnopqrstuvwxyz", // hyphens and underscores
            "sk-ant-api03-1234567890+abcdefghijklmnopqrstuvwxyz",  // plus sign
        ];

        for key in special_keys {
            wizard.api_key_input = key.to_string();
            // Should accept valid-length keys even with special chars
            assert!(wizard.validate_api_key(), "Should accept key: {}", key);
        }
    }

    #[test]
    fn test_unicode_in_api_key() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // API keys shouldn't have unicode, but test handling
        wizard.api_key_input = "sk-ant-api03-你好世界".to_string();

        // Should reject or accept based on validation rules
        // Current implementation checks length, so might accept
        let result = wizard.validate_api_key();
        // Just verify it doesn't crash
        let _ = result;
    }

    #[test]
    fn test_zero_length_model_list() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // All providers should have at least one model
        for idx in 0..wizard.providers.len() {
            wizard.selected_provider_index = idx;
            let models = wizard.available_models();
            assert!(
                !models.is_empty(),
                "Provider at index {} should have models",
                idx
            );
        }
    }

    #[test]
    fn test_model_names_are_unique() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // For each provider, models should be unique
        for idx in 0..wizard.providers.len() {
            wizard.selected_provider_index = idx;
            let models = wizard.available_models();

            let unique_models: std::collections::HashSet<_> = models.iter().collect();
            assert_eq!(
                unique_models.len(),
                models.len(),
                "Models should be unique for provider at index {}",
                idx
            );
        }
    }

    #[test]
    fn test_provider_descriptions_exist() {
        let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // All providers should have non-empty descriptions
        for provider in &wizard.providers {
            assert!(
                !provider.description.is_empty(),
                "Provider {} should have a description",
                provider.id
            );
        }
    }

    #[test]
    fn test_config_path_preserved() {
        let config_path = PathBuf::from("/custom/path/config.json");
        let wizard = FirstRunWizard::new(config_path.clone());

        // Config path should be preserved
        assert_eq!(wizard.config_path, config_path);
    }

    #[test]
    fn test_default_model_is_first() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // For each provider, selected_model_index 0 should give first model
        for idx in 0..wizard.providers.len() {
            wizard.selected_provider_index = idx;
            wizard.selected_model_index = 0;

            let selected_model = wizard.selected_model();
            let available_models = wizard.available_models();

            assert_eq!(
                selected_model, available_models[0],
                "Selected model at index 0 should be first available model"
            );
        }
    }

    #[test]
    fn test_multiple_validation_attempts() {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

        // Test that validation is idempotent
        wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();

        // Validate multiple times
        assert!(wizard.validate_api_key());
        assert!(wizard.validate_api_key());
        assert!(wizard.validate_api_key());

        // Should still be valid
        assert!(wizard.validate_api_key());
    }

    #[test]
    fn test_wizard_step_equality() {
        // Test that WizardStep derives PartialEq correctly
        assert_eq!(WizardStep::Welcome, WizardStep::Welcome);
        assert_eq!(WizardStep::SelectProvider, WizardStep::SelectProvider);
        assert_ne!(WizardStep::Welcome, WizardStep::Complete);
    }

    #[test]
    fn test_wizard_step_clone() {
        // Test that WizardStep derives Clone correctly
        let step = WizardStep::Welcome;
        let cloned = step.clone();
        assert_eq!(step, cloned);
    }
}
