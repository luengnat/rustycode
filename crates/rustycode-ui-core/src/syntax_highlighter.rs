//! Syntax highlighting using syntect
//!
//! Provides code syntax highlighting with TextMate grammar support
//! and automatic language detection.

use once_cell::sync::Lazy;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::{
    easy::HighlightLines, highlighting::Theme, highlighting::ThemeSet, parsing::SyntaxSet,
};

/// Shared syntax set for performance
static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

/// Shared theme set for performance
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// Syntax highlighting using syntect
pub struct SyntaxHighlighter {
    /// Reference to the shared theme
    theme: &'static Theme,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter with the default theme
    pub fn new() -> Self {
        Self::new_with_theme("base16-ocean.dark")
    }

    /// Create a new syntax highlighter with the given theme name
    pub fn new_with_theme(theme_name: &str) -> Self {
        let theme = THEME_SET
            .themes
            .get(theme_name)
            .or_else(|| THEME_SET.themes.get("base16-ocean.dark"))
            .unwrap_or_else(|| THEME_SET.themes.values().next().unwrap());

        Self { theme }
    }

    /// Highlight code with syntax highlighting
    pub fn highlight(&self, code: &str, language: Option<&str>) -> Vec<Line<'static>> {
        let syntax = language
            .and_then(|lang| {
                SYNTAX_SET
                    .find_syntax_by_token(lang)
                    .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang))
            })
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, self.theme);

        let mut lines = Vec::new();
        for (line_num, line) in code.lines().enumerate() {
            let ranges = highlighter
                .highlight_line(line, &SYNTAX_SET)
                .unwrap_or_default();

            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, text)| {
                    let r = style.foreground.r;
                    let g = style.foreground.g;
                    let b = style.foreground.b;

                    // Ensure color is visible (not pure black on black)
                    let fg = if r == 0 && g == 0 && b == 0 {
                        Color::Rgb(200, 200, 200) // Light gray for invisible colors
                    } else {
                        Color::Rgb(r, g, b)
                    };

                    // Log braces for debugging first few lines
                    if line_num < 3 && text.contains('{') || text.contains('}') {
                        tracing::debug!(
                            "Brace rendering: '{}' with color RGB({},{},{})",
                            text,
                            r,
                            g,
                            b
                        );
                    }

                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            // Convert to 'static by owning strings
            lines.push(Line::from(spans));
        }
        lines
    }

    /// Auto-detect language and highlight code
    pub fn highlight_auto(&self, code: &str, file_hint: Option<&str>) -> Vec<Line<'static>> {
        let language = file_hint
            .and_then(|f| self.guess_language_from_file(f))
            .unwrap_or_else(|| self.guess_language_from_content(code));

        self.highlight(code, Some(&language))
    }

    /// Guess language from file extension
    fn guess_language_from_file(&self, filename: &str) -> Option<String> {
        let ext = std::path::Path::new(filename).extension()?.to_str()?;

        let language = match ext {
            "rs" => "rust",
            "py" => "python",
            "js" => "javascript",
            "ts" => "typescript",
            "jsx" => "javascript",
            "tsx" => "typescript",
            "go" => "go",
            "java" => "java",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" => "cpp",
            "cs" => "csharp",
            "php" => "php",
            "rb" => "ruby",
            "sh" => "bash",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "json" => "json",
            "md" => "markdown",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" => "css",
            "scss" | "sass" => "scss",
            "xml" => "xml",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "scala" => "scala",
            "dart" => "dart",
            "lua" => "lua",
            "ps1" => "powershell",
            "dockerfile" => "dockerfile",
            "r" | "R" => "r",
            _ => return None,
        };
        Some(language.to_string())
    }

    /// Guess language from code content
    fn guess_language_from_content(&self, code: &str) -> String {
        // Simple heuristics for language detection
        if code.contains("fn ") && code.contains("impl ") {
            "rust".to_string()
        } else if code.contains("def ") && code.contains("import ") {
            "python".to_string()
        } else if code.contains("function ") || code.contains("const ") {
            "javascript".to_string()
        } else if code.contains("package ") && code.contains("func ") {
            "go".to_string()
        } else if code.contains("public class ") {
            "java".to_string()
        } else if code.contains("func ") && code.contains("var ") && code.contains(":=") {
            "go".to_string()
        } else if code.contains("class ") && code.contains(": ") && code.contains("def ") {
            "python".to_string()
        } else if code.contains("import ") && code.contains("@main") {
            "swift".to_string()
        } else if code.contains("fun ") && code.contains("val ") && code.contains("var ") {
            "kotlin".to_string()
        } else if code.contains("struct ") && code.contains("impl ") && code.contains("fn ") {
            "rust".to_string()
        } else {
            "plaintext".to_string()
        }
    }

    /// Fallback highlighting without syntax (emergency use only)
    pub fn highlight_plain(&self, code: &str) -> Vec<Line<'static>> {
        code.lines()
            .map(|line| Line::from(vec![Span::raw(line.to_string())]))
            .collect()
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_highlighter_new() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight("fn test() {}", Some("rust"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_guess_language_from_file() {
        let highlighter = SyntaxHighlighter::new();

        assert_eq!(
            highlighter.guess_language_from_file("test.rs"),
            Some("rust".to_string())
        );
        assert_eq!(
            highlighter.guess_language_from_file("test.py"),
            Some("python".to_string())
        );
        assert_eq!(
            highlighter.guess_language_from_file("test.js"),
            Some("javascript".to_string())
        );
    }

    // ── New tests ─────────────────────────────────────────────────────

    #[test]
    fn test_default_trait() {
        let h1 = SyntaxHighlighter::default();
        let h2 = SyntaxHighlighter::new();
        // Both should produce the same results
        let lines1 = h1.highlight("fn main() {}", Some("rust"));
        let lines2 = h2.highlight("fn main() {}", Some("rust"));
        assert_eq!(lines1.len(), lines2.len());
    }

    #[test]
    fn test_new_with_valid_theme() {
        let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
        let lines = highlighter.highlight("fn main() {}", Some("rust"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_new_with_invalid_theme_falls_back() {
        let highlighter = SyntaxHighlighter::new_with_theme("nonexistent-theme");
        let lines = highlighter.highlight("fn main() {}", Some("rust"));
        // Should still work with fallback theme
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_no_language() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight("some plain text", None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_unknown_language() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight("hello world", Some("xyznonexistent"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_empty_code() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight("", Some("rust"));
        assert!(lines.is_empty());
    }

    #[test]
    fn test_highlight_multiline_code() {
        let highlighter = SyntaxHighlighter::new();
        let code = "fn main() {\n    println!(\"Hello\");\n}\n";
        let lines = highlighter.highlight(code, Some("rust"));
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_highlight_auto_with_file_hint() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight_auto("fn main() {}", Some("test.rs"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_rust() {
        let highlighter = SyntaxHighlighter::new();
        let code = "fn test() {}\nimpl Foo {}";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_python() {
        let highlighter = SyntaxHighlighter::new();
        let code = "def foo():\n    import os\n    pass";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_javascript() {
        let highlighter = SyntaxHighlighter::new();
        let code = "function foo() {\n    const x = 1;\n}";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_go() {
        let highlighter = SyntaxHighlighter::new();
        let code = "package main\n\nfunc main() {\n    fmt.Println(\"hi\")\n}";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_java() {
        let highlighter = SyntaxHighlighter::new();
        let code = "public class Main {\n    public static void main(String[] args) {}\n}";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_swift() {
        let highlighter = SyntaxHighlighter::new();
        let code = "import Foundation\n@main";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_kotlin() {
        let highlighter = SyntaxHighlighter::new();
        let code = "fun main() {\n    val x = 1\n    var y = 2\n}";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_auto_no_hint_falls_back_to_plaintext() {
        let highlighter = SyntaxHighlighter::new();
        let code = "just some random text with no clear language markers";
        let lines = highlighter.highlight_auto(code, None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_plain() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight_plain("line1\nline2\nline3");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_highlight_plain_empty() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight_plain("");
        assert!(lines.is_empty() || lines.len() <= 1);
    }

    #[test]
    fn test_guess_language_from_file_all_extensions() {
        let highlighter = SyntaxHighlighter::new();

        let cases = vec![
            ("test.rs", "rust"),
            ("test.py", "python"),
            ("test.js", "javascript"),
            ("test.ts", "typescript"),
            ("test.jsx", "javascript"),
            ("test.tsx", "typescript"),
            ("test.go", "go"),
            ("test.java", "java"),
            ("test.c", "c"),
            ("test.h", "c"),
            ("test.cpp", "cpp"),
            ("test.cc", "cpp"),
            ("test.cxx", "cpp"),
            ("test.hpp", "cpp"),
            ("test.cs", "csharp"),
            ("test.php", "php"),
            ("test.rb", "ruby"),
            ("test.sh", "bash"),
            ("test.yaml", "yaml"),
            ("test.yml", "yaml"),
            ("test.toml", "toml"),
            ("test.json", "json"),
            ("test.md", "markdown"),
            ("test.sql", "sql"),
            ("test.html", "html"),
            ("test.htm", "html"),
            ("test.css", "css"),
            ("test.scss", "scss"),
            ("test.sass", "scss"),
            ("test.xml", "xml"),
            ("test.swift", "swift"),
            ("test.kt", "kotlin"),
            ("test.kts", "kotlin"),
            ("test.scala", "scala"),
            ("test.dart", "dart"),
            ("test.lua", "lua"),
            ("test.ps1", "powershell"),
            ("file.dockerfile", "dockerfile"),
            ("test.r", "r"),
            ("test.R", "r"),
        ];

        for (filename, expected) in cases {
            assert_eq!(
                highlighter.guess_language_from_file(filename),
                Some(expected.to_string()),
                "Failed for filename: {}",
                filename
            );
        }
    }

    #[test]
    fn test_guess_language_from_file_unknown_extension() {
        let highlighter = SyntaxHighlighter::new();
        assert_eq!(highlighter.guess_language_from_file("test.xyz"), None);
        assert_eq!(highlighter.guess_language_from_file("test.abc"), None);
    }

    #[test]
    fn test_guess_language_from_file_no_extension() {
        let highlighter = SyntaxHighlighter::new();
        assert_eq!(highlighter.guess_language_from_file("Makefile"), None);
        assert_eq!(highlighter.guess_language_from_file("README"), None);
    }

    #[test]
    fn test_guess_language_from_file_with_path() {
        let highlighter = SyntaxHighlighter::new();
        assert_eq!(
            highlighter.guess_language_from_file("src/main.rs"),
            Some("rust".to_string())
        );
        assert_eq!(
            highlighter.guess_language_from_file("/absolute/path/to/test.py"),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_guess_language_from_content_go_with_var_and_assignment() {
        let highlighter = SyntaxHighlighter::new();
        let code = "func example() {\n    var x := 5\n}";
        assert_eq!(highlighter.guess_language_from_content(code), "go");
    }

    #[test]
    fn test_guess_language_from_content_python_class() {
        let highlighter = SyntaxHighlighter::new();
        // Must match both "class " + ": " + "def " for the python-class heuristic
        // Using spaces before colon to satisfy the ": " check
        let code = "class MyClass : Base\n    def __init__(self):\n        pass";
        assert_eq!(highlighter.guess_language_from_content(code), "python");
    }

    #[test]
    fn test_guess_language_from_content_rust_struct_and_impl() {
        let highlighter = SyntaxHighlighter::new();
        let code = "struct Foo {}\nimpl Foo {\n    fn new() -> Self {}\n}";
        assert_eq!(highlighter.guess_language_from_content(code), "rust");
    }

    #[test]
    fn test_guess_language_from_content_plaintext_fallback() {
        let highlighter = SyntaxHighlighter::new();
        let code = "Lorem ipsum dolor sit amet";
        assert_eq!(highlighter.guess_language_from_content(code), "plaintext");
    }

    #[test]
    fn test_highlight_with_extension_as_language() {
        let highlighter = SyntaxHighlighter::new();
        // Should work with both language name and extension
        let lines_name = highlighter.highlight("def foo(): pass", Some("python"));
        let lines_ext = highlighter.highlight("def foo(): pass", Some("py"));
        assert!(!lines_name.is_empty());
        assert!(!lines_ext.is_empty());
    }
}
