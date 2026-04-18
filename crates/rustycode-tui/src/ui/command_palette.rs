//! Command Palette Component
//!
//! This module provides a VS Code-style command palette for the TUI.
//!
//! ## Features
//!
//! - **Fuzzy matching**: Substring search with relevance ranking
//! - **Keyboard navigation**: Arrow keys, Enter to select, Esc to close
//! - **Modal dialog**: Centered overlay with ~60% width, ~40% height
//! - **Built-in commands**: Help, clear, quit, theme, model, save, load
//! - **Extensible**: Easy to add custom commands
//!
//! ## Usage
//!
//! ```rust,no_run

// Complete implementation - pending integration with keyboard shortcuts
//! use rustycode_tui::ui::command_palette::{CommandPalette, Command};
//! use crossterm::event::{KeyCode, KeyEvent};
//!
//! // Create command palette with default commands
//! let mut palette = CommandPalette::new();
//!
//! // Handle keyboard input
//! palette.handle_key(KeyEvent::new(KeyCode::Char('h'), crossterm::event::KeyModifiers::NONE));
//! palette.handle_key(KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE));
//! palette.handle_key(KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE));
//!
//! // Check if a command was selected
//! if let Some(command) = palette.take_selected() {
//!     (command.handler)();  // Execute command
//! }
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::fmt;

// ============================================================================
// COMMAND SYSTEM
// ============================================================================

/// Command that can be executed from the palette
#[derive(Clone)]
pub struct Command {
    /// Unique command identifier (e.g., "help", "clear")
    pub name: String,

    /// Human-readable description (e.g., "Show help dialog")
    pub description: String,

    /// Argument hint shown inline (e.g., "<subcommand> [args]")
    pub argument_hint: String,

    /// Function to execute when command is selected
    pub handler: CommandHandler,
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("argument_hint", &self.argument_hint)
            .field("handler", &"<function>")
            .finish()
    }
}

/// Command handler function type
///
/// This is a callable that executes the command logic.
/// It returns a `CommandResult` indicating success or failure.
pub type CommandHandler = fn() -> CommandResult;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandResult {
    /// Command executed successfully
    Success,

    /// Command executed with a message
    SuccessWithMessage(String),

    /// Command failed
    Error(String),

    /// Command should close the palette
    Close,
}

impl CommandResult {
    /// Check if result indicates the palette should close
    pub fn should_close(&self) -> bool {
        matches!(self, Self::Close)
    }
}

impl Command {
    /// Create a new command
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        handler: CommandHandler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            argument_hint: String::new(),
            handler,
        }
    }

    /// Create a new command with an argument hint
    pub fn with_hint(
        name: impl Into<String>,
        description: impl Into<String>,
        argument_hint: impl Into<String>,
        handler: CommandHandler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            argument_hint: argument_hint.into(),
            handler,
        }
    }

    /// Execute this command
    pub fn execute(&self) -> CommandResult {
        (self.handler)()
    }
}

// ============================================================================
// FUZZY MATCHING
// ============================================================================

/// Match relevance score
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MatchScore {
    /// No match
    None = 0,
    /// Substring match
    Substring = 1,
    /// Prefix match
    Prefix = 2,
    /// Exact match
    Exact = 3,
}

