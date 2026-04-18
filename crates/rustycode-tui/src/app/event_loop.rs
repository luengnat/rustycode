//! Responsive Event Loop
//!
//! Coordinates all UI components via one-item-per-frame processing.
//! Guarantees <50ms input latency and 60 FPS (16ms frame budget).

use crate::agent_mode::AiMode;
use crate::agents::AgentManager;
use crate::app::event_loop_commands::{
    dispatch_registered_slash_command, CommandContext, CommandEffect,
};
use crate::app::keyboard_shortcuts::KeyboardShortcutHandler;
use crate::app::rate_limit_handler::RateLimitHandler;
use crate::app::team_mode_handler::TeamModeHandler;
use crate::app::wizard_handler::WizardHandler;
use crate::app::{service_integration::*, FRAME_BUDGET_60FPS};
use crate::compaction::{CompactionConfig, ContextMonitor};
use crate::config::load_config;
#[cfg(test)]
use crate::config::TUIConfig;
use crate::conversation_service::ConversationConfig;
use crate::help::HelpState;
use crate::memory_auto::ThreadSafeAutoMemory;
use crate::memory_injection::InjectionConfig;
use crate::providers::get_all_available_models;
use crate::session::load_command_history;
use crate::skills::{SkillLoader, SkillStateManager};
use crate::tasks::{load_tasks, WorkspaceTasks};
use crate::theme::{Theme, ThemeColors};
use crate::tool_approval::ToolApprovalManager;
use crate::ui::animator::Animator;
use crate::ui::command_palette::CommandPalette;
use crate::ui::input::{InputHandler, InputMode, InputState};
use crate::ui::message::{Message, MessageRenderer, ToolExecution};
use crate::ui::message_search::SearchState;
use crate::ui::message_tags::TagFilter;
use crate::ui::file_selector::FileSelector;
use crate::ui::model_selector::ModelSelector;
use crate::ui::session_sidebar::SessionSidebar;
use crate::ui::skill_palette::SkillPalette;

use crate::ui::theme_preview::{ThemePreview, ThemeSwitcher};
use crate::ui::toast::ToastManager;
use anyhow::{Context, Result};
use crossterm::event;
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use rustycode_core::integration::HookRegistry;
use rustycode_tools::ToolRegistry;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

// Include render implementation extracted to separate module
include!("tui_render_impl.rs");

/// Terminal cleanup guard - ensures terminal is restored even on panic
struct TerminalCleanupGuard;

impl Drop for TerminalCleanupGuard {
    fn drop(&mut self) {
        // Restore terminal state - ignore errors since we're in a panic handler
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show, // Ensure cursor is visible
        );
        // Flush to ensure all commands are executed
        let _ = std::io::stdout().flush();

        // Print session summary to terminal after leaving alternate screen
        // (Goose pattern: users see cost/duration after exiting)
        // Note: We can't access TUI state here, so the summary is printed
        // by the event_loop before this guard drops.
    }
}

/// Install panic hook to ensure terminal cleanup on panic
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
        );
        let _ = std::io::stdout().flush();
        eprintln!("\nRustyCode TUI panicked:");
        eprintln!("{}", panic_info);
        eprintln!("\nPlease report this bug at https://github.com/luengnat/rustycode/issues");
    }));
}

/// Main TUI application
///
/// Wires together all UI components:
/// - Message rendering (hierarchical display)
/// - Input handling (multi-line, clipboard, images)
/// - Markdown rendering (syntax highlighting, diffs)
/// - Status bar (progress, animations)
/// - Animation system (smooth updates)
/// - Service integration (LLM streaming, tool execution, workspace loading)
pub struct TUI {
    // UI Components (plugins)
    pub(crate) message_renderer: MessageRenderer,
    pub(crate) input_handler: InputHandler,
    pub(crate) animator: Animator,

    // Service Manager (background tasks)
    pub(crate) services: ServiceManager,

    // State
    pub(crate) messages: Vec<Message>,
    pub(crate) _input_state: InputState,
    pub(crate) input_mode: InputMode,
    pub(crate) running: bool,

    // Message list state
    pub(crate) scroll_offset_line: usize, // Line-based scroll (for actual rendering)
    pub(crate) selected_message: usize,
    pub(crate) viewport_height: usize,
    pub(crate) last_total_lines: std::cell::Cell<usize>, // Track total lines from last render pass
    pub(crate) messages_area: std::cell::Cell<ratatui::layout::Rect>, // Store messages area for click detection
    pub(crate) user_scrolled: bool, // Track if user manually scrolled up
    pub(crate) last_user_scroll_time: std::time::Instant, // Debounce: prevent auto-scroll for 2s after user scrolls

    // Streaming state
    pub(crate) current_stream_content: String,
    pub(crate) streaming_render_buffer: crate::app::streaming_render_buffer::StreamingRenderBuffer,
    pub(crate) is_streaming: bool,
    pub(crate) stream_cancelled: bool, // Set by Esc/Ctrl+C, checked by Done handler
    pub(crate) chunks_received: usize,
    pub(crate) queued_message: Option<String>, // Queued while streaming (goose pattern)
    /// Background bash command result — polled in poll_services() and displayed as system message
    pub(crate) pending_bash_result: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    pub(crate) stream_start_time: Option<std::time::Instant>, // Goose pattern: response timing
    pub(crate) last_response_duration: Option<std::time::Duration>, // Shows in status bar after completion

    // Tool execution tracking
    pub(crate) active_tools: std::collections::HashMap<String, ToolExecution>,

    // Workspace state
    pub(crate) workspace_loaded: bool,
    pub(crate) workspace_context: Option<String>, // Store workspace context for LLM
    pub(crate) workspace_tasks: WorkspaceTasks,
    pub(crate) last_extraction: Option<(Vec<crate::tasks::Task>, Vec<crate::tasks::Todo>)>,
    pub(crate) workspace_scan_progress: Option<(usize, usize)>, // (scanned, total)
    pub(crate) git_branch: Option<String>,                      // Current git branch for status bar

    // Rate limit handler
    pub(crate) rate_limit: RateLimitHandler,

    // Auto-continue mode - automatically continue working on pending tasks
    pub(crate) auto_continue_enabled: bool, // Whether auto-continue is active
    pub(crate) auto_continue_pending: bool, // Whether a continuation is pending
    pub(crate) auto_continue_iterations: usize, // Number of auto-continue iterations

    // Turn-level verification (snapshot before agent turn, diff after)
    pub(crate) turn_snapshot: Option<crate::app::turn_snapshot::TurnSnapshot>,
    // Doom loop detector — tracks repetitive tool-call patterns
    pub(crate) doom_loop: crate::app::doom_loop::DoomLoopDetector,

    // Performance: dirty flag - only render when state changes
    pub(crate) dirty: bool,

    // Token compaction
    pub(crate) context_monitor: ContextMonitor,
    pub(crate) compaction_config: CompactionConfig,
    pub(crate) showing_compaction_preview: bool,
    pub(crate) pending_compaction: bool,

    // Auto-memory system
    pub(crate) auto_memory: Option<Arc<ThreadSafeAutoMemory>>,
    pub(crate) memory_injection_config: InjectionConfig,

    // Skill palette
    pub(crate) skill_palette: SkillPalette,
    pub(crate) skill_manager: Arc<RwLock<SkillStateManager>>,

    // Round 2 Features: Help system
    pub(crate) help_state: HelpState,

    // Round 2 Features: Tool approval
    pub(crate) tool_approval: ToolApprovalManager,
    pub(crate) pending_approval_request: Option<crate::tool_approval::ApprovalRequest>,
    pub(crate) awaiting_approval: bool, // Whether we're waiting for user response

    // Session start time (for elapsed time display)
    pub(crate) start_time: std::time::Instant,

    // Theme colors for live switching
    pub(crate) theme_colors: Arc<std::sync::Mutex<ThemeColors>>,

    // Theme preview for live theme switching
    pub(crate) theme_preview: ThemePreview,

    // Quick theme switcher
    pub(crate) theme_switcher: ThemeSwitcher,

    // Toast notifications for theme change feedback
    pub(crate) toast_manager: ToastManager,

    // Error display manager for prominent error messages with suggestions
    pub(crate) error_manager: crate::ui::errors::ErrorManager,
    pub(crate) showing_error: bool,

    // Tool panel visibility (independent of sidebar)
    pub(crate) showing_tool_panel: bool,
    pub(crate) tool_panel_history: Vec<crate::ui::message::ToolExecution>, // Recent tool executions
    pub(crate) tool_panel_selected_index: Option<usize>, // Selected tool for inspection
    pub(crate) showing_tool_result: bool,                // Showing detailed tool result
    pub(crate) tool_result_show_full: bool,              // Toggle full output in tool detail
    pub(crate) tool_result_scroll_offset: usize,         // Scroll offset for tool result overlay

    // Team agent timeline panel
    pub(crate) team_panel: crate::ui::team_panel::TeamPanel,
    /// Team mode handler
    pub(crate) team_handler: TeamModeHandler,

    // Worker status panel
    pub(crate) worker_panel: crate::ui::worker_panel::WorkerPanel,

    // Clarification questions panel
    pub(crate) clarification_panel: crate::ui::clarification::ClarificationPanel,
    pub(crate) awaiting_clarification: bool, // Whether we're waiting for user answers

    // Command palette for slash commands
    pub(crate) command_palette: CommandPalette,
    pub(crate) showing_command_palette: bool,
    pub(crate) showing_skill_palette: bool,

    // Collapsible sections (Phase 3 polish)
    pub(crate) status_bar_collapsed: bool,
    pub(crate) footer_collapsed: bool,

    // Double-Esc to clear input
    pub(crate) last_esc_press: Option<std::time::Instant>,

    // Stashed prompt (Ctrl+S)
    pub(crate) stashed_prompt: Option<String>,

    // Model/Provider selector screens
    pub(crate) model_selector: ModelSelector,
    pub(crate) file_selector: FileSelector,
    pub(crate) showing_provider_selector: bool,
    pub(crate) current_model: String,

    // Session sidebar
    pub(crate) session_sidebar: SessionSidebar,

    // Session recovery (crash detection + auto-save)
    pub(crate) session_recovery:
        Option<crate::app::session_recovery_integration::SessionRecoveryManager>,

    // Message click detection (for collapse/expand)
    pub(crate) message_areas: std::cell::RefCell<Vec<(usize, Rect)>>, // (message_index, area)

    // Per-message line offsets from last render (for accurate search scroll)
    pub(crate) message_line_offsets: std::cell::RefCell<Vec<usize>>, // msg_idx -> start line

    // First-run configuration wizard handler
    pub(crate) wizard: WizardHandler,

    // Agent lifecycle management
    pub(crate) agent_manager: AgentManager,

