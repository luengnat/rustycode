//! Wizard Renderer for multi-step tool questionnaires.
//! 
//! Handles rendering progress, navigation, and input for complex questionnaires.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use crate::tool_approval::mod::{QuestionnaireSession, ApprovalType};

pub struct WizardRenderer;

impl WizardRenderer {
    pub fn render(frame: &mut Frame, area: Rect, session: &QuestionnaireSession) {
        let title = format!(" {} (Step {}/{}) ", 
            session.definition.title, 
            session.current_step + 1, 
            session.definition.questions.len()
        );

        let current_question = &session.definition.questions[session.current_step];
        
        let mut content = vec![
            ratatui::text::Line::from(current_question.prompt.clone()),
            ratatui::text::Line::from(""),
        ];

        match &current_question.approval_type {
            ApprovalType::Binary => {
                content.push(ratatui::text::Line::from("[y] Yes  [n] No"));
            }
            ApprovalType::MultipleChoice { options } => {
                for (i, opt) in options.iter().enumerate() {
                    content.push(ratatui::text::Line::from(format!("[{}] {}", i + 1, opt)));
                }
            }
            ApprovalType::TextInput => {
                content.push(ratatui::text::Line::from("Type and press Enter"));
            }
        }

        let paragraph = Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(title));

        frame.render_widget(paragraph, area);
    }
}
