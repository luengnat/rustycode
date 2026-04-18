//! Message rendering coordinator
//!
//! This module coordinates the rendering of messages to the terminal UI,
//! delegating specialized rendering tasks to dedicated submodules.

use super::animator::AnimationFrame;
use super::message_image::{
    calculate_image_height, images_per_row, render_image_header, render_single_image_preview,
};
use super::message_thinking::{render_thinking_content, render_thinking_header};
use super::message_types::{ExpansionLevel, Message, ToolExecution, ToolStatus};
use super::spinner::Spinner;
use anyhow::Result as anyhowResult;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    Frame,
};
pub use rustycode_ui_core::{MarkdownRenderer, MessageTheme};

// MessageTheme removed as it is now defined in ui::markdown

// ============================================================================
// MESSAGE RENDERER
// ============================================================================

/// Message renderer - handles hierarchical display
pub struct MessageRenderer {
    /// Whether to show thinking globally
    pub show_thinking: bool,
    /// Whether to show tools globally
    pub show_tools: bool,
    /// Current animation frame for spinners
    pub anim_frame: AnimationFrame,
    /// Cache of rendered markdown lines (content_hash -> lines)
    /// Use RwLock for interior mutability since render methods take &self
    render_cache: std::sync::RwLock<std::collections::HashMap<u64, Vec<Line<'static>>>>,
}