/// Fuzzy matcher for command search
#[derive(Debug, Clone)]
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Create a new fuzzy matcher
    pub fn new() -> Self {
        Self
    }

    /// Calculate match score for a query against a command
    pub fn match_score(&self, query: &str, command: &Command) -> MatchScore {
        let query_lower = query.to_lowercase();
        let name_lower = command.name.to_lowercase();
        let desc_lower = command.description.to_lowercase();

        // Exact match in name
        if name_lower == query_lower {
            return MatchScore::Exact;
        }

        // Prefix match in name
        if name_lower.starts_with(&query_lower) {
            return MatchScore::Prefix;
        }

        // Substring match in name
        if name_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        // Substring match in description
        if desc_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        MatchScore::None
    }

    /// Filter and rank commands by query
    pub fn filter_commands(&self, query: &str, commands: &[Command]) -> Vec<(usize, MatchScore)> {
        let mut matches: Vec<(usize, MatchScore)> = commands
            .iter()
            .enumerate()
            .filter_map(|(idx, cmd)| {
                let score = self.match_score(query, cmd);
                if score != MatchScore::None {
                    Some((idx, score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (descending)
        matches.sort_by_key(|a| std::cmp::Reverse(a.1));

        matches
    }

    /// Highlight matching characters in text
    pub fn highlight_matches(&self, text: &str, query: &str) -> Line<'_> {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        if query.is_empty() {
            return Line::from(text.to_string());
        }

        let mut spans = Vec::new();
        let mut last_idx = 0;

        // Find all matches
        while let Some(idx) = text_lower[last_idx..].find(&query_lower) {
            let absolute_idx = last_idx + idx;

            // Add text before match
            if absolute_idx > last_idx {
                let before = &text[last_idx..absolute_idx];
                spans.push(Span::raw(before.to_string()));
            }

            // Add highlighted match
            let match_end = absolute_idx + query.len();
            let matched = &text[absolute_idx..match_end];
            spans.push(Span::styled(
                matched.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

            last_idx = match_end;
        }

        // Add remaining text
        if last_idx < text.len() {
            let remaining = &text[last_idx..];
            spans.push(Span::raw(remaining.to_string()));
        }

        // If no matches found, return original text
        if spans.is_empty() {
            Line::from(text.to_string())
        } else {
            Line::from(spans)
        }
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// COMMAND PALETTE STATE
// ============================================================================

/// Command palette state
#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    /// Current search query
    pub query: String,

    /// Available commands
    pub commands: Vec<Command>,

    /// Filtered and ranked command indices (index into commands)
    pub filtered_indices: Vec<usize>,

    /// Currently selected index (into filtered_indices)
    pub selected_index: usize,

    /// Whether the palette is visible
    pub visible: bool,

    /// Fuzzy matcher
    matcher: FuzzyMatcher,
}

impl CommandPaletteState {
    /// Create new command palette state
    pub fn new(commands: Vec<Command>) -> Self {
        let filtered_indices = (0..commands.len()).collect();

        Self {
            query: String::new(),
            commands,
            filtered_indices,
            selected_index: 0,
            visible: false,
            matcher: FuzzyMatcher::new(),
        }
    }

    /// Show the palette
    pub fn show(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected_index = 0;
        self.update_filtered();
    }

    /// Hide the palette
    pub fn hide(&mut self) {
        self.visible = false;
        self.query.clear();
        self.selected_index = 0;
        self.update_filtered();
    }

    /// Toggle palette visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Update filtered commands based on current query
    pub fn update_filtered(&mut self) {
        self.filtered_indices = if self.query.is_empty() {
            // Show all commands when query is empty
            (0..self.commands.len()).collect()
        } else {
            // Filter and rank by relevance
            self.matcher
                .filter_commands(&self.query, &self.commands)
                .into_iter()
                .map(|(idx, _)| idx)
                .collect()
        };

        // Clamp selected index
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered_indices.len() - 1);
        }
    }

    /// Get currently selected command (if any)
    pub fn selected_command(&self) -> Option<&Command> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.commands.get(idx))
    }

    /// Add a character to the query
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filtered();
    }

    /// Remove last character from query (backspace)
    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filtered();
    }

    /// Clear the query
    pub fn clear_query(&mut self) {
        self.query.clear();
        self.update_filtered();
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_indices.len() - 1);
        }
    }

    /// Get number of filtered commands
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }
}

// ============================================================================
// COMMAND PALETTE RENDERER
// ============================================================================

/// Command palette renderer
pub struct CommandPaletteRenderer {
    /// Visual state
    state: CommandPaletteState,
}

impl CommandPaletteRenderer {
    /// Create a new command palette with default commands
    pub fn new() -> Self {
        Self::with_commands(Self::default_commands())
    }

    /// Create a new command palette with custom commands
    pub fn with_commands(commands: Vec<Command>) -> Self {
        Self {
            state: CommandPaletteState::new(commands),
        }
    }

