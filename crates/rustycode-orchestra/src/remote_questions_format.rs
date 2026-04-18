//! Remote Questions — payload formatting and parsing helpers.
//!
//! Provides formatting and parsing utilities for remote question platforms
//! including Slack, Discord, and Telegram.
//!
//! Matches orchestra-2's remote-questions/format.ts implementation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Slack block element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<SlackText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements: Option<Vec<SlackElement>>,
}

/// Slack text object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackText {
    #[serde(rename = "type")]
    pub text_type: String,
    pub text: String,
}

/// Slack element object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackElement {
    #[serde(rename = "type")]
    pub element_type: String,
    pub text: String,
}

/// Discord embed object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordEmbed {
    pub title: String,
    pub description: String,
    pub color: u32,
    pub fields: Vec<DiscordField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<DiscordFooter>,
}

/// Discord field object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordField {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline: Option<bool>,
}

/// Discord footer object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordFooter {
    pub text: String,
}

/// Discord formatted response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordFormattedResponse {
    pub embeds: Vec<DiscordEmbed>,
    #[serde(rename = "reactionEmojis")]
    pub reaction_emojis: Vec<String>,
}

/// Telegram inline button
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramInlineButton {
    pub text: String,
    #[serde(rename = "callback_data")]
    pub callback_data: String,
}

/// Telegram inline keyboard markup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramInlineKeyboardMarkup {
    #[serde(rename = "inline_keyboard")]
    pub inline_keyboard: Vec<Vec<TelegramInlineButton>>,
}

/// Telegram message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramMessage {
    pub text: String,
    #[serde(rename = "parse_mode")]
    pub parse_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "reply_markup")]
    pub reply_markup: Option<TelegramInlineKeyboardMarkup>,
}

/// Remote prompt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemotePrompt {
    pub id: String,
    pub questions: Vec<RemoteQuestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<RemoteContext>,
}

/// Remote question
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<RemoteOption>,
    #[serde(rename = "allowMultiple")]
    pub allow_multiple: bool,
}

/// Remote option
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteOption {
    pub label: String,
    pub description: String,
}

/// Remote context
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Remote answer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAnswer {
    pub answers: HashMap<String, RemoteAnswerValue>,
}

/// Remote answer value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAnswerValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "user_note")]
    pub user_note: Option<String>,
}

// ─── Constants ───────────────────────────────────────────────────────────────

/// Discord number emojis
pub const DISCORD_NUMBER_EMOJIS: &[&str] = &["1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣"];

/// Slack number reaction names
pub const SLACK_NUMBER_REACTION_NAMES: &[&str] = &["one", "two", "three", "four", "five"];

/// Maximum user note length
const MAX_USER_NOTE_LENGTH: usize = 500;

// ─── Slack Formatting ─────────────────────────────────────────────────────────