    // TUI Configuration (mouse scroll speed, behavior settings, etc.)
    pub(crate) tui_config: crate::config::TUIConfig,

    // Keyboard shortcut handler for Vim mode and chord detection
    pub(crate) keyboard_handler: KeyboardShortcutHandler,

    // Undo stack for scroll positions (last 5 positions: (selected_message, scroll_offset_line))
    pub(crate) undo_stack: VecDeque<(usize, usize)>,

    /// File undo stack for `/undo` command — each entry is a batch of (path, old_content) pairs
    pub(crate) file_undo_stack: Vec<Vec<(String, String)>>,

    // File finder (Ctrl+O fuzzy file search)
    pub(crate) file_finder: crate::ui::file_finder::FileFinder,

    // Message search state
    pub(crate) search_state: SearchState,

    // Message tag filter state
    pub(crate) tag_filter: TagFilter,

    // Brutalist renderer mode
    pub(crate) brutalist_mode: bool,

    /// MCP server manager - shared across the session for server lifecycle management
    pub(crate) mcp_manager:
        Option<std::sync::Arc<tokio::sync::RwLock<rustycode_mcp::McpServerManager>>>,
    /// Shared todo state for LLM todo tools (todo_read, todo_write, todo_update)
    pub(crate) todo_state: rustycode_tools::TodoState,

    // Session token usage and cost tracking
    pub(crate) session_input_tokens: usize,
    pub(crate) session_output_tokens: usize,
    pub(crate) session_cost_usd: f64,
    pub(crate) cost_tracker: rustycode_llm::cost_tracker::CostTracker,

    // Hook manager for lifecycle extensibility
    pub(crate) hook_manager: rustycode_tools::hooks::HookManager,

    // Plan mode for plan-first execution gates
    pub(crate) plan_mode: rustycode_orchestra::plan_mode::PlanMode,

    // Cached API key warning (computed once, not per-frame)
    pub(crate) api_key_warning: String,
}

impl TUI {
    /// Evaluate if a task might benefit from team mode.
    /// Returns a suggestion message if team mode is recommended.
    ///
    /// Only suggests for genuinely high-risk operations to avoid noise.
    /// Common coding words like "build", "create", "service" are excluded
    /// since they appear in almost every request.
    pub fn evaluate_team_mode_suggestion(content: &str) -> Option<String> {
        let lower = content.to_lowercase();

        // Skip if already a slash command
        if content.trim().starts_with('/') {
            return None;
        }

        // Only suggest for genuinely high-risk security/production keywords
        let high_risk_keywords = [
            "authentication system",
            "authorization system",
            "password",
            "credential store",
            "api key rotation",
            "production deployment",
            "database migration",
            "payment processing",
            "encryption key",
        ];

        let has_high_risk = high_risk_keywords.iter().any(|kw| lower.contains(kw));

        if has_high_risk {
            Some(format!(
                "💡 High-risk task detected. Consider using team mode for built-in review:\n   /team {}",
                content.chars().take(50).collect::<String>()
            ))
        } else {
            None
        }
    }