    /// Get default built-in commands
    ///
    /// These match the registered slash commands in `REGISTERED_SLASH_COMMANDS`.
    /// The palette inserts the command name into the input field; actual execution
    /// happens through the normal slash command dispatch path.
    fn default_commands() -> Vec<Command> {
        vec![
            // ── Conversation ──────────────────────────────────────
            Command::new("/clear", "Clear conversation and reset session", || {
                CommandResult::Close
            }),
            Command::new("/save", "Save current conversation", || {
                CommandResult::Close
            }),
            Command::with_hint("/load", "Load a saved conversation", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint("/rename", "Rename the current session", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/compact",
                "Summarize and compress conversation history",
                "[preview|threshold]",
                || CommandResult::Close,
            ),
            Command::new("/cost", "Show session token usage and cost", || {
                CommandResult::Close
            }),
            Command::new("/regenerate", "Regenerate the last AI response", || {
                CommandResult::Close
            }),
            // ── Files ──────────────────────────────────────────────
            Command::new("/undo", "Undo the last file write operation", || {
                CommandResult::Close
            }),
            Command::new("/diff", "Show git diff of recent changes", || {
                CommandResult::Close
            }),
            Command::new("/export", "Export conversation to markdown file", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/workspace",
                "Rescan workspace context",
                "[rescan|reload]",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/extract",
                "Extract tasks/todos from text",
                "<text>",
                || CommandResult::Close,
            ),
            // ── Agents & Teams ─────────────────────────────────────
            Command::with_hint(
                "/agent",
                "Manage AI agents",
                "list | spawn <role> <task> | cancel <id>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/team",
                "Start or manage a team task",
                "<task description>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/plan",
                "Enter plan mode for structured planning",
                "[task]",
                || CommandResult::Close,
            ),
            // ── AI Configuration ───────────────────────────────────
            Command::with_hint("/model", "Switch LLM model", "<model-name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/provider",
                "Switch LLM provider",
                "anthropic|openai|ollama|local",
                || CommandResult::Close,
            ),
            Command::new("/theme", "Cycle through color themes", || {
                CommandResult::Close
            }),
            // ── Memory & Knowledge ─────────────────────────────────
            Command::with_hint(
                "/memory",
                "Manage persistent memory",
                "save|recall|search|list|delete",
                || CommandResult::Close,
            ),
            Command::with_hint("/review", "Analyze code for issues", "[path]", || {
                CommandResult::Close
            }),
            Command::new("/learnings", "Show accumulated learnings", || {
                CommandResult::Close
            }),
            // ── Tasks & Todos ──────────────────────────────────────
            Command::with_hint("/task", "Manage tasks", "create|list|status|done", || {
                CommandResult::Close
            }),
            Command::with_hint("/todo", "Manage todos", "add|list|done", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/track",
                "Show workspace progress",
                "full|detail|tasks|todos",
                || CommandResult::Close,
            ),
            // ── Skills & Extensions ────────────────────────────────
            Command::with_hint(
                "/skill",
                "Manage skills",
                "list|install|activate|run|reload",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/marketplace",
                "Browse skill marketplace",
                "list|search|install",
                || CommandResult::Close,
            ),
            Command::with_hint("/mcp", "Manage MCP servers", "list|status", || {
                CommandResult::Close
            }),
            Command::with_hint("/hook", "Manage hooks", "list|status", || {
                CommandResult::Close
            }),
            // ── Autonomous Development ─────────────────────────────
            Command::with_hint(
                "/orchestra",
                "Orchestra project management",
                "progress|state|health|plan|execute",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/workers",
                "Manage background workers",
                "list|status|cancel",
                || CommandResult::Close,
            ),
            Command::with_hint("/cron", "Manage scheduled jobs", "list|add|remove", || {
                CommandResult::Close
            }),
            // ── Misc ───────────────────────────────────────────────
            Command::new("/help", "Show keyboard shortcuts and help", || {
                CommandResult::Close
            }),
            Command::new("/copilot-login", "Sign in to GitHub Copilot", || {
                CommandResult::Close
            }),
            Command::new("/quit", "Exit the TUI (Ctrl+D/Ctrl+Q)", || {
                CommandResult::Close
            }),
        ]
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut CommandPaletteState {
        &mut self.state
    }

    /// Get reference to state
    pub fn state(&self) -> &CommandPaletteState {
        &self.state
    }

    /// Show the palette
    pub fn show(&mut self) {
        self.state.show();
    }

    /// Hide the palette
    pub fn hide(&mut self) {
        self.state.hide();
    }

    /// Toggle palette visibility
    pub fn toggle(&mut self) {
        self.state.toggle();
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            // Close palette on Escape
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.hide();
                true
            }

            // Navigate up
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.state.move_up();
                true
            }

            // Navigate down
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.state.move_down();
                true
            }