/// Format a prompt for Slack
pub fn format_for_slack(prompt: &RemotePrompt) -> Vec<SlackBlock> {
    let mut blocks = Vec::new();

    // Header
    blocks.push(SlackBlock {
        block_type: "header".to_string(),
        text: Some(SlackText {
            text_type: "plain_text".to_string(),
            text: "Orchestra needs your input".to_string(),
        }),
        elements: None,
    });

    // Instructions for multiple questions
    if prompt.questions.len() > 1 {
        blocks.push(SlackBlock {
            block_type: "context".to_string(),
            text: None,
            elements: Some(vec![SlackElement {
                element_type: "mrkdwn".to_string(),
                text: "Reply once in thread using one line per question or semicolons (`1; 2; custom note`).".to_string(),
            }]),
        });
    }

    // Questions
    for q in &prompt.questions {
        let supports_reactions = prompt.questions.len() == 1;

        // Question header and text
        blocks.push(SlackBlock {
            block_type: "section".to_string(),
            text: Some(SlackText {
                text_type: "mrkdwn".to_string(),
                text: format!("*{}*\n{}", q.header, q.question),
            }),
            elements: None,
        });

        // Options
        let options_text: Vec<String> = q
            .options
            .iter()
            .enumerate()
            .map(|(i, opt)| format!("{}. *{}* — {}", i + 1, opt.label, opt.description))
            .collect();
        blocks.push(SlackBlock {
            block_type: "section".to_string(),
            text: Some(SlackText {
                text_type: "mrkdwn".to_string(),
                text: options_text.join("\n"),
            }),
            elements: None,
        });

        // Instructions
        let instruction_text = if prompt.questions.len() > 1 {
            if q.allow_multiple {
                "For this question, use comma-separated numbers (`1,3`) or free text."
            } else {
                "For this question, use one number (`1`) or free text."
            }
        } else {
            if q.allow_multiple {
                if supports_reactions {
                    "Reply in thread with comma-separated numbers (`1,3`) or react with matching number emoji."
                } else {
                    "Reply in thread with comma-separated numbers (`1,3`) or free text."
                }
            } else {
                if supports_reactions {
                    "Reply in thread with a number (`1`) or react with the matching number emoji."
                } else {
                    "Reply in thread with a number (`1`) or free text."
                }
            }
        };
        blocks.push(SlackBlock {
            block_type: "context".to_string(),
            text: None,
            elements: Some(vec![SlackElement {
                element_type: "mrkdwn".to_string(),
                text: instruction_text.to_string(),
            }]),
        });

        // Divider
        blocks.push(SlackBlock {
            block_type: "divider".to_string(),
            text: None,
            elements: None,
        });
    }

    // Source context
    if let Some(ref context) = prompt.context {
        if let Some(ref source) = context.source {
            blocks.push(SlackBlock {
                block_type: "context".to_string(),
                text: None,
                elements: Some(vec![SlackElement {
                    element_type: "mrkdwn".to_string(),
                    text: format!("Source: `{}`", source),
                }]),
            });
        }
    }

    blocks
}

/// Parse a Slack reply
pub fn parse_slack_reply(text: &str, questions: &[RemoteQuestion]) -> RemoteAnswer {
    let mut answers = HashMap::new();
    let trimmed = text.trim();

    if questions.len() == 1 {
        let answer = parse_answer_for_question(trimmed, &questions[0]);
        answers.insert(questions[0].id.clone(), answer);
        return RemoteAnswer { answers };
    }

    // Split by semicolon or newline
    let parts = if trimmed.contains(';') {
        trimmed
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    } else {
        trimmed
            .split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    };

    for (i, q) in questions.iter().enumerate() {
        let part = parts.get(i).map(|s| s.as_str()).unwrap_or("");
        answers.insert(q.id.clone(), parse_answer_for_question(part, q));
    }

    RemoteAnswer { answers }
}

// ─── Discord Formatting ──────────────────────────────────────────────────────

/// Format a prompt for Discord
pub fn format_for_discord(prompt: &RemotePrompt) -> DiscordFormattedResponse {
    let mut reaction_emojis = Vec::new();
    let embeds: Vec<DiscordEmbed> = prompt
        .questions
        .iter()
        .enumerate()
        .map(|(question_index, q)| {
            let supports_reactions = prompt.questions.len() == 1;

            let option_lines: Vec<String> = q
                .options
                .iter()
                .enumerate()
                .map(|(i, opt)| {
                    let fallback = format!("{}.", i + 1);
                    let emoji = DISCORD_NUMBER_EMOJIS
                        .get(i)
                        .copied()
                        .unwrap_or(fallback.as_str());

                    if supports_reactions {
                        if let Some(&emoji) = DISCORD_NUMBER_EMOJIS.get(i) {
                            reaction_emojis.push(emoji.to_string());
                        }
                    }

                    format!("{} **{}** — {}", emoji, opt.label, opt.description)
                })
                .collect();

            let mut footer_parts = Vec::new();

            if supports_reactions {
                if q.allow_multiple {
                    footer_parts.push(
                        "Reply with comma-separated choices (`1,3`) or react with matching numbers"
                            .to_string(),
                    );
                } else {
                    footer_parts
                        .push("Reply with a number or react with the matching number".to_string());
                }
            } else {
                footer_parts.push(format!(
                    "Question {}/{} — reply with one line per question or use semicolons",
                    question_index + 1,
                    prompt.questions.len()
                ));
            }

            if let Some(ref context) = prompt.context {
                if let Some(ref source) = context.source {
                    footer_parts.push(format!("Source: {}", source));
                }
            }

            DiscordEmbed {
                title: q.header.clone(),
                description: q.question.clone(),
                color: 0x7c3aed,
                fields: vec![DiscordField {
                    name: "Options".to_string(),
                    value: option_lines.join("\n"),
                    inline: None,
                }],
                footer: Some(DiscordFooter {
                    text: footer_parts.join(" · "),
                }),
            }
        })
        .collect();

    DiscordFormattedResponse {
        embeds,
        reaction_emojis,
    }
}

