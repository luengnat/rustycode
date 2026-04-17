//! Orchestra Logo — ASCII block-letter logo for Orchestra.
//!
//! Provides the Orchestra logo as an array of lines and a function to render it
//! with optional color formatting.
//!
//! Matches orchestra-2's logo.ts implementation.

/// Raw Orchestra logo lines — no ANSI codes, no leading newline
///
/// # Examples
/// ```
/// use rustycode_orchestra::logo::ORCHESTRA_LOGO;
///
/// for line in ORCHESTRA_LOGO {
///     println!("{}", line);
/// }
/// ```
pub const ORCHESTRA_LOGO: &[&str] = &[
    "   ██████╗ ███████╗██████╗ ",
    "  ██╔════╝ ██╔════╝██╔══██╗",
    "  ██║  ███╗███████╗██║  ██║",
    "  ██║   ██║╚════██║██║  ██║",
    "  ╚██████╔╝███████║██████╔╝",
    "   ╚═════╝ ╚══════╝╚═════╝ ",
];

/// Render the Orchestra logo with a color function applied to each line
///
/// # Arguments
/// * `color` - Function that takes a string and returns a colored string
///
/// # Returns
/// Ready-to-print string with leading/trailing newlines
///
/// # Examples
/// ```
/// use rustycode_orchestra::logo::render_logo;
///
/// // No color
/// let plain = render_logo(|s| s.to_string());
/// print!("{}", plain);
///
/// // With cyan ANSI color
/// let cyan = render_logo(|s| format!("\x1b[36m{}\x1b[0m", s));
/// print!("{}", cyan);
///
/// // With bold
/// let bold = render_logo(|s| format!("\x1b[1m{}\x1b[0m", s));
/// print!("{}", bold);
/// ```
pub fn render_logo<F>(color: F) -> String
where
    F: Fn(&str) -> String,
{
    let colored: Vec<String> = ORCHESTRA_LOGO.iter().map(|line| color(line)).collect();
    format!("\n{}\n", colored.join("\n"))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logo_lines() {
        assert_eq!(ORCHESTRA_LOGO.len(), 6);
        assert_eq!(ORCHESTRA_LOGO[0], "   ██████╗ ███████╗██████╗ ");
        assert_eq!(ORCHESTRA_LOGO[5], "   ╚═════╝ ╚══════╝╚═════╝ ");
    }

    #[test]
    fn test_render_logo_plain() {
        let rendered = render_logo(|s| s.to_string());
        assert!(rendered.starts_with('\n'));
        assert!(rendered.ends_with('\n'));
        assert_eq!(rendered.matches('\n').count(), 7); // Leading + 6 lines + trailing
    }

    #[test]
    fn test_render_logo_with_color() {
        let rendered = render_logo(|s| format!("[COLOR]{}", s));
        assert!(rendered.contains("[COLOR]   ██████╗"));
        assert!(rendered.contains("[COLOR]  ██╔════╝"));
    }

    #[test]
    fn test_render_logo_format() {
        let rendered = render_logo(|s| s.to_string());
        // Split by newline and filter out empty strings (from leading/trailing newlines)
        let lines: Vec<&str> = rendered.split('\n').filter(|s| !s.is_empty()).collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0], "   ██████╗ ███████╗██████╗ ");
    }

    #[test]
    fn test_logo_characters() {
        // Verify the logo contains expected Unicode box-drawing characters
        let all_lines = ORCHESTRA_LOGO.join("");
        assert!(all_lines.contains('█'));
        assert!(all_lines.contains('╗'));
        assert!(all_lines.contains('╝'));
        assert!(all_lines.contains('║'));
        assert!(all_lines.contains('╔'));
        assert!(all_lines.contains('╚'));
    }

    #[test]
    fn test_render_logo_idempotent() {
        let rendered1 = render_logo(|s| s.to_string());
        let rendered2 = render_logo(|s| s.to_string());
        assert_eq!(rendered1, rendered2);
    }

    #[test]
    fn test_render_logo_custom_formatter() {
        // Test with a custom formatter that adds line numbers
        let rendered = render_logo(|s| format!("LINE: {}", s));
        assert!(rendered.contains("LINE:    ██████╗"));
        assert!(rendered.contains("LINE:    ╚═════╝"));
    }
}