impl Default for MessageRenderer {
    fn default() -> Self {
        Self {
            show_thinking: false,
            show_tools: true,
            anim_frame: AnimationFrame::default(),
            render_cache: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl MessageRenderer {
    /// Create a new message renderer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new message renderer with animation frame
    pub fn with_animation(anim_frame: AnimationFrame) -> Self {
        Self {
            show_thinking: false,
            show_tools: true,
            anim_frame,
            render_cache: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Invalidate the render cache (call on terminal resize since
    /// line wrapping depends on terminal width).
    pub fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.render_cache.write() {
            cache.clear();
        }
    }

    /// Update the animation frame
    pub fn update_animation(&mut self, frame: AnimationFrame) {
        self.anim_frame = frame;
    }

    /// Render a message
    pub fn render_message(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        let (pipe_char, pipe_color) = message.pipe_style();

        // Calculate image height for positioning
        let image_height = if message.has_images() {
            calculate_image_height(true, message.image_count())
        } else {
            0
        };

        let mut current_y = area.y;

        // Render images first (at the top)
        if message.has_images() && current_y < area.bottom() {
            let image_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: image_height as u16,
            };
            self.render_image_previews(f, image_area, message, pipe_char, pipe_color)?;
            current_y += image_height as u16;
        }

        // Render main message content
        let content_area = Rect {
            x: area.x,
            y: current_y,
            width: area.width,
            height: (area.bottom() - current_y).max(1),
        };
        self.render_content(f, content_area, message, pipe_char, pipe_color, theme)?;

        // Update current_y after content
        current_y += self.calculate_content_height(message, area.width as usize) as u16;

        // If assistant message has tools, render inline summary
        if message.has_tools() && self.show_tools && current_y < area.bottom() {
            let tool_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: (area.bottom() - current_y).min(10), // Max 10 lines for tools
            };
            self.render_tool_summary(f, tool_area, message, pipe_char, pipe_color, theme)?;
            current_y += self.calculate_tool_height(message) as u16;
        }

        // If thinking is present and user wants to see it
        if message.has_thinking() && self.show_thinking && current_y < area.bottom() {
            let thinking_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: (area.bottom() - current_y).min(10),
            };
            self.render_thinking(f, thinking_area, message, pipe_char, pipe_color, theme)?;
        }

        Ok(())
    }

    /// Render image previews
    fn render_image_previews(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
    ) -> anyhowResult<()> {
        let images = message
            .metadata
            .images
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No images found in message metadata"))?;

        // Render header
        render_image_header(f, area, images.len(), pipe, color)?;

        // Render up to 3 images per row
        let images_per_row = images_per_row(area.width);

        for (i, img) in images.iter().enumerate() {
            let row = i / images_per_row;
            let col = i % images_per_row;

            let img_width = area.width / images_per_row as u16;
            let img_x = area.x + (col as u16 * img_width);
            let img_y = area.y + 1 + (row as u16 * 8); // 8 lines per image

            if img_y + 8 > area.bottom() {
                break; // No more space
            }

            let img_area = Rect {
                x: img_x,
                y: img_y,
                width: img_width,
                height: 8,
            };

            render_single_image_preview(f, img_area, img, pipe, color)?;
        }

        Ok(())
    }

    /// Render the main message content
    fn render_content(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        _color: Color, // Keep for signature compatibility, but use theme instead
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        // Use themed colors for pipe
        let pipe_color = match message.role {
            crate::ui::message::MessageRole::User => theme.user_color,
            crate::ui::message::MessageRole::Assistant => theme.ai_color,
            crate::ui::message::MessageRole::System => theme.system_color,
        };

        // Check if message is collapsed
        if message.collapsed {
            // Show collapsed state: just first line or summary
            let first_line = message.content.lines().next().unwrap_or("");
            let preview = if first_line.chars().count() > 60 {
                let s: String = first_line.chars().take(57).collect();
                format!("{}...", s)
            } else {
                first_line.to_string()
            };

            let all_lines = vec![Line::from(vec![
                Span::styled(format!("{} ", pipe), Style::default().fg(pipe_color)),
                Span::styled(preview, Style::default().fg(Color::DarkGray)),
                Span::styled(" (collapsed)", Style::default().fg(Color::DarkGray)),
            ])];

            let paragraph = ratatui::widgets::Paragraph::new(all_lines)
                .style(theme.default_style)
                .wrap(ratatui::widgets::Wrap { trim: false });

            f.render_widget(paragraph, area);
            return Ok(());
        }

        // Parse and render markdown content with syntax highlighting
        let rendered_lines =
            MarkdownRenderer::render_content(&message.content, theme, Some(&self.render_cache));

        // Use vertical border instead of inline bar to avoid copying bar when selecting text
        // This leaves the left column (1 char) for the border, giving more text space
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::LEFT)
            .border_style(ratatui::style::Style::default().fg(pipe_color));

        let paragraph = ratatui::widgets::Paragraph::new(rendered_lines)
            .style(theme.default_style)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(block);

        f.render_widget(paragraph, area);

        Ok(())
    }

    /// Render tool summary
    fn render_tool_summary(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        let tools = message
            .tool_executions
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No tool executions found in message"))?;
        let total = tools.len();
        let complete = message.completed_tool_count();
        let failed = message.failed_tool_count();
        let total_bytes = message.total_tool_output_size();

        // Format size
        let size_str = if total_bytes < 1024 {
            format!("{}b", total_bytes)
        } else if total_bytes < 1024 * 1024 {
            format!("{:.1}kb", total_bytes as f64 / 1024.0)
        } else {
            format!("{:.1}mb", total_bytes as f64 / (1024.0 * 1024.0))
        };

        // Build header line
        let header_text = format!(
            "🔧 Executed: {} {} {}{} [▾] {}",
            total,
            if total == 1 { "tool" } else { "tools" },
            if failed > 0 {
                format!("({} failed)", failed)
            } else {
                String::new()
            },
            if complete == total && total > 0 {
                " ✅".to_string()
            } else if complete > 0 {
                format!(" ({}/{})", complete, total)
            } else {
                String::new()
            },
            size_str
        );

        let header_line = Line::from(vec![
            Span::styled(format!("{} ", pipe), Style::default().fg(color)),
            Span::styled(header_text, Style::default().fg(theme.tool_summary_color)),
        ]);

        let paragraph = ratatui::widgets::Paragraph::new(header_line);
        f.render_widget(paragraph, area);

        // If expanded, render tool list
        if message.tools_expansion != ExpansionLevel::Collapsed && area.height > 2 {
            self.render_tool_list(f, area, message, pipe, color, theme)?;
        }

        Ok(())
    }

    /// Render expanded tool list
    fn render_tool_list(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        let tools = message
            .tool_executions
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No tool executions found in message"))?;
        let mut lines = vec![];

        // Add border line
        lines.push(Line::from(vec![
            Span::styled(format!("{} ┌", pipe), Style::default().fg(color)),
            Span::styled(
                "─".repeat(area.width.saturating_sub(4) as usize),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("┐", Style::default().fg(Color::DarkGray)),
        ]));

        // Add each tool
        for (i, tool) in tools.iter().enumerate() {
            let is_focused = message.focused_tool_index == Some(i);
            let tool_color = tool.status.color();

            let tool_line = if message.tools_expansion == ExpansionLevel::Deep && is_focused {
                // Deep expansion - show detailed output
                self.render_tool_details(tool, pipe, color, theme)?
            } else {
                // Normal expansion - show summary with animated spinner for running tools
                let status_icon = if tool.status == ToolStatus::Running {
                    // Use animated spinner for running tools
                    let spinner = Spinner::working(&tool.name);
                    spinner.render_char(&self.anim_frame).content.to_string()
                } else {
                    tool.status.icon().to_string()
                };

                Line::from(vec![
                    Span::styled(format!("{} │ ", pipe), Style::default().fg(color)),
                    Span::styled(status_icon, Style::default().fg(tool_color)),
                    Span::styled(
                        format!(" [{}] {} ", i + 1, tool.result_summary),
                        Style::default().fg(theme.tool_text_color),
                    ),
                    Span::styled(
                        tool.size_summary().to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            };

            lines.push(tool_line);

            // Add separator line (except for last tool)
            if i < tools.len() - 1 {
                lines.push(Line::from(vec![Span::styled(
                    format!("{} │", pipe),
                    Style::default().fg(color),
                )]));
            }
        }

        // Add border line
        lines.push(Line::from(vec![
            Span::styled(format!("{} └", pipe), Style::default().fg(color)),
            Span::styled(
                "─".repeat(area.width.saturating_sub(4) as usize),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("┘", Style::default().fg(Color::DarkGray)),
        ]));

        let paragraph = ratatui::widgets::Paragraph::new(lines);
        f.render_widget(paragraph, area);

        Ok(())
    }

    /// Render detailed tool output
    fn render_tool_details(
        &self,
        tool: &ToolExecution,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
    ) -> anyhowResult<Line<'_>> {
        let no_output = "(no output)".to_string();
        let detailed_output = tool.detailed_output.as_ref().unwrap_or(&no_output);

        // Truncate if too long
        let max_len = 200;
        let output = if detailed_output.len() > max_len {
            let end = detailed_output.floor_char_boundary(max_len);
            format!("{}...", &detailed_output[..end])
        } else {
            detailed_output.clone()
        };

        Ok(Line::from(vec![
            Span::styled(format!("{} │ ┌", pipe), Style::default().fg(color)),
            Span::styled(
                format!(" {} ", output),
                Style::default().fg(theme.tool_detail_color),
            ),
            Span::styled("┐", Style::default().fg(Color::DarkGray)),
        ]))
    }

    /// Render thinking block
    fn render_thinking(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        let thinking = message
            .thinking
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No thinking content found in message"))?;

        // Render header
        render_thinking_header(f, area, thinking, pipe, color, theme)?;

        // If expanded, render thinking content
        if message.thinking_expansion != ExpansionLevel::Collapsed {
            render_thinking_content(f, area, thinking, pipe, color, theme)?;
        }

        Ok(())
    }

    /// Calculate content height accounting for wrapping
    fn calculate_content_height(&self, message: &Message, content_width: usize) -> usize {
        // Count lines, accounting for wrapped lines at actual content width
        let line_count = message.content.lines().count();

        // Estimate wrapped lines based on actual content width
        let width = content_width.max(1);
        let estimated_wraps = message
            .content
            .lines()
            .filter(|line| line.len() > width)
            .map(|line| line.len() / width)
            .sum::<usize>();

        let total_lines = line_count + estimated_wraps;

        // Add 1 for header (if message has one)
        total_lines
            + if message.role == crate::ui::message::MessageRole::System {
                0
            } else {
                1
            }
    }

    /// Calculate tool area height
    fn calculate_tool_height(&self, message: &Message) -> usize {
        if !message.has_tools() {
            return 0;
        }

        match message.tools_expansion {
            ExpansionLevel::Collapsed => 1, // Just header
            ExpansionLevel::Expanded => {
                // Header + border + tools + border
                2 + message.tool_count() + 1
            }
            ExpansionLevel::Deep => {
                // Header + border + tools (with detail) + border
                // Deep expansion shows detail for one tool
                2 + message.tool_count() + 4 + 1
            }
        }
    }

    /// Render plain text content without markdown parsing (for streaming)
    pub fn render_plain_text(&self, content: &str, theme: &MessageTheme) -> Vec<Line<'static>> {
        MarkdownRenderer::render_plain_text(content, theme)
    }

    /// Render markdown content with syntax highlighting and diff support
    /// Uses caching to avoid re-parsing the same content multiple times.
    pub fn render_markdown_content(
        &self,
        content: &str,
        theme: &MessageTheme,
    ) -> Vec<Line<'static>> {
        MarkdownRenderer::render_content(content, theme, Some(&self.render_cache))
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::message_types::MessageRole;
    use super::*;

    #[test]
    fn test_message_renderer_default() {
        let renderer = MessageRenderer::default();
        assert!(!renderer.show_thinking);
        assert!(renderer.show_tools);
    }

    #[test]
    fn test_message_renderer_new() {
        let renderer = MessageRenderer::new();
        assert!(!renderer.show_thinking);
        assert!(renderer.show_tools);
    }

    #[test]
    fn test_message_renderer_with_animation() {
        let anim_frame = AnimationFrame::default();
        let renderer = MessageRenderer::with_animation(anim_frame);
        assert!(!renderer.show_thinking);
        assert!(renderer.show_tools);
    }

    #[test]
    fn test_message_renderer_update_animation() {
        let mut renderer = MessageRenderer::new();
        let anim_frame = AnimationFrame::default();
        renderer.update_animation(anim_frame);
        // Animation frame updated
    }

    #[test]
    fn test_message_theme_default() {
        let theme = MessageTheme::default();
        // Default colors are mauve/purple for user, teal for AI, yellow for system (midnight-rust theme)
        assert_eq!(theme.user_color, Color::Rgb(180, 142, 173));
        assert_eq!(theme.ai_color, Color::Rgb(143, 188, 187));
        assert_eq!(theme.system_color, Color::Rgb(235, 203, 139));
    }

    #[test]
    fn test_calculate_content_height() {
        let renderer = MessageRenderer::new();
        let message = Message::new(MessageRole::Assistant, "Line 1\nLine 2\nLine 3".to_string());
        let height = renderer.calculate_content_height(&message, 80);
        assert!(height >= 4); // 3 lines + 1 for header
    }

    #[test]
    fn test_calculate_content_height_empty() {
        let renderer = MessageRenderer::new();
        let message = Message::new(MessageRole::Assistant, String::new());
        let height = renderer.calculate_content_height(&message, 80);
        assert!(height >= 1); // At least header
    }

    #[test]
    fn test_calculate_tool_height_no_tools() {
        let renderer = MessageRenderer::new();
        let message = Message::new(MessageRole::Assistant, String::new());
        let height = renderer.calculate_tool_height(&message);
        assert_eq!(height, 0);
    }

    #[test]
    fn test_calculate_tool_height_collapsed() {
        let renderer = MessageRenderer::new();
        let tool = ToolExecution::new("tool_1".to_string(), "test".to_string(), "Done".to_string());
        let mut message = Message::new(MessageRole::Assistant, String::new());
        message.tool_executions = Some(vec![tool]);
        message.tools_expansion = ExpansionLevel::Collapsed;
        let height = renderer.calculate_tool_height(&message);
        assert_eq!(height, 1);
    }

    #[test]
    fn test_calculate_tool_height_expanded() {
        let renderer = MessageRenderer::new();
        let tool1 = ToolExecution::new(
            "tool_1".to_string(),
            "test1".to_string(),
            "Done1".to_string(),
        );
        let tool2 = ToolExecution::new(
            "tool_2".to_string(),
            "test2".to_string(),
            "Done2".to_string(),
        );
        let mut message = Message::new(MessageRole::Assistant, String::new());
        message.tool_executions = Some(vec![tool1, tool2]);
        message.tools_expansion = ExpansionLevel::Expanded;
        let height = renderer.calculate_tool_height(&message);
        // Header + border + 2 tools + border
        assert_eq!(height, 5);
    }

    #[test]
    fn test_calculate_tool_height_deep() {
        let renderer = MessageRenderer::new();
        let tool = ToolExecution::new("tool_1".to_string(), "test".to_string(), "Done".to_string());
        let mut message = Message::new(MessageRole::Assistant, String::new());
        message.tool_executions = Some(vec![tool]);
        message.tools_expansion = ExpansionLevel::Deep;
        let height = renderer.calculate_tool_height(&message);
        // Header + border + tool + detail + border
        assert!(height >= 7);
    }

    #[test]
    fn test_render_plain_text() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let lines = renderer.render_plain_text("Hello, world!", &theme);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_markdown_content() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let lines = renderer.render_markdown_content("# Test", &theme);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_markdown_content_cache() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let content = "# Test Heading\n\nSome content";

        // First call - cache miss
        let lines1 = renderer.render_markdown_content(content, &theme);
        // Second call - cache hit
        let lines2 = renderer.render_markdown_content(content, &theme);

        assert_eq!(lines1.len(), lines2.len());
    }
}
