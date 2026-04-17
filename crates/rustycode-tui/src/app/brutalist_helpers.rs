//! Helper functions for brutalist rendering
//!
//! Utility functions for formatting and display in the brutalist TUI.

/// Format elapsed seconds into a compact display string.
///
/// Goose pattern: compact timing display for status bars.
/// Examples: "3s", "1m4s", "2m"
pub fn format_elapsed_short(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else {
        let mins = secs / 60;
        let remain_secs = secs % 60;
        if remain_secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m{}s", mins, remain_secs)
        }
    }
}

/// Format token count compactly for inline display.
///
/// Examples: "8.2k", "1.5M", "500"
pub fn format_tokens_compact(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Extract the most relevant parameter from a tool call for inline display.
///
/// Goose pattern: show file path for file tools, command for shell tools,
/// query for search tools — the single most useful piece of context.
pub fn extract_tool_key_param(
    tool_name: &str,
    input_json: Option<&serde_json::Value>,
    result_summary: &str,
) -> Option<String> {
    let name = tool_name.to_lowercase();

    if let Some(json) = input_json {
        if name.contains("read")
            || name.contains("write")
            || name.contains("edit")
            || name.contains("create")
            || name.contains("view")
            || name.contains("cat")
        {
            if let Some(path) = json
                .get("file_path")
                .or(json.get("path"))
                .and_then(|v| v.as_str())
            {
                return Some(shorten_tool_param(path, 50));
            }
        }

        if name.contains("bash")
            || name.contains("exec")
            || name.contains("shell")
            || name.contains("run")
        {
            if let Some(cmd) = json
                .get("command")
                .or(json.get("cmd"))
                .and_then(|v| v.as_str())
            {
                let first_line = cmd.lines().next().unwrap_or(cmd);
                let truncated = if first_line.len() > 40 {
                    format!("{}…", &first_line[..first_line.floor_char_boundary(39)])
                } else {
                    first_line.to_string()
                };
                return Some(truncated);
            }
        }

        if name.contains("grep") || name.contains("search") {
            if let Some(pattern) = json
                .get("pattern")
                .or(json.get("query"))
                .and_then(|v| v.as_str())
            {
                return Some(if pattern.len() > 40 {
                    format!("{}…", &pattern[..pattern.floor_char_boundary(39)])
                } else {
                    pattern.to_string()
                });
            }
        }

        if name.contains("glob") || name.contains("find") || name.contains("list") {
            if let Some(pattern) = json
                .get("pattern")
                .or(json.get("glob"))
                .and_then(|v| v.as_str())
            {
                return Some(pattern.to_string());
            }
        }
    }

    if !result_summary.is_empty()
        && result_summary.len() < 80
        && (name.contains("read") || name.contains("write") || name.contains("edit"))
        && (result_summary.contains('/') || result_summary.contains('\\'))
    {
        return Some(shorten_tool_param(result_summary, 50));
    }

    None
}

/// Shorten a tool parameter (typically a file path) for compact display.
///
/// Uses Goose-style path shortening: abbreviates middle components to their
/// first letter while preserving the filename and prefix.
/// Example: `/Users/nat/dev/rustycode/crates/rustycode-tui/src/app/mod.rs`
///       → `~/d/r/c/r-tui/s/a/mod.rs`
pub fn shorten_tool_param(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    let display = if let Ok(home) = std::env::var("HOME") {
        if s.starts_with(&home) {
            format!("~{}", &s[home.len()..])
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    };

    if display.len() <= max_len {
        return display;
    }

    let components: Vec<&str> = display.split('/').collect();
    if components.len() <= 2 {
        return display[..max_len.saturating_sub(3)].to_string() + "…";
    }

    let first = components.first().unwrap_or(&"");
    let last = components.last().unwrap_or(&"");

    let prefix = if first.is_empty() { "/" } else { "" };
    let suffix = format!("/{}", last);

    let available = max_len.saturating_sub(prefix.len() + suffix.len() + 3);

    if available < 4 {
        return format!("{}{}…{}", prefix, &first[..1], last);
    }

    let mut result = prefix.to_string();
    result.push_str(&first[..1]);

    for comp in components.iter().skip(1).take(components.len() - 2) {
        if result.len() + comp.len() + suffix.len() + 3 > max_len {
            break;
        }
        result.push('/');
        result.push_str(&comp[..1]);
    }

    result.push('…');
    result.push_str(&suffix);

    result
}

/// Count consecutive occurrences of `byte` starting at `start`.
pub fn count_consecutive(bytes: &[u8], start: usize, byte: u8) -> usize {
    bytes[start..].iter().take_while(|&&b| b == byte).count()
}

/// Find position of `count` consecutive `byte` values in the slice.
pub fn find_consecutive(bytes: &[u8], byte: u8, count: usize) -> Option<usize> {
    bytes
        .windows(count)
        .position(|window| window.iter().all(|&b| b == byte))
}

/// Find position of two consecutive `byte` values (e.g., `**` or `~~`).
pub fn find_byte_pair(bytes: &[u8], byte: u8) -> Option<usize> {
    find_consecutive(bytes, byte, 2)
}

/// Find position of a single byte.
pub fn find_byte(bytes: &[u8], byte: u8) -> Option<usize> {
    bytes.iter().position(|&b| b == byte)
}

/// Get a type-specific icon for a tool name (ASCII-safe, consistent with worker panel)
pub fn tool_type_icon(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("read") || n.contains("view") || n.contains("cat") {
        "◎"
    } else if n.contains("write") || n.contains("edit") || n.contains("create") {
        "✎"
    } else if n.contains("bash") || n.contains("shell") || n.contains("exec") {
        "▸"
    } else if n.contains("search") || n.contains("grep") || n.contains("find") {
        "⌕"
    } else if n.contains("glob") || n.contains("list") {
        "⋮"
    } else if n.contains("diff") || n.contains("patch") {
        "≠"
    } else if n.contains("git") {
        "⎇"
    } else if n.contains("mcp") || n.contains("server") {
        "◉"
    } else if n.contains("apply") || n.contains("tool") {
        "▶"
    } else {
        "○"
    }
}