            // Select command on Enter
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if let Some(command) = self.state.selected_command() {
                    command.execute();
                }
                self.hide();
                true
            }

            // Typing characters
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                true
            }

            // Backspace
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.state.backspace();
                true
            }

            // Clear query on Ctrl+U
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.state.clear_query();
                true
            }

            _ => false,
        }
    }

    /// Take the selected command (removes it from state)
    pub fn take_selected_command(&mut self) -> Option<Command> {
        if let Some(_cmd) = self.state.selected_command() {
            let idx = self.state.filtered_indices[self.state.selected_index];
            Some(self.state.commands[idx].clone())
        } else {
            None
        }
    }

    /// Render the command palette
    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.state.visible {
            return;
        }

        let count = self.state.filtered_indices.len();
        if count == 0 {
            return;
        }

        // Dropdown height: up to 8 items, each 2 lines (name + desc), plus border (2)
        let max_items = 8.min(count);
        let available_height = area.height.saturating_sub(5);
        if available_height < 4 {
            return; // Terminal too small to show palette
        }
        let dropdown_height = (max_items * 2 + 2).min(available_height as usize) as u16;

        // Position: full content width, anchored above input area
        // Input is at bottom: input(3) + search(0/1) + status(1) = 4-5 rows from bottom
        let dropdown_width = (area.width * 70) / 100;
        let x = area.x + (area.width - dropdown_width) / 2;
        let y = area.y + area.height.saturating_sub(dropdown_height + 5);
        let dropdown_area = Rect::new(x, y, dropdown_width, dropdown_height);

        f.render_widget(Clear, dropdown_area);
        self.render_command_list(f, dropdown_area);
    }

    /// Render the filtered command list
    fn render_command_list(&self, f: &mut Frame, area: Rect) {
        if self.state.filtered_indices.is_empty() {
            // Show "no results" message
            let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                "No commands found",
                Style::default().fg(Color::DarkGray),
            )]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

            f.render_widget(paragraph, area);
            return;
        }

        // Create list items
        let items: Vec<ListItem> = self
            .state
            .filtered_indices
            .iter()
            .map(|&cmd_idx| {
                let command = &self.state.commands[cmd_idx];

                // Highlight matches in name
                let mut name_spans = self
                    .state
                    .matcher
                    .highlight_matches(&command.name, &self.state.query)
                    .spans;

                // Append argument hint in dim text
                if !command.argument_hint.is_empty() {
                    name_spans.push(Span::styled(
                        format!(" {}", command.argument_hint),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                // Create description line
                let desc_span =
                    Span::styled(&command.description, Style::default().fg(Color::DarkGray));

                ListItem::new(vec![Line::from(name_spans), Line::from(desc_span)])
            })
            .collect();

        // Render list
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        // Create list state
        let mut list_state = ListState::default();
        list_state.select(Some(self.state.selected_index));

        f.render_stateful_widget(list, area, &mut list_state);
    }
}

impl Default for CommandPaletteRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// COMMAND PALETTE (HIGH-LEVEL API)
// ============================================================================

/// High-level command palette API
///
/// This combines state and rendering into a single convenient interface.
pub struct CommandPalette {
    /// Renderer with embedded state
    renderer: CommandPaletteRenderer,
}

impl CommandPalette {
    /// Create a new command palette with default commands
    pub fn new() -> Self {
        Self {
            renderer: CommandPaletteRenderer::new(),
        }
    }

    /// Create a new command palette with custom commands
    pub fn with_commands(commands: Vec<Command>) -> Self {
        Self {
            renderer: CommandPaletteRenderer::with_commands(commands),
        }
    }

    /// Check if palette is visible
    pub fn is_visible(&self) -> bool {
        self.renderer.state().visible
    }

    /// Show the palette
    pub fn show(&mut self) {
        self.renderer.show();
    }

    /// Hide the palette
    pub fn hide(&mut self) {
        self.renderer.hide();
    }

    /// Toggle palette visibility
    pub fn toggle(&mut self) {
        self.renderer.toggle();
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.renderer.handle_key(key)
    }

    /// Take the selected command (removes it from state)
    pub fn take_selected(&mut self) -> Option<Command> {
        self.renderer.take_selected_command()
    }

    /// Render the command palette
    pub fn render(&self, f: &mut Frame, area: Rect) {
        self.renderer.render(f, area);
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut CommandPaletteState {
        self.renderer.state_mut()
    }

    /// Get reference to state
    pub fn state(&self) -> &CommandPaletteState {
        self.renderer.state()
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// EXAMPLE USAGE
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_creation() {
        let handler = || CommandResult::Success;
        let cmd = Command::new("test", "Test command", handler);

        assert_eq!(cmd.name, "test");
        assert_eq!(cmd.description, "Test command");
    }

    #[test]
    fn test_fuzzy_matcher() {
        let matcher = FuzzyMatcher::new();
        let cmd = Command::new("help", "Show help dialog", || CommandResult::Success);

        // Exact match
        assert_eq!(matcher.match_score("help", &cmd), MatchScore::Exact);

        // Prefix match
        assert_eq!(matcher.match_score("hel", &cmd), MatchScore::Prefix);

        // Substring match
        assert_eq!(matcher.match_score("el", &cmd), MatchScore::Substring);

        // Description match
        assert_eq!(matcher.match_score("dialog", &cmd), MatchScore::Substring);

        // No match
        assert_eq!(matcher.match_score("xyz", &cmd), MatchScore::None);
    }

    #[test]
    fn test_command_palette_state() {
        let commands = vec![
            Command::new("help", "Show help", || CommandResult::Success),
            Command::new("clear", "Clear history", || CommandResult::Success),
        ];

        let mut state = CommandPaletteState::new(commands);

        assert!(!state.visible);
        assert_eq!(state.filtered_count(), 2);

        state.show();
        assert!(state.visible);

        state.insert_char('h');
        assert_eq!(state.query, "h");
        assert_eq!(state.filtered_count(), 2); // Both "help" (name) and "clear" (description "history") match

        state.backspace();
        assert_eq!(state.query, "");
        assert_eq!(state.filtered_count(), 2);
    }

    #[test]
    fn test_highlight_matches() {
        let matcher = FuzzyMatcher::new();

        // Exact match
        let line = matcher.highlight_matches("help", "help");
        assert!(!line.spans.is_empty());

        // Substring match
        let line = matcher.highlight_matches("el", "help");
        assert!(!line.spans.is_empty());

        // No match
        let line = matcher.highlight_matches("xyz", "help");
        // Should return original text as single span
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_keyboard_navigation() {
        let mut palette = CommandPalette::new();
        palette.show();

        assert_eq!(palette.state().selected_index, 0);

        // Navigate down
        palette.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 1);

        // Navigate up
        palette.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 0);

        // Escape hides palette
        palette.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!palette.is_visible());
    }

    #[test]
    fn test_query_filtering() {
        let mut palette = CommandPalette::new();
        palette.show();

        // Type "he" to filter for "help" and commands with "he" in description
        palette.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        palette.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        assert_eq!(palette.state().query, "he");
        // "he" matches "help" (name) and commands with "he" in their descriptions
        // like "theme" ("Switch between dark and light theme"), "model", "save", "load"
        assert!(palette.state().filtered_count() >= 1);

        // Backspace
        palette.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(palette.state().query, "h");

        // Clear with Ctrl+U
        palette.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(palette.state().query, "");
    }

    #[test]
    fn test_command_execution() {
        let mut palette = CommandPalette::new();
        palette.show();

        // Select first command and execute
        let cmd = palette.take_selected();
        assert!(cmd.is_some());

        if let Some(cmd) = cmd {
            let result = cmd.execute();
            // All palette commands return Close (actual execution goes through slash dispatch)
            assert!(matches!(result, CommandResult::Close));
        }
    }
}
