//! Plugin manager UI

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::manager::PluginManager;

/// Plugin manager UI state
pub struct PluginManagerUI {
    /// Whether the UI is visible
    pub visible: bool,

    /// Currently selected plugin index
    pub selected_index: usize,

    /// Scroll offset
    pub scroll_offset: usize,

    /// Current mode (list/details)
    pub mode: PluginManagerMode,
}

/// Plugin manager display mode
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum PluginManagerMode {
    /// List view
    #[default]
    List,

    /// Detail view
    Details,
}

impl PluginManagerUI {
    /// Create new plugin manager UI
    pub fn new() -> Self {
        Self {
            visible: false,
            selected_index: 0,
            scroll_offset: 0,
            mode: PluginManagerMode::List,
        }
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.selected_index = 0;
            self.scroll_offset = 0;
            self.mode = PluginManagerMode::List;
        }
    }

    /// Select previous plugin
    pub fn select_previous(&mut self, plugin_count: usize) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.update_scroll_offset(plugin_count);
        }
    }

    /// Select next plugin
    pub fn select_next(&mut self, plugin_count: usize) {
        if self.selected_index < plugin_count.saturating_sub(1) {
            self.selected_index += 1;
            self.update_scroll_offset(plugin_count);
        }
    }

    /// Update scroll offset
    fn update_scroll_offset(&mut self, plugin_count: usize) {
        const MAX_VISIBLE: usize = 10;

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + MAX_VISIBLE {
            self.scroll_offset = self.selected_index - MAX_VISIBLE + 1;
        }

        // Don't scroll past end
        if self.scroll_offset > plugin_count.saturating_sub(MAX_VISIBLE) {
            self.scroll_offset = plugin_count.saturating_sub(MAX_VISIBLE);
        }
    }

    /// Render the plugin manager UI
    pub fn render(&self, frame: &mut Frame, area: Rect, manager: &PluginManager) {
        let plugins = manager.get_plugins();

        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
            .split(area);

        // Render main content
        match self.mode {
            PluginManagerMode::List => self.render_list(frame, chunks[0], &plugins),
            PluginManagerMode::Details => self.render_details(frame, chunks[0], &plugins),
        }

        // Render help footer
        self.render_footer(frame, chunks[1]);
    }

    /// Render plugin list
    fn render_list(&self, frame: &mut Frame, area: Rect, plugins: &[&super::manager::Plugin]) {
        let title = "Plugin Manager";

        // Create list items
        let items: Vec<ListItem> = plugins
            .iter()
            .enumerate()
            .map(|(i, plugin)| {
                let is_selected = i == self.selected_index;
                let status = if plugin.enabled {
                    "enabled"
                } else {
                    "disabled"
                };

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let content = format!("{} ({})", plugin.manifest.name, status);

                ListItem::new(content).style(style)
            })
            .collect();

        // Create list widget
        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(list, area);
    }

    /// Render plugin details
    fn render_details(&self, frame: &mut Frame, area: Rect, plugins: &[&super::manager::Plugin]) {
        let plugin = match plugins.get(self.selected_index) {
            Some(p) => p,
            None => return,
        };

        let title = format!("Plugin: {}", plugin.manifest.name);

        // Create detail text
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.name),
            ]),
            Line::from(vec![
                Span::styled("Version: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.version),
            ]),
            Line::from(vec![
                Span::styled("Author: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.author),
            ]),
            Line::from(vec![
                Span::styled("Description: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.description),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    if plugin.enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    },
                    Style::default().fg(if plugin.enabled {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Permissions:",
                Style::default().fg(Color::Cyan),
            )]),
        ];

        for perm in plugin.permissions.describe() {
            lines.push(Line::from(vec![Span::raw("  • "), Span::raw(perm)]));
        }

        if !plugin.manifest.slash_commands.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Commands:",
                Style::default().fg(Color::Cyan),
            )]));

            for cmd in &plugin.manifest.slash_commands {
                lines.push(Line::from(vec![
                    Span::raw("  • /"),
                    Span::styled(&cmd.name, Style::default().fg(Color::Yellow)),
                    Span::raw(": "),
                    Span::raw(&cmd.description),
                ]));
            }
        }

        // Create paragraph widget
        let paragraph = Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset as u16, 0));

        frame.render_widget(paragraph, area);
    }

    /// Render footer with key bindings
    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = match self.mode {
            PluginManagerMode::List => "↑↓:Select  Enter:Details  e:Enable  d:Disable  Esc:Close",
            PluginManagerMode::Details => "Esc:Back  e:Enable  d:Disable",
        };

        let paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));

        frame.render_widget(paragraph, area);
    }
}

impl Default for PluginManagerUI {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_ui_new() {
        let ui = PluginManagerUI::new();
        assert!(!ui.visible);
        assert_eq!(ui.selected_index, 0);
        assert_eq!(ui.mode, PluginManagerMode::List);
    }

    #[test]
    fn test_plugin_manager_ui_toggle() {
        let mut ui = PluginManagerUI::new();

        ui.toggle();
        assert!(ui.visible);

        ui.toggle();
        assert!(!ui.visible);
    }

    #[test]
    fn test_select_next_previous() {
        let mut ui = PluginManagerUI::new();

        ui.select_next(10);
        assert_eq!(ui.selected_index, 1);

        ui.select_previous(10);
        assert_eq!(ui.selected_index, 0);

        // Can't go below 0
        ui.select_previous(10);
        assert_eq!(ui.selected_index, 0);
    }
}