    /// Create a new TUI instance with service integration
    #[allow(clippy::await_holding_lock)]
    pub fn new(cwd: PathBuf, ai_mode: AiMode, reconfigure: bool) -> Result<Self> {
        let services = ServiceManager::new(cwd.clone(), ai_mode);

        // Load TUI configuration
        let tui_config = load_config();
        let brutalist_mode = tui_config.ui.brutalist_mode; // Extract before move
        let reduced_motion = tui_config.behavior.reduced_motion; // Extract before move

        let compaction_config = CompactionConfig::default();
        let context_monitor = ContextMonitor::new(
            compaction_config.max_tokens,
            compaction_config.warning_threshold,
        );

        // Initialize auto-memory system
        let auto_memory = ThreadSafeAutoMemory::new(&cwd).ok().map(Arc::new);

        // Initialize memory injection configuration
        let memory_injection_config = InjectionConfig::default();

        // Initialize theme colors with default theme
        let theme_colors = Arc::new(std::sync::Mutex::new(ThemeColors::from(&Theme::default())));

        // Initialize theme preview and switcher
        let theme_preview = ThemePreview::new(theme_colors.clone());
        let theme_switcher = ThemeSwitcher::new(theme_colors.clone());
        let toast_manager = ToastManager::new();
        let error_manager = crate::ui::errors::ErrorManager::new();

        // Initialize command palette
        let command_palette = CommandPalette::new();

        // Load available skills
        let skill_loader = SkillLoader::new();
        let available_skills = skill_loader.load_all().unwrap_or_default();
        let skill_palette = SkillPalette::new(available_skills.clone());

        // Initialize skill state manager
        let skill_manager = Arc::new(RwLock::new(SkillStateManager::new()));
        // Load skills asynchronously in background
        let skill_manager_clone = skill_manager.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create tokio runtime for skill loading: {}", e);
                    return;
                }
            };
            rt.block_on(async {
                let load_result = {
                    #[allow(clippy::await_holding_lock)]
                    let mut manager = skill_manager_clone
                        .write()
                        .unwrap_or_else(|e| e.into_inner());
                    manager.load_skills().await
                };
                if let Err(e) = load_result {
                    tracing::error!("Failed to load skills: {}", e);
                }
            });
        });

        // Initialize hooks registry
        let _hook_registry = Arc::new(RwLock::new(HookRegistry::new()));

        // Initialize input handler and load command history
        let mut input_handler = InputHandler::new();
        let history = load_command_history();
        input_handler.set_history(history);

        Ok(Self {
            message_renderer: MessageRenderer::new(),
            input_handler,
            animator: Animator::new(4, reduced_motion),
            services,
            messages: Vec::new(),
            _input_state: InputState::new(),
            input_mode: InputMode::SingleLine,
            running: true,
            scroll_offset_line: 0,
            last_total_lines: std::cell::Cell::new(0),
            selected_message: 0,
            viewport_height: 20,
            messages_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
            user_scrolled: false,
            last_user_scroll_time: std::time::Instant::now(),
            current_stream_content: String::new(),
            streaming_render_buffer:
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new(),
            is_streaming: false,
            stream_cancelled: false,
            queued_message: None,
            pending_bash_result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            chunks_received: 0,
            stream_start_time: None,
            last_response_duration: None,
            active_tools: std::collections::HashMap::new(),
            workspace_loaded: false,
            workspace_context: None, // Initialize workspace context as None
            workspace_tasks: load_tasks(),
            last_extraction: None,
            workspace_scan_progress: None,
            git_branch: None,
            rate_limit: RateLimitHandler::new(),
            auto_continue_enabled: false,
            auto_continue_pending: false,
            auto_continue_iterations: 0,
            turn_snapshot: None,
            doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
            dirty: true,
            context_monitor,
            theme_colors,
            compaction_config,
            showing_compaction_preview: false,
            pending_compaction: false,
            auto_memory,
            memory_injection_config,
            skill_palette,
            skill_manager: Arc::new(RwLock::new(SkillStateManager::new())),
            help_state: HelpState::new(),
            tool_approval: ToolApprovalManager::new(),
            pending_approval_request: None,
            awaiting_approval: false,
            start_time: std::time::Instant::now(),
            theme_preview,
            theme_switcher,
            toast_manager,
            error_manager,
            showing_error: false,
            showing_tool_panel: false,
            tool_panel_history: Vec::new(),
            tool_panel_selected_index: None,
            showing_tool_result: false,
            tool_result_show_full: false,
            tool_result_scroll_offset: 0,
            command_palette,
            showing_command_palette: false,
            showing_skill_palette: false,
            status_bar_collapsed: false,
            footer_collapsed: false,
            last_esc_press: None,
            stashed_prompt: None,
            model_selector: ModelSelector::with_models(get_all_available_models()),
            file_selector: FileSelector::new(Vec::new()),
            showing_provider_selector: false,
            current_model: rustycode_llm::load_model_from_config().unwrap_or_default(),
            session_sidebar: SessionSidebar::new(),
            session_recovery:
                crate::app::session_recovery_integration::SessionRecoveryManager::new(
                    crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                )
                .ok(),
            message_areas: std::cell::RefCell::new(Vec::new()), // Track message areas for click detection
            message_line_offsets: std::cell::RefCell::new(Vec::new()), // Per-message line offsets
            agent_manager: AgentManager::new(),
            // First-run wizard initialization
            wizard: WizardHandler::new(&cwd, reconfigure),
            // Keyboard shortcut handler for Vim mode (gg chord detection)
            keyboard_handler: KeyboardShortcutHandler::new(tui_config.behavior.vim_enabled),
            // Undo stack for scroll positions (max 5 entries)
            undo_stack: VecDeque::with_capacity(5),
            file_undo_stack: Vec::new(),
            // Message search state
            search_state: SearchState::new(),
            // File finder (Ctrl+O)
            file_finder: crate::ui::file_finder::FileFinder::new(cwd.clone()),
            // Message tag filter state
            tag_filter: TagFilter::new(),
            // TUI configuration
            tui_config,
            // Brutalist mode from config (new distinctive look)
            brutalist_mode,
            // MCP server manager (initialized in init_services)
            mcp_manager: None,
            // Shared todo state for LLM todo tools
            todo_state: rustycode_tools::new_todo_state(),
            // Team agent timeline panel
            team_panel: crate::ui::team_panel::TeamPanel::new(),
            team_handler: TeamModeHandler::new(),
            // Worker status panel
            worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
            // Clarification questions panel
            clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
            awaiting_clarification: false,
            // Session token usage and cost tracking
            session_input_tokens: 0,
            session_output_tokens: 0,
            session_cost_usd: 0.0,
            cost_tracker: rustycode_llm::cost_tracker::CostTracker::new(None),
            hook_manager: rustycode_tools::hooks::HookManager::new(
                std::path::PathBuf::from(".rustycode/hooks"),
                rustycode_tools::hooks::HookProfile::Standard,
                String::new(),
            ),
            plan_mode: rustycode_orchestra::plan_mode::PlanMode::new(
                rustycode_orchestra::plan_mode::PlanModeConfig::default(),
            ),
            // Cached API key warning (computed once)
            api_key_warning: Self::compute_api_key_warning(),
        })
    }

    /// Compute API key warning string once at startup (not per-frame)
    fn compute_api_key_warning() -> String {
        if let Ok((provider_type, _, v2_config)) = rustycode_llm::load_provider_config_from_env() {
            let needs_api_key = !matches!(
                provider_type.to_lowercase().as_str(),
                "ollama" | "local" | "lmstudio" | ""
            );
            if needs_api_key && v2_config.api_key.is_none() {
                return format!(
                    "⚠ No API key — set {} to get started",
                    rustycode_config::api_key_env_name(&provider_type)
                );
            }
        }
        String::new()
    }

    #[cfg(test)]
    /// Create a TUI instance for testing (minimal setup)
    pub fn new_for_test() -> Self {
        use std::path::PathBuf;

        let cwd = PathBuf::from(".");
        let services = ServiceManager::new(cwd.clone(), AiMode::Ask);

        let compaction_config = CompactionConfig::default();
        let context_monitor = ContextMonitor::new(
            compaction_config.max_tokens,
            compaction_config.warning_threshold,
        );

        let auto_memory = None;
        let memory_injection_config = InjectionConfig::default();
        let theme_colors = Arc::new(std::sync::Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_preview = ThemePreview::new(theme_colors.clone());
        let theme_switcher = ThemeSwitcher::new(theme_colors.clone());
        let toast_manager = ToastManager::new();
        let error_manager = crate::ui::errors::ErrorManager::new();
        let command_palette = CommandPalette::new();

        let skill_loader = SkillLoader::new();
        let available_skills = skill_loader.load_all().unwrap_or_default();
        let skill_palette = SkillPalette::new(available_skills.clone());

        let input_handler = InputHandler::new();

        // Brutalist mode for tests (use default config value)
        let brutalist_mode = TUIConfig::default().ui.brutalist_mode;

        Self {
            message_renderer: MessageRenderer::new(),
            input_handler,
            animator: Animator::new(4, false),
            services,
            messages: Vec::new(),
            _input_state: InputState::new(),
            input_mode: InputMode::SingleLine,
            running: true,
            scroll_offset_line: 0,
            selected_message: 0,
            viewport_height: 20,
            last_total_lines: std::cell::Cell::new(0),
            messages_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
            user_scrolled: false,
            last_user_scroll_time: std::time::Instant::now(),
            current_stream_content: String::new(),
            streaming_render_buffer:
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new(),
            is_streaming: false,
            stream_cancelled: false,
            queued_message: None,
            pending_bash_result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            chunks_received: 0,
            stream_start_time: None,
            last_response_duration: None,
            active_tools: std::collections::HashMap::new(),
            workspace_loaded: false,
            workspace_context: None,
            workspace_tasks: load_tasks(),
            last_extraction: None,
            workspace_scan_progress: None,
            git_branch: None,
            rate_limit: RateLimitHandler::new(),
            auto_continue_enabled: false,
            auto_continue_pending: false,
            auto_continue_iterations: 0,
            turn_snapshot: None,
            doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
            dirty: true,
            context_monitor,
            theme_colors,
            compaction_config,
            showing_compaction_preview: false,
            pending_compaction: false,
            auto_memory,
            memory_injection_config,
            skill_palette,
            skill_manager: Arc::new(RwLock::new(SkillStateManager::new())),
            help_state: HelpState::new(),
            tool_approval: ToolApprovalManager::new(),
            pending_approval_request: None,
            awaiting_approval: false,
            start_time: std::time::Instant::now(),
            theme_preview,
            theme_switcher,
            toast_manager,
            error_manager,
            showing_error: false,
            showing_tool_panel: false,
            tool_panel_history: Vec::new(),
            tool_panel_selected_index: None,
            showing_tool_result: false,
            tool_result_show_full: false,
            tool_result_scroll_offset: 0,
            last_esc_press: None,
            stashed_prompt: None,
            command_palette,
            showing_command_palette: false,
            showing_skill_palette: false,
            status_bar_collapsed: false,
            footer_collapsed: false,
            model_selector: ModelSelector::with_models(get_all_available_models()),
            file_selector: FileSelector::new(Vec::new()),
            showing_provider_selector: false,
            current_model: rustycode_llm::load_model_from_config().unwrap_or_default(),
            session_sidebar: SessionSidebar::new(),
            session_recovery:
                crate::app::session_recovery_integration::SessionRecoveryManager::new(
                    crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                )
                .ok(),
            message_areas: std::cell::RefCell::new(Vec::new()),
            message_line_offsets: std::cell::RefCell::new(Vec::new()),
            agent_manager: AgentManager::new(),
            wizard: WizardHandler::new(&PathBuf::from("."), false),
            tui_config: TUIConfig::default(),
            keyboard_handler: KeyboardShortcutHandler::new(false),
            undo_stack: VecDeque::with_capacity(5),
            file_undo_stack: Vec::new(),
            file_finder: crate::ui::file_finder::FileFinder::new(PathBuf::from(".")),
            search_state: SearchState::new(),
            tag_filter: TagFilter::new(),
            brutalist_mode,
            mcp_manager: None,
            todo_state: rustycode_tools::new_todo_state(),
            team_panel: crate::ui::team_panel::TeamPanel::new(),
            team_handler: TeamModeHandler::new(),
            clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
            awaiting_clarification: false,
            // Worker panel (sub-agent orchestration)
            worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
            // Session token usage and cost tracking
            session_input_tokens: 0,
            session_output_tokens: 0,
            session_cost_usd: 0.0,
            cost_tracker: rustycode_llm::cost_tracker::CostTracker::new(None),
            hook_manager: rustycode_tools::hooks::HookManager::new(
                std::path::PathBuf::from(".rustycode/hooks"),
                rustycode_tools::hooks::HookProfile::Standard,
                String::new(),
            ),
            plan_mode: rustycode_orchestra::plan_mode::PlanMode::default(),
            // Cached API key warning
            api_key_warning: String::new(),
        }
    }

    /// Initialize all background services
    pub fn init_services(&mut self) -> Result<()> {
        let config = ConversationConfig::default();
        let mut tool_registry = ToolRegistry::new();

        // Register built-in tools - these are essential for AI coding assistant functionality
        self.register_builtin_tools(&mut tool_registry);

        // Load MCP tools if MCP servers are configured
        self.load_mcp_tools(&mut tool_registry);

        // Count tools before moving registry
        let tool_count = tool_registry.list().len();

        self.services.start_conversation(config, tool_registry)?;
        self.services.start_workspace_loading()?;

        // Wire shared todo state into service manager so LLM can use todo tools
        self.services.set_todo_state(self.todo_state.clone());

        tracing::info!("Services initialized with {} tools", tool_count);

        Ok(())
    }

    /// Resume the most recent session from disk.
    ///
    /// Called when `--resume` flag is passed on the CLI. Finds the most
    /// recently saved session and loads its messages/scroll state.
    pub fn resume_most_recent_session(&mut self) {
        if let Some(ref recovery) = self.session_recovery {
            match recovery.list_recoverable_sessions() {
                Ok(sessions) => {
                    if sessions.is_empty() {
                        self.add_system_message("No previous sessions found to resume".to_string());
                        return;
                    }

                    // Try sessions in order, load the first one that works
                    for session_id in &sessions {
                        if let Ok(state) = recovery.load_state(session_id) {
                            let msg_count = state.messages.len();
                            if msg_count == 0 {
                                continue;
                            }

                            let age = chrono::Utc::now()
                                .signed_duration_since(state.last_saved)
                                .num_minutes();

                            // Reset session state for clean load
                            self.selected_message = 0;
                            self.scroll_offset_line = state.scroll_position;
                            self.user_scrolled = false;
                            self.active_tools.clear();
                            self.tool_panel_history.clear();
                            self.tool_panel_selected_index = None;
                            self.showing_tool_result = false;
                            // Reset streaming state (session could have been saved mid-stream)
                            self.is_streaming = false;
                            self.stream_cancelled = false;
                            self.current_stream_content.clear();
                            self.streaming_render_buffer =
                                crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
                            self.chunks_received = 0;
                            self.queued_message = None;

                            // Restore messages
                            self.messages = state.messages;
                            // Recompute token context based on restored messages so the
                            // context usage bar reflects the loaded session.
                            self.context_monitor.update(&self.messages);
                            if !self.messages.is_empty() {
                                self.selected_message = self.messages.len().saturating_sub(1);
                            }

                            self.add_system_message(format!(
                                "Resumed session '{}' ({} messages, {} min ago)",
                                session_id.split('-').next().unwrap_or(session_id),
                                msg_count,
                                age
                            ));
                            self.dirty = true;
                            tracing::info!(
                                "Resumed session {} ({} messages)",
                                session_id,
                                msg_count
                            );
                            return;
                        }
                    }

                    self.add_system_message("Could not load any saved sessions".to_string());
                }
                Err(e) => {
                    tracing::warn!("Failed to list sessions for resume: {}", e);
                    self.add_system_message("Could not find saved sessions".to_string());
                }
            }
        } else {
            self.add_system_message("Session persistence not available".to_string());
        }
    }

    /// Register all built-in tools for AI coding assistant functionality
    fn register_builtin_tools(&self, tool_registry: &mut ToolRegistry) {
        use crate::skills::as_tool::{
            CreateCronTool, CreateTeamTool, SkillToolRegistry, SpawnAgentTool,
        };
        use rustycode_tools::{
            BashTool, EditFile, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool, GlobTool,
            GrepTool, ListDirTool, QuestionTool, ReadFileTool, SearchReplace, WriteFileTool,
        };

        // Core file system tools
        tool_registry.register(ReadFileTool);
        tool_registry.register(WriteFileTool);
        tool_registry.register(ListDirTool);
        tool_registry.register(EditFile);

        // Search tools
        tool_registry.register(GrepTool);
        tool_registry.register(GlobTool);
        tool_registry.register(SearchReplace);

        // Command execution
        tool_registry.register(BashTool);

        // Git tools
        tool_registry.register(GitStatusTool);
        tool_registry.register(GitDiffTool);
        tool_registry.register(GitLogTool);
        tool_registry.register(GitCommitTool);

        // Interactive tools
        tool_registry.register(QuestionTool);

        // Agent spawning tool - allows LLM to delegate to specialized agents
        tool_registry.register(SpawnAgentTool::new());

        // Team management tool - allows LLM to create agent teams
        tool_registry.register(CreateTeamTool::new());

        // Cron scheduling tool - allows LLM to create scheduled tasks
        tool_registry.register(CreateCronTool::new());

        // Register skill-as-tool wrappers for active skills
        let skill_tool_registry = SkillToolRegistry::new(self.skill_manager.clone());
        let skill_tools = skill_tool_registry.build_tools();
        for skill_tool in skill_tools {
            tool_registry.register_boxed(skill_tool);
        }

        tracing::info!("Registered {} built-in tools", tool_registry.list().len());
    }

    /// Load tools from configured MCP servers
    fn load_mcp_tools(&mut self, tool_registry: &mut ToolRegistry) {
        use rustycode_mcp::manager::{ManagerConfig, McpConfigFile};
        use rustycode_mcp::proxy::{ProxyConfig, ToolProxy};
        use std::sync::Arc;

        // Load MCP config from all standard locations
        let configs = McpConfigFile::load_from_standard_locations();

        if configs.is_empty() {
            tracing::debug!("No MCP server configs found in standard locations");
            return;
        }

        // Create a shared MCP server manager
        let manager = rustycode_mcp::McpServerManager::new(ManagerConfig::default());
        let manager_arc = Arc::new(tokio::sync::RwLock::new(manager));

        // Store the manager for later use
        self.mcp_manager = Some(manager_arc.clone());

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create tokio runtime for MCP: {}", e);
                return;
            }
        };

        // Load and start servers from all config files
        let mut started_count = 0;
        let mut tools_registered = 0;
        for (config_path, config_file) in configs {
            tracing::info!("Loading MCP servers from {:?}", config_path);

            for (server_id, server_config) in config_file.servers {
                tracing::info!("Starting MCP server '{}'", server_id);

                // Create a tool proxy for this server (stdio only)
                let command = match server_config.command.clone() {
                    Some(cmd) => cmd,
                    None => {
                        tracing::debug!(
                            "Skipping MCP server '{}': no command (remote transport)",
                            server_id
                        );
                        continue;
                    }
                };
                let proxy_config = ProxyConfig {
                    server_name: server_id.clone(),
                    command,
                    args: server_config.args.clone(),
                    tool_prefix: None,
                    cache_tools: true,
                };

                match rt.block_on(ToolProxy::with_discovery(proxy_config)) {
                    Ok(proxy) => {
                        tracing::info!("MCP server '{}' connected successfully", server_id);
                        started_count += 1;

                        // Get all tools from the proxy and register them
                        let proxied_tools = rt.block_on(proxy.get_tools());
                        for proxied_tool in proxied_tools {
                            let tool_name = proxied_tool.name.clone();
                            tool_registry.register(proxied_tool);
                            tracing::debug!("  Registered MCP tool: {}", tool_name);
                            tools_registered += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to connect to MCP server '{}': {}", server_id, e);
                    }
                }
            }
        }

        if started_count > 0 {
            tracing::info!(
                "Started {} MCP server(s) with {} tools registered",
                started_count,
                tools_registered
            );
        }
    }

    /// Check for tmux compatibility and add warning messages if needed
    pub(crate) fn check_tmux_compatibility(&mut self) {
        if std::env::var("TMUX").is_err() {
            return;
        }

        use std::process::Command;

        // Check escape-time
        let escape_time = Command::new("tmux")
            .args(["show-options", "-gv", "escape-time"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());

        if let Some(et) = escape_time {
            if et > 50 {
                self.add_system_message(format!(
                    "⚠️ High tmux escape-time detected ({}ms). ESC key may feel sluggish. Recommend: set -sg escape-time 0",
                    et
                ));
            }
        }

        // Check mouse support
        let mouse = Command::new("tmux")
            .args(["show-options", "-gv", "mouse"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "on");

        if let Some(false) = mouse {
            self.add_system_message(
                "⚠️ Tmux mouse support is off. Scrolling may not work. Recommend: set -g mouse on"
                    .to_string(),
            );
        }

        // Check focus-events
        let focus_events = Command::new("tmux")
            .args(["show-options", "-gv", "focus-events"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "on");

        if let Some(false) = focus_events {
            self.add_system_message(
                "⚠️ Tmux focus-events is off. TUI may not detect when you switch windows. Recommend: set -g focus-events on"
                    .to_string(),
            );
        }

        // Check for Ctrl+B clash
        self.add_system_message(
            "💡 Inside tmux: Use Ctrl+L as an alternative to Ctrl+B for toggling the sidebar."
                .to_string(),
        );

        self.dirty = true;
    }

    /// Run the TUI main loop
    pub fn run(&mut self) -> Result<()> {
        // Install panic hook FIRST - before any terminal operations
        install_panic_hook();

        tracing::info!("TUI run() — setting up terminal");

        // Setup terminal with automatic cleanup guard
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|e| {
            tracing::error!("Failed to create terminal backend: {}", e);
            e
        })?;

        tracing::info!("TUI run() — entering alternate screen");

        // Clear screen and setup terminal
        //
        // Mouse capture ENABLED for scroll wheel support while preserving native text selection.
        // We only handle ScrollUp/ScrollDown events and ignore all other mouse events,
        // which allows the terminal to handle text selection natively.
        //
        // For text selection: Works natively (just click and drag)
        // For scroll: Mouse wheel/trackpad works via captured events
        //
        // Enter alternate screen first, then clear
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableMouseCapture,
        )
        .map_err(|e| {
            tracing::error!("Failed to enter alternate screen: {}", e);
            e
        })?;
        terminal.clear().map_err(|e| {
            tracing::error!("Failed to clear terminal: {}", e);
            e
        })?;
        crossterm::terminal::enable_raw_mode().map_err(|e| {
            tracing::error!("Failed to enable raw mode: {}", e);
            e
        })?;

        // Set terminal title to project name (Goose pattern for tab identification)
        if let Some(dir_name) = self.services.cwd().file_name().and_then(|n| n.to_str()) {
            // Sanitize: strip control characters to prevent terminal escape injection
            let sanitized: String = dir_name.chars().filter(|c| !c.is_control()).collect();
            // OSC 0 sets the terminal window/tab title
            print!("\x1b]0;rustycode: {}\x07", sanitized);
            let _ = std::io::stdout().flush();
        }

        // Create cleanup guard that runs on drop (even on panic)
        let _cleanup_guard = TerminalCleanupGuard;

        // Setup signal handlers for graceful shutdown
        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
        let shutdown_tx_clone = shutdown_tx.clone();

        tracing::info!("TUI run() — setting up signal handler");

        ctrlc::set_handler(move || {
            let _ = shutdown_tx_clone.send(());
        })
        .map_err(|e| {
            tracing::error!("Failed to set Ctrl+C handler: {}", e);
            e
        })?;

        tracing::info!("TUI run() — entering event loop");

        // Cleanup happens automatically when _cleanup_guard goes out of scope

        // Initialize session recovery (create lock file, check for crash recovery)
        if let Some(ref recovery) = self.session_recovery {
            if let Err(e) = recovery.init_session() {
                tracing::warn!("Session recovery init failed: {}", e);
            }

            // Check for crash recovery — but only notify, don't auto-restore.
            // Auto-restoring old sessions caused API errors (stale tool results)
            // and confused users who expected a fresh start. Use /resume or --resume
            // to explicitly load a previous session.
            if let Ok(Some(state)) = recovery.check_crash_recovery() {
                let msg_count = state.messages.len();
                let age = chrono::Utc::now()
                    .signed_duration_since(state.last_saved)
                    .num_minutes();

                self.add_system_message(format!(
                    "Found recoverable session ({} messages, {} min ago). Use /resume to load it.",
                    msg_count, age
                ));
                self.dirty = true;
            }
        }

        // Check for tmux compatibility issues
        self.check_tmux_compatibility();

        self.event_loop(&mut terminal, shutdown_rx)?;

        tracing::info!("Event loop exited normally");

        // Print session summary to terminal (after event loop exits, before cleanup guard drops)
        self.print_session_summary();

        Ok(())
    }

    /// Main event loop with one-item-per-frame processing
    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        shutdown_rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut last_frame_time = Instant::now();
        let mut frame_count: u32 = 0;

        while self.running {
            // Check for shutdown signal (Ctrl+C)
            if shutdown_rx.try_recv().is_ok() {
                // User requested shutdown
                self.running = false;
                break;
            }

            let frame_start = Instant::now();

            // Calculate delta time for animations (in milliseconds)
            let delta_ms = last_frame_time.elapsed().as_millis() as u64;
            last_frame_time = frame_start;

            // Phase 1: Update animations (only marks dirty when frame actually advances)
            if self.animator.update() {
                // Only mark dirty if an animation is visible (streaming or active tools)
                if self.is_streaming || !self.active_tools.is_empty() {
                    self.dirty = true;
                }
            }

            // Update session sidebar info
            self.session_sidebar
                .update_session_info(self.messages.len(), self.active_tools.len());
            self.session_sidebar
                .set_rate_limited(self.rate_limit.until.is_some());

            // Update toast animations
            let has_active_toasts = self.toast_manager.tick(delta_ms);
            if has_active_toasts {
                self.dirty = true; // Mark dirty for animation updates
            }

            // Error auto-dismiss: If error_manager is showing, mark dirty so
            // the next render can check is_showing() and clear the error overlay
            // after the auto-dismiss timeout (10s). Without this, the error
            // indicator persists indefinitely when no other state changes occur.
            if self.error_manager.is_showing() {
                self.dirty = true;
            }

            // Phase 2: Poll async sources (ONE item each)
            self.poll_services()?;

            // Phase 2.5: Update countdowns (rate limit, agents, etc.)
            if self.update_rate_limit_countdown() {
                self.dirty = true; // Mark dirty if countdown updated
            }

            // Update running agents
            self.agent_manager.update_running_agents();

            // Periodic cleanup: remove completed/failed agents older than 1 hour
            // and cap total terminal agents at 50
            self.agent_manager.cleanup_old_agents(3600);
            self.agent_manager.cleanup_excess_agents(50);

            // Session auto-save (every 30s when dirty)
            if let Some(ref mut recovery) = self.session_recovery {
                if recovery.should_auto_save() {
                    let state = recovery.create_state(&self.messages, self.scroll_offset_line);
                    if let Err(e) = recovery.save_state(&state) {
                        tracing::warn!("Session auto-save failed: {}", e);
                    }
                }
            }

            // Execute pending auto-compaction when not streaming
            if self.pending_compaction && !self.is_streaming {
                self.execute_compaction();
            }

            // Phase 3: Check frame budget
            let elapsed = frame_start.elapsed();

            if elapsed < FRAME_BUDGET_60FPS {
                // Phase 4: Render (only if dirty)
                // dirty is set to true when new content arrives, so no need to check is_streaming
                let should_render = self.dirty || frame_count < 3;

                if should_render {
                    terminal.draw(|f| self.render(f))?;
                    frame_count += 1;
                    self.dirty = false; // Clear dirty after render
                }

                // Phase 5: Handle input with remaining time
                // poll() blocks for up to `timeout`, consuming the remaining budget.
                // No additional sleep needed after this — poll handles the yield.
                let timeout = FRAME_BUDGET_60FPS.saturating_sub(frame_start.elapsed());

                if event::poll(timeout)? {
                    self.handle_input()?;
                }
            } else {
                // Frame over budget, skip render, handle input with small timeout
                // to prevent CPU spin when consistently over budget
                if event::poll(Duration::from_millis(1))? {
                    self.handle_input()?;
                }
            }
        }

        // Cleanup: stop any active stream
        if self.is_streaming {
            self.services.request_stop_stream();
            self.stream_cancelled = true;
            // Don't set is_streaming=false here — let the async stream task's
            // Done handler clean up to avoid racing with channel receivers.
        }

        // Shutdown MCP servers to prevent orphaned child processes
        if let Some(mcp_manager) = &self.mcp_manager {
            let mcp = mcp_manager.clone();
            // Spawn a small tokio runtime for async cleanup since we're in sync context
            let _ = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                if let Ok(rt) = rt {
                    rt.block_on(async {
                        let manager = mcp.read().await;
                        manager.shutdown().await;
                    });
                }
            })
            .join();
        }

        // Reset terminal title on exit so it doesn't show stale rustycode state
        print!("\x1b]0;\x07");
        let _ = std::io::stdout().flush();

        // Save history on exit
        self.save_history();

        // Session recovery shutdown: save state and release lock
        if let Some(ref mut recovery) = self.session_recovery {
            let state = recovery.create_state(&self.messages, self.scroll_offset_line);
            if let Err(e) = recovery.shutdown(&state) {
                tracing::warn!("Session recovery shutdown failed: {}", e);
            }
        }

        Ok(())
    }

    /// Handle bracketed paste event
    ///
    /// This handles paste from the terminal's native paste (Cmd+V, Ctrl+Shift+V).
    /// The entire pasted content is received at once, preventing multiple sends.
    pub(crate) fn handle_bracketed_paste(&mut self, content: &str) -> Result<()> {
        use crate::ui::input_state::InputMode;

        if content.is_empty() {
            return Ok(());
        }

        // Check if content has newlines - if so, ensure we are in multiline mode
        // (but don't force it if it's already multiline)
        if content.contains('\n') && self.input_handler.state.mode == InputMode::SingleLine {
            self.input_handler.state.mode = InputMode::MultiLine;
        }

        let state = &mut self.input_handler.state;

        // Split content into lines, preserving empty lines
        let lines: Vec<&str> = content.split('\n').collect();

        if lines.len() == 1 {
            // Single line paste - just insert the string
            let text = lines[0];
            if state.cursor_row < state.lines.len() {
                let current_line = &mut state.lines[state.cursor_row];
                let cursor_col = state.cursor_col.min(current_line.len());
                current_line.insert_str(cursor_col, text);
                state.cursor_col += text.len();
            }
        } else {
            // Multiline paste
            if state.cursor_row < state.lines.len() {
                let cursor_col = state.cursor_col.min(state.lines[state.cursor_row].len());
                let before = state.lines[state.cursor_row][..cursor_col].to_string();
                let after = state.lines[state.cursor_row][cursor_col..].to_string();

                // Replace current line with "before" + first pasted line
                state.lines[state.cursor_row] = format!("{}{}", before, lines[0]);

                // Insert middle lines
                #[allow(clippy::needless_range_loop)]
                for i in 1..lines.len() - 1 {
                    state
                        .lines
                        .insert(state.cursor_row + i, lines[i].to_string());
                }

                // Last line: last pasted part + "after"
                let last_idx = lines.len() - 1;
                let last_pasted_part = lines[last_idx];
                state.lines.insert(
                    state.cursor_row + last_idx,
                    format!("{}{}", last_pasted_part, after),
                );

                // Move cursor to end of pasted content
                state.cursor_row += last_idx;
                state.cursor_col = last_pasted_part.len();
            }
        }

        self.dirty = true;
        Ok(())
    }

    /// Handle a slash command
    pub(crate) fn handle_slash_command(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        // Handle /cost locally (needs TUI state not in CommandContext)
        if matches!(parts[0], "/cost" | "/usage") {
            self.handle_cost_command();
            self.dirty = true;
            self.auto_scroll();
            return Ok(());
        }

        if parts[0] == "/plan" {
            use rustycode_orchestra::plan_mode::ExecutionPhase;
            let current = self.plan_mode.current_phase();
            match current {
                ExecutionPhase::Planning => {
                    self.plan_mode.approve().ok();
                    self.add_system_message(
                        "Plan mode: switched to implementation phase".to_string(),
                    );
                }
                ExecutionPhase::Implementation => {
                    self.plan_mode.reset();
                    self.add_system_message("Plan mode: switched to planning phase".to_string());
                }
            }
            self.dirty = true;
            return Ok(());
        }

        // Handle /r locally as regenerate alias (Goose pattern)
        if matches!(parts[0], "/r" | "/regen" | "/regenerate") {
            self.regenerate_last_response()?;
            return Ok(());
        }

        if let Some(command_tx) = self.services.command_sender() {
            let cwd = self.services.cwd().clone();
            let effect = dispatch_registered_slash_command(
                input,
                CommandContext {
                    cwd: &cwd,
                    command_tx,
                    workspace_tasks: &mut self.workspace_tasks,
                    messages: &mut self.messages,
                    current_stream_content: &mut self.current_stream_content,
                    is_streaming: &mut self.is_streaming,
                    last_extraction: &mut self.last_extraction,
                    services: &mut self.services,
                    agent_manager: &mut self.agent_manager,
                    memory_injection_config: &mut self.memory_injection_config,
                    theme_colors: &self.theme_colors,
                    skill_manager: &self.skill_manager,
                    running: &mut self.running,
                    context_monitor: &mut self.context_monitor,
                    compaction_config: &mut self.compaction_config,
                    showing_compaction_preview: &mut self.showing_compaction_preview,
                    pending_compaction: &mut self.pending_compaction,
                    file_undo_stack: &mut self.file_undo_stack,
                    session_input_tokens: self.session_input_tokens,
                    session_output_tokens: self.session_output_tokens,
                    session_cost_usd: self.session_cost_usd,
                    current_model: self.current_model.clone(),
                    session_start: self.start_time,
                },
            )?;

            if let Some(effect) = effect {
                self.apply_slash_command_effect(effect)?;
                return Ok(());
            }
        }

        let cmd = parts[0];

        {
            self.add_system_message(format!(
                "Unknown command: {}. Type /help for available commands.",
                cmd
            ));
        }

        self.dirty = true;
        self.auto_scroll();
        Ok(())
    }

    /// Apply a shared slash-command effect to the TUI state.
    fn apply_slash_command_effect(&mut self, effect: CommandEffect) -> Result<()> {
        match effect {
            CommandEffect::AsyncStarted(message) | CommandEffect::SystemMessage(message) => {
                self.add_system_message(message);
            }
            CommandEffect::MultipleMessages(messages) => {
                for message in messages {
                    self.add_system_message(message);
                }
            }
            CommandEffect::ShowHelp => {
                self.help_state.visible = true;
                self.help_state.scroll_offset = 0;
                self.add_system_message("Help opened - press Esc to close".to_string());
            }
            CommandEffect::None => {}
            CommandEffect::ModelSwitch { model_id } => {
                self.current_model = model_id.clone();
                let short = model_id.rsplit('/').next().unwrap_or(&model_id);
                self.toast_manager.success(format!("Model: {}", short));
            }
            CommandEffect::ClearConversation => {
                // Signal background stream to stop BEFORE clearing state.
                // Without this, the stream thread keeps running and its Done
                // handler would trigger auto-continue or queued message on
                // the now-empty conversation.
                if self.is_streaming {
                    self.services.request_stop_stream();
                    self.stream_cancelled = true;
                }
                // Reset all scroll and selection state
                self.selected_message = 0;
                self.scroll_offset_line = 0;
                self.user_scrolled = false;
                self.active_tools.clear();
                self.tool_panel_history.clear();
                self.tool_panel_selected_index = None;
                self.showing_tool_result = false;
                // Dismiss all overlays
                self.dismiss_any_overlay();
                // Reset streaming state
                self.is_streaming = false;
                self.current_stream_content.clear();
                self.streaming_render_buffer =
                    crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
                self.chunks_received = 0;
                self.queued_message = None;
                self.stashed_prompt = None;
                // Reset session usage tracking
                self.session_input_tokens = 0;
                self.session_output_tokens = 0;
                self.session_cost_usd = 0.0;
                self.context_monitor.current_tokens = 0;
                self.context_monitor.needs_compaction = false;
                self.add_system_message("Conversation cleared".to_string());
            }
            CommandEffect::StartTeam { task } => {
                self.spawn_team_orchestrator(&task)?;
            }
            CommandEffect::CancelTeam => {
                self.cancel_team();
            }
            CommandEffect::LoadSession {
                name,
                messages,
                summary,
            } => {
                // Signal background stream to stop before loading new session
                if self.is_streaming {
                    self.services.request_stop_stream();
                    self.stream_cancelled = true;
                }
                // Reset scroll and selection state
                self.selected_message = 0;
                self.scroll_offset_line = 0;
                self.user_scrolled = false;
                self.active_tools.clear();
                self.tool_panel_history.clear();
                self.tool_panel_selected_index = None;
                self.showing_tool_result = false;
                // Dismiss all overlays
                self.dismiss_any_overlay();
                // Reset streaming state
                self.is_streaming = false;
                self.current_stream_content.clear();
                self.streaming_render_buffer =
                    crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
                self.chunks_received = 0;
                self.queued_message = None;
                self.stashed_prompt = None;
                // Reset session usage tracking
                self.session_input_tokens = 0;
                self.session_output_tokens = 0;
                self.session_cost_usd = 0.0;
                self.messages = messages;
                self.context_monitor.update(&self.messages);
                if !self.messages.is_empty() {
                    self.selected_message = self.messages.len() - 1;
                }
                self.add_system_message(format!("✓ Loaded session '{}' — {}", name, summary));
            }
            CommandEffect::SetPlanMode { planning } => {
                if planning {
                    self.add_system_message("Plan mode enabled — tools are read-only".to_string());
                } else {
                    self.add_system_message("Plan mode disabled — full tool access".to_string());
                }
            }
            CommandEffect::SetBudget { limit } => {
                if let Some(amount) = limit {
                    self.add_system_message(format!("Budget limit set to ${:.2}", amount));
                } else {
                    self.add_system_message("Budget limit removed".to_string());
                }
            }
            CommandEffect::RetryLastMessage => {
                // Find the last user message and re-send it
                if let Some(last_user_msg) = self
                    .messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                {
                    if !last_user_msg.content.is_empty() {
                        self.retry_last_message(last_user_msg.content.clone());
                    }
                }
            }
        }

        Ok(())
    }

    /// Spawn a TeamOrchestrator on a background thread, subscribe to its
    /// broadcast channel, and wire events into the team panel.
    fn spawn_team_orchestrator(&mut self, task: &str) -> Result<()> {
        use rustycode_core::team::orchestrator::TeamOrchestrator;

        let cwd = std::env::current_dir().unwrap_or_default();

        // Load provider
        let (provider_type, model, v2_config) = rustycode_llm::load_provider_config_from_env()
            .context("Failed to load provider config for team mode")?;

        let provider =
            rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config)
                .context("Failed to create provider for team mode")?;

        let orchestrator = TeamOrchestrator::new(&cwd, provider, model.to_string());
        let event_rx = orchestrator.subscribe();

        // Get cancel token for cooperative cancellation
        let cancel_token = orchestrator.cancel_token();
        self.team_handler.cancel_token = Some(cancel_token);

        // Show the team panel
        self.team_panel.set_task(task);
        self.team_panel.visible = true;
        self.team_panel.reset();
        self.dirty = true;

        self.add_system_message(format!(
            "🤖 Team mode started: \"{}\"\n   Architect → Builder → Skeptic → Judge → Scalpel\n   Press Ctrl+G to toggle team panel | Esc to cancel",
            task
        ));

        // Store the receiver for polling in the event loop
        self.team_handler.event_rx = Some(event_rx);

        // Spawn the orchestrator on a background thread
        let task_owned = task.to_string();
        std::thread::spawn(move || {
            rustycode_shared_runtime::block_on_shared(async move {
                if let Err(e) = orchestrator.execute(&task_owned).await {
                    tracing::error!("Team orchestrator failed: {}", e);
                }
            });
        });

        Ok(())
    }

    /// Cancel a running team orchestrator, Shows a summary and hides the panel.
    pub(crate) fn cancel_team(&mut self) {
        if let Some(token) = &self.team_handler.cancel_token {
            token.store(true, std::sync::atomic::Ordering::SeqCst);
            self.add_system_message("⏹ Team task cancelled.".to_string());
            self.team_panel.visible = false;
            self.team_handler.event_rx = None;
            self.team_handler.cancel_token = None;
            self.dirty = true;
        } else {
            self.add_system_message("⚠ No team task is running.".to_string());
        }
    }

    /// Show session cost and usage summary
    fn handle_cost_command(&mut self) {
        let total_tokens = self.session_input_tokens + self.session_output_tokens;
        let turn_count = self
            .messages
            .iter()
            .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            .count();

        let cost_str = if self.session_cost_usd < 0.001 {
            "negligible".to_string()
        } else if self.session_cost_usd < 0.01 {
            format!("${:.4}", self.session_cost_usd)
        } else {
            format!("${:.2}", self.session_cost_usd)
        };

        let token_str = if total_tokens >= 1_000_000 {
            format!("{:.1}M", total_tokens as f64 / 1_000_000.0)
        } else if total_tokens >= 1_000 {
            format!("{:.1}k", total_tokens as f64 / 1_000.0)
        } else {
            total_tokens.to_string()
        };

        let input_str = if self.session_input_tokens >= 1_000 {
            format!("{:.1}k", self.session_input_tokens as f64 / 1_000.0)
        } else {
            self.session_input_tokens.to_string()
        };

        let output_str = if self.session_output_tokens >= 1_000 {
            format!("{:.1}k", self.session_output_tokens as f64 / 1_000.0)
        } else {
            self.session_output_tokens.to_string()
        };

        let ctx_pct = if self.context_monitor.max_tokens > 0 {
            format!("{:.0}%", self.context_monitor.usage_percentage() * 100.0)
        } else {
            "N/A".to_string()
        };

        let model_display = self
            .current_model
            .rsplit('/')
            .next()
            .unwrap_or(&self.current_model)
            .to_string();

        let summary = format!(
            "Session Usage ({} turns, {}):\n  Tokens: {} total ({} in / {} out)\n  Context: {} used\n  Cost: {} ({})\n  API calls: {}",
            turn_count, model_display, token_str, input_str, output_str, ctx_pct, cost_str,
            if self.session_cost_usd > 0.0 { "estimated" } else { "free/local model" },
            self.cost_tracker.calls_count(),
        );

        let mut full_summary = summary;

        let by_tool = self.cost_tracker.costs_by_tool();
        if !by_tool.is_empty() {
            let tool_breakdown: Vec<String> = by_tool
                .iter()
                .filter(|(_, c)| **c > 0.0)
                .map(|(tool, cost)| format!("    {}: ${:.4}", tool, cost))
                .collect();
            if !tool_breakdown.is_empty() {
                full_summary.push_str("\n  Cost by tool:\n");
                full_summary.push_str(&tool_breakdown.join("\n"));
            }
        }

        self.add_system_message(full_summary);
        // Ensure UI shows the latest summary by auto-scrolling to bottom
        self.auto_scroll();
    }

    /// Print session summary to stdout after TUI exits.
    ///
    /// Goose pattern: users see a one-line cost/duration summary in their terminal
    /// after the TUI exits, making it easy to track spending across sessions.
    fn print_session_summary(&self) {
        let turn_count = self
            .messages
            .iter()
            .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            .count();
        let total_tokens = self.session_input_tokens + self.session_output_tokens;
        let model = self
            .current_model
            .rsplit('/')
            .next()
            .unwrap_or(&self.current_model);

        let fmt = |n: usize| -> String {
            if n >= 1_000_000 {
                format!("{:.1}M", n as f64 / 1_000_000.0)
            } else if n >= 1_000 {
                format!("{:.1}k", n as f64 / 1_000.0)
            } else {
                n.to_string()
            }
        };

        let cost = if self.session_cost_usd > 0.01 {
            format!("${:.2}", self.session_cost_usd)
        } else if self.session_cost_usd > 0.0 {
            format!("${:.4}", self.session_cost_usd)
        } else {
            "free".to_string()
        };

        // Only print if there was actual activity
        if turn_count > 0 {
            println!(
                "\n  Session: {} turns, {} tokens ({} in / {} out), {}, model: {}",
                turn_count,
                fmt(total_tokens),
                fmt(self.session_input_tokens),
                fmt(self.session_output_tokens),
                cost,
                model
            );
        }
    }

    /// Update terminal window/tab title dynamically based on state.
    ///
    /// Goose pattern: users with many terminal tabs can see at a glance
    /// whether the AI is idle, thinking, or running tools.
    pub(crate) fn update_terminal_title(&self) {
        if let Some(dir_name) = self.services.cwd().file_name().and_then(|n| n.to_str()) {
            let sanitized: String = dir_name.chars().filter(|c| !c.is_control()).collect();
            let state = if self.is_streaming {
                if self.active_tools.is_empty() {
                    "thinking"
                } else {
                    "tools"
                }
            } else {
                "ready"
            };
            print!("\x1b]0;rustycode: {} [{}]\x07", sanitized, state);
            let _ = std::io::stdout().flush();
        }
    }

    pub(crate) fn apply_model_switch(&mut self, model: &crate::ui::model_selector::ModelInfo) {
        std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", &model.id);
        std::env::set_var("RUSTYCODE_PROVIDER_OVERRIDE", &model.provider);
        self.current_model = model.id.clone();
        self.compaction_config.model_id = Some(model.id.clone());
        self.add_system_message(format!(
            "✓ Model switched to `{}` ({}). New requests will use this model.",
            model.name, model.provider
        ));
        self.model_selector.hide();
        self.dirty = true;
    }

    /// Update rate limit countdown message with auto-retry
    fn update_rate_limit_countdown(&mut self) -> bool {
        // Capture message_index BEFORE update_countdown() clears it on expiry.
        let saved_msg_idx = self.rate_limit.message_index;

        // Use the rate limit handler to update countdown
        if let Some(new_content) = self.rate_limit.update_countdown() {
            // Update the countdown message in-place (if index is still valid)
            if let Some(msg_idx) = saved_msg_idx {
                if let Some(message) = self.messages.get_mut(msg_idx) {
                    message.content = new_content;
                    self.dirty = true;
                }
            }

            // Check if we should auto-retry (countdown expired, not cancelled)
            if self.rate_limit.should_auto_retry() {
                if let Some(last_msg) = self.rate_limit.take_last_message() {
                    self.retry_last_message(last_msg);
                }
            }
            return true;
        }
        false
    }

    /// Render the full TUI frame by delegating to sub-render methods
    pub(crate) fn render(&mut self, frame: &mut ratatui::Frame) {
        use crate::ui::footer::Footer;
        use crate::ui::header::Header;
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Style};
        use ratatui::widgets::{Block, Clear};

        let size = frame.area();

        // Minimum size guard: if terminal is too small, show a message instead
        if size.width < 40 || size.height < 8 {
            frame.render_widget(Clear, size);
            let msg = ratatui::widgets::Paragraph::new("Terminal too small (min 40×8)")
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(msg, size);
            return;
        }

        // Brutalist mode uses its own complete renderer
        if self.brutalist_mode {
            self.render_brutalist(frame);
            return;
        }

        // Clear the entire frame first to prevent text overlap from previous renders
        frame.render_widget(Clear, size);

        // Auto-collapse chrome on small terminals to maximize message space
        // Minimum layout: header(1) + input(3) = 4 rows, leaving rest for messages.
        // Note: We don't auto-restore collapsed state because the user may have
        // manually collapsed via Ctrl+Shift+H — they can restore it themselves.
        // The resize handler already resets scroll_offset_line to prevent blank screens.
        if size.height < 12 {
            self.status_bar_collapsed = true;
            self.footer_collapsed = true;
        }

        // New polished layout: Header | Status Bar | Messages | Input | Footer
        // Following the TUI redesign spec for visual hierarchy
        // Collapsible sections: status bar and footer can be hidden
        let status_bar_height = if self.status_bar_collapsed { 0 } else { 1 };
        let footer_height = if self.footer_collapsed { 0 } else { 1 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),                 // Header (1 row)
                Constraint::Length(status_bar_height), // Status Bar (collapsible)
                Constraint::Min(0),                    // Messages (flexible, min 0)
                Constraint::Length(3),                 // Input Area (3 rows)
                Constraint::Length(footer_height),     // Footer (collapsible)
            ])
            .split(size);

        // Update viewport height from the messages area
        self.viewport_height = chunks[2].height.max(1) as usize;
        self.messages_area.set(chunks[2]);

        // Render polished header with explicit background
        let task_count = self.workspace_tasks.tasks.len();
        let pending_tools = self.active_tools.len();
        let project_name = self
            .services
            .cwd()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Render header background first
        let header_bg = Block::default().style(Style::default().bg(Color::Rgb(23, 23, 23)));
        frame.render_widget(header_bg, chunks[0]);

        // Determine header status (goose pattern: color-coded status in header)
        let header_status = if self.error_manager.is_showing() {
            crate::ui::header::HeaderStatus::Error
        } else if self.is_streaming {
            if self.active_tools.is_empty() {
                crate::ui::header::HeaderStatus::Thinking
            } else {
                crate::ui::header::HeaderStatus::RunningTools
            }
        } else {
            crate::ui::header::HeaderStatus::Ready
        };

        let header = Header::new()
            .with_app_name("rustycode")
            .with_project_name(project_name)
            .with_git_branch(self.git_branch.clone())
            .with_counts(task_count, pending_tools)
            .with_turn_count(
                self.messages
                    .iter()
                    .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                    .count(),
            )
            .with_status(header_status)
            .with_spinner_frame(self.animator.current_frame().progress_frame / 5);
        header.render(frame, chunks[0]);

        // Render status bar (existing implementation) - skip if collapsed
        if !self.status_bar_collapsed {
            self.render_status_safe(frame, chunks[1]);
        }

        // Render messages
        self.render_messages_safe(frame, chunks[2]);

        // Render input area
        self.render_input_safe(frame, chunks[3]);

        // Render polished footer - skip if collapsed
        if !self.footer_collapsed {
            // Render footer background first
            let footer_bg = Block::default().style(Style::default().bg(Color::Rgb(23, 23, 23)));
            frame.render_widget(footer_bg, chunks[4]);

            let session_secs = self.start_time.elapsed().as_secs();
            // Build task summary for footer (e.g., "✓5 ☐3")
            let done_count = self
                .workspace_tasks
                .tasks
                .iter()
                .filter(|t| matches!(t.status, crate::tasks::TaskStatus::Completed))
                .count();
            let pending_count = self
                .workspace_tasks
                .tasks
                .iter()
                .filter(|t| matches!(t.status, crate::tasks::TaskStatus::Pending))
                .count();
            let task_summary = if done_count > 0 || pending_count > 0 {
                format!("✓{} ☐{}", done_count, pending_count)
            } else {
                String::new()
            };
            let footer = Footer::new()
                .with_session_duration(Footer::format_duration(session_secs))
                .with_task_summary(task_summary)
                .with_model(
                    self.current_model
                        .rsplit('/')
                        .next()
                        .map(|s| s.strip_prefix("claude-").unwrap_or(s))
                        .unwrap_or(&self.current_model)
                        .to_string(),
                )
                .with_session_cost(self.session_cost_usd);
            footer.render(frame, chunks[4]);
        }

        // Overlay: search box (over message area - chunks[2])
        if self.search_state.visible {
            self.render_search_box(frame, chunks[2]);
        }

        // Tool panel overlay (Ctrl+P) - over message area
        if self.showing_tool_panel {
            self.render_tool_panel(frame, chunks[2]);
        }

        // Worker status panel overlay (Ctrl+W) - right side overlay
        if self.worker_panel.visible {
            self.render_worker_panel(frame, chunks[2]);
        }

        // Team agent timeline overlay (Ctrl+G) - right side overlay
        if self.team_panel.visible {
            frame.render_widget(ratatui::widgets::Clear, chunks[2]);
            frame.render_widget(self.team_panel.clone(), chunks[2]);
        }

        // Overlay: clarification panel (when AI asks a question)
        if self.awaiting_clarification && self.clarification_panel.visible {
            // Render as a centered popup covering the middle of the screen
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let x = (size.width.saturating_sub(panel_width)) / 2;
            let y = (size.height.saturating_sub(panel_height)) / 2;
            let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.clarification_panel.clone(), panel_area);
        }

        // Overlay: provider selector
        if self.showing_provider_selector {
            self.render_provider_selector(frame);
        }

        // Overlay: file finder
        if self.file_finder.is_visible() {
            self.file_finder.render(frame, size);
        }

        // Overlay: model selector (Alt+P)
        if self.model_selector.is_visible() {
            self.model_selector.render(frame, size);
        }

        // Overlay: file selector (@)
        if self.file_selector.is_visible() {
            self.file_selector.render(frame, size);
        }

        // Overlay: skill palette
        if self.skill_palette.is_visible() {
            self.skill_palette.render(frame, size);
        }

        // Overlay: theme preview
        if self.theme_preview.is_visible() {
            self.theme_preview.render(frame, size);
        }

        // Overlay: command palette (Ctrl+K)
        if self.command_palette.is_visible() {
            self.command_palette.render(frame, size);
        }

        // Overlay: help panel (?)
        if self.help_state.visible {
            crate::help::render_help(frame, size, &self.help_state);
        }

        // Overlay: approval dialog
        if self.awaiting_approval {
            if let Some(ref req) = self.pending_approval_request {
                let panel_height = 12u16.min(size.height.saturating_sub(4));
                let panel_width = 70u16.min(size.width.saturating_sub(4));
                let x = (size.width.saturating_sub(panel_width)) / 2;
                let y = (size.height.saturating_sub(panel_height)) / 2;
                let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);
                // render_approval_prompt calls Clear internally
                crate::tool_approval::render_approval_prompt(frame, panel_area, req);
            }
        }

        // Overlay: error display
        if self.error_manager.is_showing() {
            frame.render_widget(ratatui::widgets::Clear, size);
            self.error_manager.render(frame, size);
        }

        // Overlay: session sidebar (Ctrl+B)
        if self.session_sidebar.is_visible() {
            self.session_sidebar.render(frame, size);
        }

        // Overlay: compaction preview (while pending)
        if self.showing_compaction_preview {
            self.render_compaction_preview(frame, size);
        }

        // Overlay: first-run wizard (covers entire screen)
        if self.wizard.showing_wizard {
            if let Some(ref mut wizard) = self.wizard.wizard {
                frame.render_widget(ratatui::widgets::Clear, size);
                wizard.render(frame, size);
            }
        }

        // Overlay: toast notifications (topmost — always visible)
        self.toast_manager.render(frame, size);
    }

    /// Render messages with panic recovery
    fn render_messages_safe(&mut self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.render_messages(frame, area);
        }));
        if result.is_err() {
            tracing::error!("Panic in render_messages — showing fallback");
            Self::render_fallback_error(frame, area, "Message render error");
        }
    }

    /// Render input with panic recovery
    fn render_input_safe(&mut self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.render_input(frame, area);
        }));
        if result.is_err() {
            tracing::error!("Panic in render_input — showing fallback");
            Self::render_fallback_error(frame, area, "Input render error");
        }
    }

    /// Render status bar with panic recovery
    fn render_status_safe(&mut self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.render_status(frame, area);
        }));
        if result.is_err() {
            tracing::error!("Panic in render_status — showing fallback");
            Self::render_fallback_error(frame, area, "Status render error");
        }
    }

    /// Fallback error display when a component panics
    fn render_fallback_error(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, msg: &str) {
        use ratatui::style::{Color, Style};
        use ratatui::widgets::Paragraph;

        let text = ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(format!("⚠ {} ", msg), Style::default().fg(Color::Yellow)),
            ratatui::text::Span::styled(
                "(component recovered)",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        let paragraph = Paragraph::new(text);
        frame.render_widget(paragraph, area);
    }

    /// Render using brutalist renderer (complete UI)
    fn render_brutalist(&mut self, frame: &mut ratatui::Frame) {
        let input_text = self.input_handler.state.all_text();

        // Compute dynamic input area height based on content lines
        let input_line_count = if input_text.is_empty() {
            1
        } else {
            input_text.lines().count().max(1)
        };
        let input_rows: u16 = if input_line_count > 1 {
            2u16.saturating_add(input_line_count.min(6) as u16)
        } else {
            2
        };

        // Update viewport height from brutalist layout accounting for collapsed sections
        let size = frame.area();
        let header_rows: u16 = if self.status_bar_collapsed { 0 } else { 1 };
        let footer_rows: u16 = if self.footer_collapsed { 0 } else { 1 };
        let fixed_rows = header_rows + footer_rows + input_rows;
        let main_height = size.height.saturating_sub(fixed_rows);
        self.viewport_height = main_height.max(1) as usize;

        // Set messages area for mouse click detection (scroll-to-bottom indicator)
        self.messages_area.set(ratatui::layout::Rect {
            x: size.x,
            y: header_rows, // After header (0 if collapsed)
            width: size.width,
            height: main_height,
        });

        let renderer = self.create_brutalist_renderer(&input_text);

        // Compute message layout once — reused for rendering and click areas.
        // Using render_with_heights avoids recomputing heights inside render_messages.
        let width = size.width as usize;
        let (total_lines, heights) = renderer.compute_message_layout(width);

        // Render the complete brutalist UI with precomputed heights
        renderer.render_with_heights(frame, &heights);

        // Register message click areas for mouse interaction.
        // Uses the pre-computed heights to avoid redundant estimation.
        self.clear_message_areas();
        let main_height_click = size.height.saturating_sub(fixed_rows) as usize;
        let main_y = header_rows;
        let safe_viewport = main_height_click.max(1);

        // Save total lines for scroll operations (scroll_down_by, page_up, etc.)
        self.last_total_lines.set(total_lines);

        // Populate message_line_offsets from pre-computed heights so turn-based
        // navigation (Shift+Up/Down) can scroll to the correct position.
        // Without this, navigate_to_prev_turn/next_turn falls back to i*3 estimate.
        {
            let mut offsets = self.message_line_offsets.borrow_mut();
            offsets.clear();
            offsets.resize(self.messages.len(), 0);
            let mut acc = 0usize;
            for (msg_idx, &h) in heights.iter().enumerate() {
                offsets[msg_idx] = acc;
                acc += h;
            }
        }

        let max_scroll = total_lines.saturating_sub(safe_viewport);
        let effective_offset = if self.user_scrolled {
            self.scroll_offset_line.min(max_scroll)
        } else {
            max_scroll
        };

        let mut cum_line = 0usize;
        for (msg_idx, &h) in heights.iter().enumerate() {
            let end_line = cum_line + h;
            if end_line <= effective_offset {
                cum_line += h;
                continue;
            }
            if cum_line >= effective_offset + safe_viewport {
                break;
            }
            let vis_start = cum_line.saturating_sub(effective_offset);
            let vis_end = (end_line.saturating_sub(effective_offset)).min(safe_viewport);
            let vis_height = vis_end.saturating_sub(vis_start) as u16;
            if vis_height > 0 {
                let area = ratatui::layout::Rect {
                    x: size.x,
                    y: main_y + vis_start as u16,
                    width: size.width,
                    height: vis_height,
                };
                self.register_message_area(msg_idx, area);
            }
            cum_line += h;
        }

        // Overlay panels on top of brutalist UI

        // Worker status panel overlay (Ctrl+W)
        if self.worker_panel.visible {
            let panel_width = 50u16.min(size.width.saturating_sub(10));
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let x = size.width.saturating_sub(panel_width);
            let y = 2u16;
            let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            self.worker_panel.render(panel_area, frame.buffer_mut());
        }

        // Team agent timeline overlay (Ctrl+G)
        if self.team_panel.visible {
            let panel_width = 60u16.min(size.width.saturating_sub(10));
            let panel_height = 20u16.min(size.height.saturating_sub(4));
            let x = size.width.saturating_sub(panel_width);
            let y = 2u16;
            let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.team_panel.clone(), panel_area);
        }

        // Overlay: clarification panel (when AI asks a question)
        if self.awaiting_clarification && self.clarification_panel.visible {
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let x = (size.width.saturating_sub(panel_width)) / 2;
            let y = (size.height.saturating_sub(panel_height)) / 2;
            let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.clarification_panel.clone(), panel_area);
        }

        // Overlay: search box (position at bottom of message area, not over footer)
        if self.search_state.visible {
            let search_area = ratatui::layout::Rect {
                x: size.x,
                y: header_rows,
                width: size.width,
                height: main_height,
            };
            self.render_search_box(frame, search_area);
        }

        // Tool panel overlay (Ctrl+P) - over message area
        if self.showing_tool_panel {
            let tool_area = ratatui::layout::Rect {
                x: size.x,
                y: header_rows,
                width: size.width,
                height: main_height,
            };
            self.render_tool_panel(frame, tool_area);
        }

        // Overlay: provider selector
        if self.showing_provider_selector {
            self.render_provider_selector(frame);
        }

        // Overlay: file finder
        if self.file_finder.is_visible() {
            self.file_finder.render(frame, size);
        }

        // Overlay: model selector (Alt+P)
        if self.model_selector.is_visible() {
            self.model_selector.render(frame, size);
        }

        // Overlay: file selector (@)
        if self.file_selector.is_visible() {
            self.file_selector.render(frame, size);
        }

        // Overlay: skill palette
        if self.skill_palette.is_visible() {
            self.skill_palette.render(frame, size);
        }

        // Overlay: theme preview
        if self.theme_preview.is_visible() {
            self.theme_preview.render(frame, size);
        }

        // Overlay: command palette (Ctrl+K)
        if self.command_palette.is_visible() {
            self.command_palette.render(frame, size);
        }

        // Overlay: help panel (?)
        if self.help_state.visible {
            crate::help::render_help(frame, size, &self.help_state);
        }

        // Overlay: approval dialog (before error display so errors can appear on top)
        if self.awaiting_approval {
            if let Some(ref req) = self.pending_approval_request {
                let panel_height = 12u16.min(size.height.saturating_sub(4));
                let panel_width = 70u16.min(size.width.saturating_sub(4));
                let x = (size.width.saturating_sub(panel_width)) / 2;
                let y = (size.height.saturating_sub(panel_height)) / 2;
                let panel_area = ratatui::layout::Rect::new(x, y, panel_width, panel_height);
                crate::tool_approval::render_approval_prompt(frame, panel_area, req);
            }
        }

        // Overlay: error display
        if self.error_manager.is_showing() {
            self.error_manager.render(frame, size);
        }

        // Overlay: session sidebar (Ctrl+B)
        if self.session_sidebar.is_visible() {
            self.session_sidebar.render(frame, size);
        }

        // Overlay: compaction preview (while pending)
        if self.showing_compaction_preview {
            self.render_compaction_preview(frame, size);
        }

        // Overlay: first-run wizard (covers entire screen)
        if self.wizard.showing_wizard {
            if let Some(ref mut wizard) = self.wizard.wizard {
                frame.render_widget(ratatui::widgets::Clear, size);
                wizard.render(frame, size);
            }
        }

        // Overlay: toast notifications (topmost — always visible)
        self.toast_manager.render(frame, size);
    }

    /// Render compaction preview overlay
    fn render_compaction_preview(&self, frame: &mut ratatui::Frame, size: ratatui::layout::Rect) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let width = 50u16.min(size.width.saturating_sub(4));
        let height = 5u16;
        let x = (size.width.saturating_sub(width)) / 2;
        let y = (size.height.saturating_sub(height)) / 2;
        let area = ratatui::layout::Rect::new(x, y, width, height);

        frame.render_widget(Clear, area);

        let text = ratatui::text::Line::from(vec![ratatui::text::Span::styled(
            "💾 Compacting context...",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]);

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title("Auto-Compact")
                    .title_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(ratatui::layout::Alignment::Center)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Save command history on exit
    pub(crate) fn save_history(&mut self) {
        let history = self.input_handler.get_history();
        if let Err(e) = crate::session::save_command_history(history) {
            tracing::warn!("Failed to save command history: {}", e);
        }
    }
}