/// Discord reaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordReaction {
    pub emoji: String,
    pub count: usize,
}

/// Parse a Discord response from reactions
pub fn parse_discord_reaction_response(
    reactions: &[DiscordReaction],
    questions: &[RemoteQuestion],
) -> RemoteAnswer {
    let mut answers = HashMap::new();

    if questions.len() != 1 {
        for q in questions {
            answers.insert(
                q.id.clone(),
                RemoteAnswerValue {
                    answers: Some(Vec::new()),
                    user_note: Some(
                        "Discord reactions are only supported for single-question prompts"
                            .to_string(),
                    ),
                },
            );
        }
        return RemoteAnswer { answers };
    }

    let q = &questions[0];
    let picked: Vec<String> = reactions
        .iter()
        .filter(|r| DISCORD_NUMBER_EMOJIS.contains(&r.emoji.as_str()) && r.count > 0)
        .filter_map(|r| {
            let idx = DISCORD_NUMBER_EMOJIS.iter().position(|&e| e == r.emoji)?;
            q.options.get(idx).map(|opt| opt.label.clone())
        })
        .collect();

    let answer_value = if picked.is_empty() {
        RemoteAnswerValue {
            answers: Some(Vec::new()),
            user_note: Some("No clear response via reactions".to_string()),
        }
    } else {
        RemoteAnswerValue {
            answers: Some(if q.allow_multiple {
                picked
            } else {
                vec![picked[0].clone()]
            }),
            user_note: None,
        }
    };

    answers.insert(q.id.clone(), answer_value);
    RemoteAnswer { answers }
}

/// Parse a Discord response from reply text
pub fn parse_discord_response(
    reply_text: Option<&str>,
    questions: &[RemoteQuestion],
) -> RemoteAnswer {
    if let Some(text) = reply_text {
        return parse_slack_reply(text, questions);
    }

    let mut answers = HashMap::new();

    if questions.len() != 1 {
        for q in questions {
            answers.insert(
                q.id.clone(),
                RemoteAnswerValue {
                    answers: Some(Vec::new()),
                    user_note: Some(
                        "Discord reactions are only supported for single-question prompts"
                            .to_string(),
                    ),
                },
            );
        }
        return RemoteAnswer { answers };
    }

    let q = &questions[0];
    answers.insert(
        q.id.clone(),
        RemoteAnswerValue {
            answers: Some(Vec::new()),
            user_note: Some("No clear response via reactions".to_string()),
        },
    );

    RemoteAnswer { answers }
}

// ─── Telegram Formatting ────────────────────────────────────────────────────

/// Format a prompt for Telegram
pub fn format_for_telegram(prompt: &RemotePrompt) -> TelegramMessage {
    let mut lines = vec![
        "<b>Orchestra needs your input</b>".to_string(),
        "".to_string(),
    ];

    for (qi, q) in prompt.questions.iter().enumerate() {
        lines.push(format!("<b>{}</b>", escape_html(&q.header)));
        lines.push(escape_html(&q.question));
        lines.push("".to_string());

        for (i, opt) in q.options.iter().enumerate() {
            lines.push(format!(
                "{}. <b>{}</b> — {}",
                i + 1,
                opt.label,
                escape_html(&opt.description)
            ));
        }

        lines.push("".to_string());

        if prompt.questions.len() == 1 {
            lines.push(if q.allow_multiple {
                "Reply with comma-separated numbers (1,3) or free text.".to_string()
            } else {
                "Reply with a number or tap a button below.".to_string()
            });
        } else {
            lines.push(format!(
                "Question {}/{} — reply with one line per question or use semicolons.",
                qi + 1,
                prompt.questions.len()
            ));
        }

        if qi < prompt.questions.len() - 1 {
            lines.push("".to_string());
        }
    }

    let mut result = TelegramMessage {
        text: lines.join("\n"),
        parse_mode: "HTML".to_string(),
        reply_markup: None,
    };

    // Inline keyboard for single-question with <=5 options
    let is_single = prompt.questions.len() == 1;
    if is_single && prompt.questions[0].options.len() <= 5 {
        let inline_keyboard: Vec<Vec<TelegramInlineButton>> = prompt.questions[0]
            .options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                vec![TelegramInlineButton {
                    text: format!("{}. {}", i + 1, opt.label),
                    callback_data: format!("{}:{}", prompt.id, i),
                }]
            })
            .collect();

        result.reply_markup = Some(TelegramInlineKeyboardMarkup { inline_keyboard });
    }

    result
}

