//! Tool Approval System
//!
//! Risk-based tool classification and approval UI for safe tool execution.

use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub mod risk;

/// Tool approval state
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApprovalState {
    /// Tool has not been approved yet
    Pending,
    /// Tool approved for execution
    Approved,
    /// Tool rejected by user
    Rejected,
    /// Tool approved for all future uses (session)
    ApprovedAll,
    /// Tool rejected and blocked for rest of session
    RejectedAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalType {
    Binary,
    MultipleChoice { options: Vec<String> },
    TextInput,
}

#[derive(Debug, Clone)]
pub struct Question {
    pub prompt: String,
    pub approval_type: ApprovalType,
}

#[derive(Debug, Clone)]
pub struct QuestionnaireDefinition {
    pub title: String,
    pub questions: Vec<Question>,
}

impl QuestionnaireSession {
    pub fn answer(&mut self, ans: String) -> bool {
        self.answers.push(ans);
        self.current_step += 1;
        self.current_step >= self.definition.questions.len()
    }
}

/// Tool approval request
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub tool_type: risk::ToolType,
    pub risk_level: risk::RiskLevel,
    pub description: String,
    pub command: String,
    pub state: ApprovalState,
    pub approval_type: ApprovalType,
    pub questionnaire: Option<QuestionnaireSession>,
}

impl ApprovalRequest {
    pub fn new(
        tool_name: String,
        tool_type: risk::ToolType,
        description: String,
        command: String,
        approval_type: ApprovalType,
    ) -> Self {
        let risk_level = risk::classify_tool_risk(&tool_type, &command);

        Self {
            tool_name,
            tool_type,
            risk_level,
            description,
            command,
            state: ApprovalState::Pending,
            approval_type,
        }
    }

    pub fn approve(&mut self) {
        self.state = ApprovalState::Approved;
    }

    pub fn reject(&mut self) {
        self.state = ApprovalState::Rejected;
    }

    pub fn reject_all(&mut self) {
        self.state = ApprovalState::RejectedAll;
    }

    pub fn approve_all(&mut self) {
        self.state = ApprovalState::ApprovedAll;
    }

    pub fn is_approved(&self) -> bool {
        matches!(
            self.state,
            ApprovalState::Approved | ApprovalState::ApprovedAll
        )
    }
}

/// Tool approval manager
pub struct ToolApprovalManager {
    pub session_approvals: Vec<(String, ApprovalState)>,
    pub auto_approve_safe: bool,
}

impl ToolApprovalManager {
    pub fn new() -> Self {
        Self {
            session_approvals: Vec::new(),
            auto_approve_safe: true, // Auto-approve safe tools
        }
    }

    /// Check if a tool requires approval based on session state
    pub fn requires_approval(&self, tool_name: &str, risk_level: risk::RiskLevel) -> bool {
        // Check if we've already approved this tool in the session
        if let Some((_, state)) = self
            .session_approvals
            .iter()
            .find(|(name, _)| name == tool_name)
        {
            return !matches!(state, ApprovalState::Approved | ApprovalState::ApprovedAll);
        }

        // Auto-approve safe tools if enabled
        if self.auto_approve_safe && matches!(risk_level, risk::RiskLevel::Safe) {
            return false;
        }

        true
    }

    /// Record approval decision for session
    pub fn record_approval(&mut self, tool_name: String, state: ApprovalState) {
        // Remove existing approval for this tool if any
        self.session_approvals
            .retain(|(name, _)| name != &tool_name);

        // Add new approval
        self.session_approvals.push((tool_name, state));
    }

    /// Get approval state for a tool
    pub fn get_approval_state(&self, tool_name: &str) -> Option<&ApprovalState> {
        self.session_approvals
            .iter()
            .find(|(name, _)| name == tool_name)
            .map(|(_, state)| state)
    }

    /// Check if a tool has been blocked for the rest of the session
    pub fn is_blocked(&self, tool_name: &str) -> bool {
        self.session_approvals
            .iter()
            .any(|(name, state)| name == tool_name && matches!(state, ApprovalState::RejectedAll))
    }
}

impl Default for ToolApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Render tool approval UI
pub fn render_approval_prompt(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    request: &ApprovalRequest,
) {
    // Calculate risk color
    let risk_color = match request.risk_level {
        risk::RiskLevel::Safe => Color::Green,
        risk::RiskLevel::Medium => Color::Yellow,
        risk::RiskLevel::High => Color::Rgb(255, 165, 0), // Orange
        risk::RiskLevel::Dangerous => Color::Red,
    };

    let risk_label = match request.risk_level {
        risk::RiskLevel::Safe => "safe",
        risk::RiskLevel::Medium => "medium",
        risk::RiskLevel::High => "high",
        risk::RiskLevel::Dangerous => "dangerous",
    };

    let title = format!(" Tool Approval: {} ", request.tool_name);

    // Truncate long commands for display
    let cmd_display = if request.command.len() > 80 {
        let truncated: String = request.command.chars().take(77).collect();
        format!("{}...", truncated)
    } else {
        request.command.clone()
    };

    let risk_guidance = match request.risk_level {
        risk::RiskLevel::Safe => "This tool is safe and will be auto-approved.",
        risk::RiskLevel::Medium => "This tool will modify files. Review carefully.",
        risk::RiskLevel::High => "This tool executes system commands. Monitor closely.",
        risk::RiskLevel::Dangerous => "This tool is destructive! Use with extreme caution.",
    };

    let mut content = vec![
        Line::from(vec![
            Span::styled("Risk: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                risk_label,
                Style::default()
                    .fg(risk_color)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(risk_guidance, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            &request.description,
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(risk_color)),
            Span::styled(&cmd_display, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
    ];

    match &request.approval_type {
        ApprovalType::Binary => {
            content.push(Line::from(vec![
                Span::styled("[y] ", Style::default().fg(Color::Green)),
                Span::styled("Yes  ", Style::default().fg(Color::White)),
                Span::styled("[n] ", Style::default().fg(Color::Red)),
                Span::styled("No  ", Style::default().fg(Color::White)),
                Span::styled("[a] ", Style::default().fg(Color::Cyan)),
                Span::styled("Always  ", Style::default().fg(Color::White)),
                Span::styled("[N] ", Style::default().fg(Color::Rgb(255, 100, 100))),
                Span::styled("Block  ", Style::default().fg(Color::White)),
                Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
                Span::styled("Cancel", Style::default().fg(Color::DarkGray)),
            ]));
        }
        ApprovalType::MultipleChoice { options } => {
            for (i, opt) in options.iter().enumerate() {
                content.push(Line::from(format!("[{}] {}", i + 1, opt)));
            }
            content.push(Line::from("[Esc] Cancel"));
        }
        ApprovalType::TextInput => {
            content.push(Line::from("Type input and press Enter. [Esc] Cancel"));
        }
    }

    let paragraph = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(risk_color))
                .style(Style::default().bg(ratatui::style::Color::Rgb(20, 20, 25)))
                .padding(ratatui::widgets::Padding::uniform(1)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(paragraph, area);
}
