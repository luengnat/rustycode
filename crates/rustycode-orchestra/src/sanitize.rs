//! Orchestra Sanitize — Error message sanitization utilities.
//!
//! Redacts token-like strings from error messages before surfacing them.
//! Prevents accidental leakage of API keys, tokens, and other secrets.
//!
//! Matches orchestra-2's sanitize.ts implementation.

use once_cell::sync::Lazy;
use regex::Regex;

// ─── Token Patterns ───────────────────────────────────────────────────────────

/// Token patterns for redaction
static TOKEN_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Anthropic API keys: sk-ant-...
        Regex::new(r"sk-ant-api03-[A-Za-z0-9\-_]{80,}").unwrap(),
        // OpenAI API keys: sk-...
        Regex::new(r"sk-[A-Za-z0-9]{40,}").unwrap(),
        // AWS access keys: AKIA...
        Regex::new(r"AKIA[A-Z0-9]{16}").unwrap(),
        // GitHub tokens: ghp_, gho_, ghu_, ghs_, ghr_
        Regex::new(r"gh[posur]_[A-Za-z0-9]{36,}").unwrap(),
        // GitLab tokens: glpat-...
        Regex::new(r"glpat-[A-Za-z0-9\-]{20,}").unwrap(),
        // Generic Bearer tokens in headers
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-_.~+/]+=*").unwrap(),
        // Slack bot tokens: xoxb-...
        Regex::new(r"xoxb-[A-Za-z0-9\-]+").unwrap(),
        // Slack user tokens: xoxp-...
        Regex::new(r"xoxp-[A-Za-z0-9\-]+").unwrap(),
        // Slack app tokens: xoxa-...
        Regex::new(r"xoxa-[A-Za-z0-9\-]+").unwrap(),
        // Telegram bot tokens: 8-10 digits : 35 chars
        Regex::new(r"\d{8,10}:[A-Za-z0-9_-]{35}").unwrap(),
        // Long opaque secrets (Discord tokens, etc.): 20+ chars
        Regex::new(r"[A-Za-z0-9_\-.]{20,}").unwrap(),
    ]
});

// ─── Sanitization ─────────────────────────────────────────────────────────────

/// Sanitize an error message by redacting token-like strings.
///
/// Replaces patterns that look like API keys, tokens, or secrets with `[REDACTED]`.
/// Prevents accidental leakage of sensitive information in logs and error messages.
///
/// # Arguments
/// * `msg` - The error message to sanitize
///
/// # Returns
/// Sanitized message with tokens redacted
///
/// # Examples
/// ```
/// use rustycode_orchestra::sanitize::sanitize_error;
///
/// let msg = "Error with token xoxb-1234567890abcdef";
/// let sanitized = sanitize_error(msg);
/// assert_eq!(sanitized, "Error with token [REDACTED]");
///
/// let msg2 = "API key: sk-placeholder-key-example";
/// let sanitized2 = sanitize_error(msg2);
/// assert!(sanitized2.contains("[REDACTED]"));
/// ```
pub fn sanitize_error(msg: &str) -> String {
    let mut sanitized = msg.to_string();
    for pattern in TOKEN_PATTERNS.iter() {
        sanitized = pattern.replace_all(&sanitized, "[REDACTED]").to_string();
    }
    sanitized
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_slack_bot_token() {
        let msg = "Error with token xoxb-1234567890abcdef";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "Error with token [REDACTED]");
    }

    #[test]
    fn test_sanitize_slack_user_token() {
        let msg = "User token xoxp-9876543210fedcba invalid";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "User token [REDACTED] invalid");
    }

    #[test]
    fn test_sanitize_slack_app_token() {
        let msg = "App token xoxa-abcdef1234567890 expired";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "App token [REDACTED] expired");
    }

    #[test]
    fn test_sanitize_telegram_token() {
        // Telegram bot tokens: 8-10 digits : 35 chars
        let msg = "Bot 1234567890:ABCdefghijklmnopqrstuvwxyz123456789 failed";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "Bot [REDACTED] failed");
    }

    #[test]
    fn test_sanitize_long_opaque_secret() {
        let msg = "Key: abcdefghijklmnopqrst12-filename.ext";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "Key: [REDACTED]");
    }

    #[test]
    fn test_sanitize_multiple_tokens() {
        let msg = "Slack tokens xoxb-123 and xoxp-456 found";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "Slack tokens [REDACTED] and [REDACTED] found");
    }

    #[test]
    fn test_sanitize_no_tokens() {
        let msg = "Regular error message without tokens";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "Regular error message without tokens");
    }

    #[test]
    fn test_sanitize_empty_string() {
        let msg = "";
        let sanitized = sanitize_error(msg);
        assert_eq!(sanitized, "");
    }

    #[test]
    fn test_sanitize_anthropic_api_key() {
        let key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let msg = format!("Error with key {}", key);
        let sanitized = sanitize_error(&msg);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("sk-ant"));
    }

    #[test]
    fn test_sanitize_openai_api_key() {
        let msg = "Failed with key sk-1234567890abcdef1234567890abcdef1234567890abcdef";
        let sanitized = sanitize_error(msg);
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_aws_access_key() {
        let msg = "AWS key AKIAIOSFODNN7EXAMPLE detected";
        let sanitized = sanitize_error(msg);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("AKIA"));
    }

    #[test]
    fn test_sanitize_github_token() {
        let msg = "Using token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let sanitized = sanitize_error(msg);
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_gitlab_token() {
        let msg = "Token glpat-abcdefghijklmnopqrstuvwx";
        let sanitized = sanitize_error(msg);
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_bearer_token() {
        let msg = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc";
        let sanitized = sanitize_error(msg);
        assert!(sanitized.contains("[REDACTED]"));
    }
}