/// Parse a Telegram response
pub fn parse_telegram_response(
    callback_data: Option<&str>,
    reply_text: Option<&str>,
    questions: &[RemoteQuestion],
    prompt_id: &str,
) -> RemoteAnswer {
    // Handle callback_data from inline keyboard button press
    if let Some(callback) = callback_data {
        let pattern = format!("^{}:(\\d+)$", regex_escape(prompt_id));
        if let Ok(re) = regex::Regex::new(&pattern) {
            if let Some(caps) = re.captures(callback) {
                if questions.len() == 1 {
                    if let Some(idx_str) = caps.get(1) {
                        if let Ok(idx) = idx_str.as_str().parse::<usize>() {
                            let q = &questions[0];
                            if idx < q.options.len() {
                                let mut answers = HashMap::new();
                                answers.insert(
                                    q.id.clone(),
                                    RemoteAnswerValue {
                                        answers: Some(vec![q.options[idx].label.clone()]),
                                        user_note: None,
                                    },
                                );
                                return RemoteAnswer { answers };
                            }
                        }
                    }
                }
            }
        }
    }

    // Handle text reply — delegate to parse_slack_reply
    if let Some(text) = reply_text {
        return parse_slack_reply(text, questions);
    }

    // No response
    let mut answers = HashMap::new();
    for q in questions {
        answers.insert(
            q.id.clone(),
            RemoteAnswerValue {
                answers: Some(Vec::new()),
                user_note: Some("No response provided".to_string()),
            },
        );
    }
    RemoteAnswer { answers }
}

/// Escape HTML special characters
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape special regex characters
fn regex_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '.' | '*' | '+' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                vec!['\\', c]
            }
            _ => vec![c],
        })
        .collect()
}

// ─── Slack Reaction Parsing ──────────────────────────────────────────────────

/// Parse Slack reaction response
pub fn parse_slack_reaction_response(
    reaction_names: &[String],
    questions: &[RemoteQuestion],
) -> RemoteAnswer {
    let mut answers = HashMap::new();

    if questions.len() != 1 {
        for q in questions {
            answers.insert(
                q.id.clone(),
                RemoteAnswerValue {
                    answers: Some(Vec::new()),
                    user_note: Some(
                        "Slack reactions are only supported for single-question prompts"
                            .to_string(),
                    ),
                },
            );
        }
        return RemoteAnswer { answers };
    }

    let q = &questions[0];
    let picked: Vec<String> = reaction_names
        .iter()
        .filter(|name| SLACK_NUMBER_REACTION_NAMES.contains(&name.as_str()))
        .filter_map(|name| {
            let idx = SLACK_NUMBER_REACTION_NAMES
                .iter()
                .position(|&n| n == name)?;
            q.options.get(idx).map(|opt| opt.label.clone())
        })
        .collect();

    let answer_value = if picked.is_empty() {
        RemoteAnswerValue {
            answers: Some(Vec::new()),
            user_note: Some("No clear response via reactions".to_string()),
        }
    } else {
        RemoteAnswerValue {
            answers: Some(if q.allow_multiple {
                picked
            } else {
                vec![picked[0].clone()]
            }),
            user_note: None,
        }
    };

    answers.insert(q.id.clone(), answer_value);
    RemoteAnswer { answers }
}

// ─── Answer Parsing ─────────────────────────────────────────────────────────