impl Default for TUI {
    fn default() -> Self {
        #[cfg(test)]
        {
            // Use the lightweight test constructor when running tests to avoid
            // terminal/IO dependencies in `Default::default()` during test runs.
            Self::new_for_test()
        }

        #[cfg(not(test))]
        {
            Self::new(std::path::PathBuf::from("."), AiMode::default(), false)
                .expect("Failed to create TUI")
        }
    }
}

// ============================================================================
// CHANNEL TYPES
// Note: These types are defined here but wiring to actual services
// will be done as part of future async service integration
// ============================================================================

/// Event from async services
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AsyncEvent {
    /// Stream chunk from LLM
    StreamChunk { delta: String, finished: bool },
    /// Tool execution result
    ToolResult {
        tool_name: String,
        success: bool,
        output: String,
    },
    /// Command result
    CommandResult { success: bool, output: String },
    /// Workspace update
    WorkspaceUpdate { file_count: usize },
}

/// Sender for async events
pub type AsyncEventSender = mpsc::Sender<AsyncEvent>;

/// Receiver for async events
pub type AsyncEventReceiver = mpsc::Receiver<AsyncEvent>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::message::{Message, MessageRole, ToolExecution, ToolStatus};
    use chrono::Utc;

    #[test]
    fn test_tui_creation() {
        let tui = TUI::new(std::path::PathBuf::from("."), AiMode::default(), false);
        assert!(tui.is_ok());
    }

    #[test]
    fn test_default_tui() {
        let tui = TUI::default();
        assert_eq!(tui.messages.len(), 0);
        assert_eq!(tui.input_mode, InputMode::SingleLine);
    }

    // Tests to prevent None-panics in rendering

    #[test]
    fn test_render_message_without_tools() {
        // Create a message without tool_executions (None)
        let message = Message::new(MessageRole::User, "Hello, world!".to_string());

        // Verify tool_executions is None
        assert!(message.tool_executions.is_none());

        // Verify we can check has_thinking without panicking
        assert!(!message.has_thinking());
    }

    #[test]
    fn test_render_message_without_thinking() {
        // Create a message without thinking (None)
        let message = Message::new(MessageRole::Assistant, "Hi there!".to_string());

        // Verify thinking is None
        assert!(message.thinking.is_none());

        // Verify has_thinking returns false
        assert!(!message.has_thinking());
    }

    #[test]
    fn test_render_message_with_empty_tools() {
        // Create a message with empty tool_executions
        let mut message = Message::new(MessageRole::Assistant, "Done!".to_string());

        // Set tools to empty array
        message.tool_executions = Some(vec![]);

        // Verify tool_executions is Some but empty
        assert!(message.tool_executions.is_some());
        assert_eq!(message.tool_executions.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_render_message_with_thinking() {
        // Create a message with thinking
        let mut message = Message::new(MessageRole::Assistant, "Answer!".to_string());

        // Set thinking
        message.thinking = Some("Let me think...".to_string());

        // Verify thinking is Some
        assert!(message.thinking.is_some());
        assert!(message.has_thinking());
    }

    #[test]
    fn test_render_all_message_states() {
        // Test message without tools or thinking
        let msg1 = Message::new(MessageRole::User, "Test".to_string());
        assert!(msg1.tool_executions.is_none());
        assert!(msg1.thinking.is_none());
        assert!(!msg1.has_thinking());

        // Test message with tools only
        let mut msg2 = Message::new(MessageRole::Assistant, "Test".to_string());
        msg2.tool_executions = Some(vec![ToolExecution {
            tool_id: "test_123".to_string(),
            name: "test_tool".to_string(),
            status: ToolStatus::Complete,
            start_time: Utc::now(),
            end_time: Some(Utc::now()),
            duration_ms: Some(100),
            result_summary: "success".to_string(),
            detailed_output: Some("success".to_string()),
            input_json: None,
            progress_current: None,
            progress_total: None,
            progress_description: None,
        }]);
        assert!(msg2.tool_executions.is_some());
        assert!(msg2.thinking.is_none());
        assert!(!msg2.has_thinking());

        // Test message with thinking only
        let mut msg3 = Message::new(MessageRole::Assistant, "Test".to_string());
        msg3.thinking = Some("thinking...".to_string());
        assert!(msg3.tool_executions.is_none());
        assert!(msg3.thinking.is_some());
        assert!(msg3.has_thinking());

        // Test message with both
        let mut msg4 = Message::new(MessageRole::Assistant, "Test".to_string());
        msg4.tool_executions = Some(vec![]);
        msg4.thinking = Some("thinking...".to_string());
        assert!(msg4.tool_executions.is_some());
        assert!(msg4.thinking.is_some());
        assert!(msg4.has_thinking());
    }

    #[test]
    fn test_message_defaults() {
        // Test that Message::new() creates valid defaults
        let message = Message::new(MessageRole::User, "Test".to_string());

        // Verify all Option fields are None by default
        assert!(message.tool_executions.is_none());
        assert!(message.thinking.is_none());
        assert_eq!(
            message.tools_expansion,
            crate::ui::message::ExpansionLevel::default()
        );
        assert_eq!(
            message.thinking_expansion,
            crate::ui::message::ExpansionLevel::default()
        );
        assert!(message.focused_tool_index.is_none());
        assert!(!message.collapsed);
    }

    #[test]
    fn test_message_with_tools_but_none_thinking() {
        // Test a common case: assistant message with tools but no thinking
        let mut message = Message::new(
            MessageRole::Assistant,
            "I'll help you with that.".to_string(),
        );

        message.tool_executions = Some(vec![ToolExecution {
            tool_id: "read_123".to_string(),
            name: "read_file".to_string(),
            status: ToolStatus::Complete,
            start_time: Utc::now(),
            end_time: Some(Utc::now()),
            duration_ms: Some(50),
            result_summary: "read_file: success".to_string(),
            detailed_output: Some("file contents".to_string()),
            input_json: None,
            progress_current: None,
            progress_total: None,
            progress_description: None,
        }]);

        // This should not panic
        assert!(message.tool_executions.is_some());
        assert!(message.thinking.is_none());
        assert!(!message.has_thinking());
    }

    #[test]
    fn test_message_expansion_levels() {
        let message = Message::new(MessageRole::User, "Test".to_string());

        // Test that expansion levels work correctly
        assert_eq!(
            message.tools_expansion,
            crate::ui::message::ExpansionLevel::Collapsed
        );
        assert_eq!(
            message.thinking_expansion,
            crate::ui::message::ExpansionLevel::Collapsed
        );
    }
}