/// Parse answer for a specific question
fn parse_answer_for_question(text: &str, q: &RemoteQuestion) -> RemoteAnswerValue {
    if text.is_empty() {
        return RemoteAnswerValue {
            answers: Some(Vec::new()),
            user_note: Some("No response provided".to_string()),
        };
    }

    // Check if text contains only digits, commas, and whitespace
    if is_number_list(text) {
        let nums: Vec<usize> = text
            .split(',')
            .flat_map(|s| s.trim().parse::<usize>())
            .filter(|&n| n >= 1 && n <= q.options.len())
            .collect();

        if !nums.is_empty() {
            let selected: Vec<String> = nums
                .iter()
                .map(|&n| q.options[n - 1].label.clone())
                .collect();
            return RemoteAnswerValue {
                answers: Some(if q.allow_multiple {
                    selected
                } else {
                    vec![selected[0].clone()]
                }),
                user_note: None,
            };
        }
    }

    // Try single number
    if let Ok(single) = text.parse::<usize>() {
        if single >= 1 && single <= q.options.len() {
            return RemoteAnswerValue {
                answers: Some(vec![q.options[single - 1].label.clone()]),
                user_note: None,
            };
        }
    }

    // Free text response
    RemoteAnswerValue {
        answers: Some(Vec::new()),
        user_note: Some(truncate_note(text)),
    }
}

/// Check if string contains only numbers, commas, and whitespace
fn is_number_list(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_digit() || c == ',' || c.is_ascii_whitespace())
}

/// Truncate note to maximum length
fn truncate_note(text: &str) -> String {
    if text.len() > MAX_USER_NOTE_LENGTH {
        let trunc = text.floor_char_boundary(MAX_USER_NOTE_LENGTH);
        format!("{}…", &text[..trunc])
    } else {
        text.to_string()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("Hello & goodbye"), "Hello &amp; goodbye");
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a > b"), "a &gt; b");
        assert_eq!(escape_html("normal"), "normal");
    }

    #[test]
    fn test_regex_escape() {
        assert_eq!(regex_escape("test.js"), r"test\.js");
        assert_eq!(regex_escape("a+b"), r"a\+b");
        assert_eq!(regex_escape("normal"), "normal");
    }

    #[test]
    fn test_is_number_list() {
        assert!(is_number_list("1"));
        assert!(is_number_list("1, 2, 3"));
        assert!(is_number_list("1,2,3"));
        assert!(is_number_list(" 1 , 2 , 3 "));
        let result1 = !is_number_list("abc");
        assert!(result1);
        let result2 = !is_number_list("1, 2, abc");
        assert!(result2);
        let result3 = !is_number_list("1.5");
        assert!(result3);
    }

    #[test]
    fn test_truncate_note() {
        let long_text = "a".repeat(600);
        let truncated = truncate_note(&long_text);
        // Ellipsis '…' is 3 bytes in UTF-8
        assert_eq!(truncated.len(), MAX_USER_NOTE_LENGTH + 3);
        assert!(truncated.ends_with('…'));

        let short_text = "short";
        assert_eq!(truncate_note(short_text), "short");
    }

    #[test]
    fn test_format_for_slack_single_question() {
        let prompt = RemotePrompt {
            id: "test-prompt".to_string(),
            questions: vec![RemoteQuestion {
                id: "q1".to_string(),
                header: "Question 1".to_string(),
                question: "What is your choice?".to_string(),
                options: vec![
                    RemoteOption {
                        label: "Option 1".to_string(),
                        description: "First option".to_string(),
                    },
                    RemoteOption {
                        label: "Option 2".to_string(),
                        description: "Second option".to_string(),
                    },
                ],
                allow_multiple: false,
            }],
            context: None,
        };

        let blocks = format_for_slack(&prompt);
        assert!(!blocks.is_empty());
        assert_eq!(blocks[0].block_type, "header");
    }

    #[test]
    fn test_parse_slack_reply_single_question() {
        let questions = vec![RemoteQuestion {
            id: "q1".to_string(),
            header: "Question 1".to_string(),
            question: "What is your choice?".to_string(),
            options: vec![
                RemoteOption {
                    label: "Option 1".to_string(),
                    description: "First option".to_string(),
                },
                RemoteOption {
                    label: "Option 2".to_string(),
                    description: "Second option".to_string(),
                },
            ],
            allow_multiple: false,
        }];

        let answer = parse_slack_reply("1", &questions);
        assert!(answer.answers.contains_key("q1"));
        let ans_value = answer.answers.get("q1").unwrap();
        assert_eq!(ans_value.answers.as_ref().unwrap().len(), 1);
        assert_eq!(ans_value.answers.as_ref().unwrap()[0], "Option 1");
    }

    #[test]
    fn test_parse_discord_reaction_response() {
        let questions = vec![RemoteQuestion {
            id: "q1".to_string(),
            header: "Question 1".to_string(),
            question: "What is your choice?".to_string(),
            options: vec![
                RemoteOption {
                    label: "Option 1".to_string(),
                    description: "First option".to_string(),
                },
                RemoteOption {
                    label: "Option 2".to_string(),
                    description: "Second option".to_string(),
                },
            ],
            allow_multiple: false,
        }];

        let reactions = vec![
            DiscordReaction {
                emoji: "1️⃣".to_string(),
                count: 1,
            },
            DiscordReaction {
                emoji: "2️⃣".to_string(),
                count: 0,
            },
        ];

        let answer = parse_discord_reaction_response(&reactions, &questions);
        assert!(answer.answers.contains_key("q1"));
        let ans_value = answer.answers.get("q1").unwrap();
        assert_eq!(ans_value.answers.as_ref().unwrap().len(), 1);
        assert_eq!(ans_value.answers.as_ref().unwrap()[0], "Option 1");
    }

    #[test]
    fn test_parse_slack_reaction_response() {
        let questions = vec![RemoteQuestion {
            id: "q1".to_string(),
            header: "Question 1".to_string(),
            question: "What is your choice?".to_string(),
            options: vec![
                RemoteOption {
                    label: "Option 1".to_string(),
                    description: "First option".to_string(),
                },
                RemoteOption {
                    label: "Option 2".to_string(),
                    description: "Second option".to_string(),
                },
            ],
            allow_multiple: false,
        }];

        let reaction_names = vec!["one".to_string(), "two".to_string()];
        let answer = parse_slack_reaction_response(&reaction_names, &questions);

        assert!(answer.answers.contains_key("q1"));
        let ans_value = answer.answers.get("q1").unwrap();
        assert_eq!(ans_value.answers.as_ref().unwrap().len(), 1);
        assert_eq!(ans_value.answers.as_ref().unwrap()[0], "Option 1");
    }

    #[test]
    fn test_parse_answer_free_text() {
        let question = RemoteQuestion {
            id: "q1".to_string(),
            header: "Question 1".to_string(),
            question: "What is your choice?".to_string(),
            options: vec![
                RemoteOption {
                    label: "Option 1".to_string(),
                    description: "First option".to_string(),
                },
                RemoteOption {
                    label: "Option 2".to_string(),
                    description: "Second option".to_string(),
                },
            ],
            allow_multiple: false,
        };

        let answer = parse_answer_for_question("custom answer", &question);
        assert!(answer.answers.as_ref().unwrap().is_empty());
        assert!(answer.user_note.is_some());
        assert!(answer.user_note.as_ref().unwrap().contains("custom"));
    }

    #[test]
    fn test_parse_answer_multiple_numbers() {
        let question = RemoteQuestion {
            id: "q1".to_string(),
            header: "Question 1".to_string(),
            question: "What is your choice?".to_string(),
            options: vec![
                RemoteOption {
                    label: "Option 1".to_string(),
                    description: "First option".to_string(),
                },
                RemoteOption {
                    label: "Option 2".to_string(),
                    description: "Second option".to_string(),
                },
                RemoteOption {
                    label: "Option 3".to_string(),
                    description: "Third option".to_string(),
                },
            ],
            allow_multiple: true,
        };

        let answer = parse_answer_for_question("1,2", &question);
        assert_eq!(answer.answers.as_ref().unwrap().len(), 2);
        assert_eq!(answer.answers.as_ref().unwrap()[0], "Option 1");
        assert_eq!(answer.answers.as_ref().unwrap()[1], "Option 2");
    }

    #[test]
    fn test_truncate_note_multibyte_utf8() {
        // Verify truncation handles multi-byte UTF-8 characters without panicking
        let emoji_note = "Hello 🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍";
        let result = truncate_note(emoji_note);
        assert!(
            result.is_char_boundary(result.len()),
            "truncated note should be valid UTF-8"
        );

        // Chinese characters (3 bytes each)
        let chinese = "这是一个很长的中文笔记应该被截断";
        let result = truncate_note(chinese);
        assert!(result.is_char_boundary(result.len()));

        // Short string should pass through unchanged
        assert_eq!(truncate_note("short"), "short");

        // Empty string
        assert_eq!(truncate_note(""), "");
    }
}
