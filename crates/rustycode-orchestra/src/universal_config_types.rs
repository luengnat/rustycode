//! Orchestra Universal Config Types — Shared types for config discovery.
//!
//! Normalized schema for discovered configuration items from all supported
//! AI coding tools: Claude Code, Cursor, Windsurf, Gemini CLI, Codex,
//! Cline, GitHub Copilot, VS Code.
//!
//! Matches orchestra-2's types.ts implementation.

use std::collections::HashMap;

// ── Tool Identifiers ───────────────────────────────────────────────────────────

/// Unique identifier for supported AI coding tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ToolId {
    Claude,
    Cursor,
    Windsurf,
    Gemini,
    Codex,
    Cline,
    GitHubCopilot,
    VSCode,
}

impl ToolId {
    /// Convert tool ID to string identifier
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolId::Claude => "claude",
            ToolId::Cursor => "cursor",
            ToolId::Windsurf => "windsurf",
            ToolId::Gemini => "gemini",
            ToolId::Codex => "codex",
            ToolId::Cline => "cline",
            ToolId::GitHubCopilot => "github-copilot",
            ToolId::VSCode => "vscode",
        }
    }

    /// Parse string identifier to tool ID
    pub fn parse_id(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(ToolId::Claude),
            "cursor" => Some(ToolId::Cursor),
            "windsurf" => Some(ToolId::Windsurf),
            "gemini" => Some(ToolId::Gemini),
            "codex" => Some(ToolId::Codex),
            "cline" => Some(ToolId::Cline),
            "github-copilot" => Some(ToolId::GitHubCopilot),
            "vscode" => Some(ToolId::VSCode),
            _ => None,
        }
    }

    /// Get display name for the tool
    pub fn display_name(&self) -> &'static str {
        match self {
            ToolId::Claude => "Claude Code",
            ToolId::Cursor => "Cursor",
            ToolId::Windsurf => "Windsurf",
            ToolId::Gemini => "Gemini CLI",
            ToolId::Codex => "OpenAI Codex",
            ToolId::Cline => "Cline",
            ToolId::GitHubCopilot => "GitHub Copilot",
            ToolId::VSCode => "VS Code",
        }
    }
}

// ── Config Level ──────────────────────────────────────────────────────────────

/// Configuration level (user vs project)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConfigLevel {
    User,
    Project,
}

impl ConfigLevel {
    /// Convert config level to string
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigLevel::User => "user",
            ConfigLevel::Project => "project",
        }
    }
}

// ── Source Metadata ───────────────────────────────────────────────────────────

/// Source information for a discovered config item
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSource {
    /// Which tool this config came from
    pub tool: ToolId,
    /// Display name of the tool
    pub tool_name: String,
    /// Absolute path to the config file
    pub path: String,
    /// User-level (~) or project-level (./)
    pub level: ConfigLevel,
}

// ── Discovered Config Items ───────────────────────────────────────────────────

/// Discovered MCP server configuration
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredMCPServer {
    pub type_name: &'static str, // "mcp-server"
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    pub transport: Option<TransportType>,
    pub source: ConfigSource,
}

/// Transport type for MCP server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportType {
    Stdio,
    Sse,
    Http,
}

impl TransportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransportType::Stdio => "stdio",
            TransportType::Sse => "sse",
            TransportType::Http => "http",
        }
    }
}

/// Discovered rule (e.g., .cursorrules, .clinerules)
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredRule {
    pub type_name: &'static str, // "rule"
    pub name: String,
    pub content: String,
    /// Glob patterns this rule applies to
    pub globs: Option<Vec<String>>,
    /// Whether the rule applies to all files
    pub always_apply: Option<bool>,
    pub description: Option<String>,
    pub source: ConfigSource,
}

/// Discovered context file (e.g., context.md, prompt.md)
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredContextFile {
    pub type_name: &'static str, // "context-file"
    pub name: String,
    pub content: String,
    pub source: ConfigSource,
}

/// Discovered settings (key-value pairs from config files)
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredSettings {
    pub type_name: &'static str, // "settings"
    pub data: HashMap<String, serde_json::Value>,
    pub source: ConfigSource,
}

/// Discovered Claude skill
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredClaudeSkill {
    pub type_name: &'static str, // "claude-skill"
    pub name: String,
    pub path: String,
    pub source: ConfigSource,
}

/// Discovered Claude plugin
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredClaudePlugin {
    pub type_name: &'static str, // "claude-plugin"
    pub name: String,
    pub path: String,
    pub package_name: Option<String>,
    pub source: ConfigSource,
}

/// Union type for all discovered items
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DiscoveredItem {
    MCPServer(DiscoveredMCPServer),
    Rule(DiscoveredRule),
    ContextFile(DiscoveredContextFile),
    Settings(DiscoveredSettings),
    ClaudeSkill(DiscoveredClaudeSkill),
    ClaudePlugin(DiscoveredClaudePlugin),
}

impl DiscoveredItem {
    /// Get the type name for this item
    pub fn type_name(&self) -> &'static str {
        match self {
            DiscoveredItem::MCPServer(_) => "mcp-server",
            DiscoveredItem::Rule(_) => "rule",
            DiscoveredItem::ContextFile(_) => "context-file",
            DiscoveredItem::Settings(_) => "settings",
            DiscoveredItem::ClaudeSkill(_) => "claude-skill",
            DiscoveredItem::ClaudePlugin(_) => "claude-plugin",
        }
    }

    /// Get the source for this item
    pub fn source(&self) -> &ConfigSource {
        match self {
            DiscoveredItem::MCPServer(item) => &item.source,
            DiscoveredItem::Rule(item) => &item.source,
            DiscoveredItem::ContextFile(item) => &item.source,
            DiscoveredItem::Settings(item) => &item.source,
            DiscoveredItem::ClaudeSkill(item) => &item.source,
            DiscoveredItem::ClaudePlugin(item) => &item.source,
        }
    }

    /// Get the name for this item
    pub fn name(&self) -> &str {
        match self {
            DiscoveredItem::MCPServer(item) => &item.name,
            DiscoveredItem::Rule(item) => &item.name,
            DiscoveredItem::ContextFile(item) => &item.name,
            DiscoveredItem::ClaudeSkill(item) => &item.name,
            DiscoveredItem::ClaudePlugin(item) => &item.name,
            DiscoveredItem::Settings(_) => "<settings>",
        }
    }
}

// ── Discovery Result ─────────────────────────────────────────────────────────

/// Discovery result for a single tool
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDiscoveryResult {
    pub tool: ToolId,
    pub tool_name: String,
    pub items: Vec<DiscoveredItem>,
    pub warnings: Vec<String>,
}

/// Summary counts for discovered items
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiscoverySummary {
    pub mcp_servers: usize,
    pub rules: usize,
    pub context_files: usize,
    pub settings: usize,
    pub claude_skills: usize,
    pub claude_plugins: usize,
    pub total_items: usize,
    pub tools_scanned: usize,
    pub tools_with_config: usize,
}

/// Complete discovery result across all tools
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryResult {
    /// All discovered items grouped by tool
    pub tools: Vec<ToolDiscoveryResult>,
    /// Flat list of all discovered items
    pub all_items: Vec<DiscoveredItem>,
    /// Summary counts by category
    pub summary: DiscoverySummary,
    /// Warnings from scanners
    pub warnings: Vec<String>,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_id_as_str() {
        assert_eq!(ToolId::Claude.as_str(), "claude");
        assert_eq!(ToolId::Cursor.as_str(), "cursor");
        assert_eq!(ToolId::GitHubCopilot.as_str(), "github-copilot");
    }

    #[test]
    fn test_tool_id_from_str() {
        assert_eq!(ToolId::parse_id("claude"), Some(ToolId::Claude));
        assert_eq!(ToolId::parse_id("cursor"), Some(ToolId::Cursor));
        assert_eq!(ToolId::parse_id("unknown"), None);
    }

    #[test]
    fn test_tool_id_display_name() {
        assert_eq!(ToolId::Claude.display_name(), "Claude Code");
        assert_eq!(ToolId::GitHubCopilot.display_name(), "GitHub Copilot");
    }

    #[test]
    fn test_config_level_as_str() {
        assert_eq!(ConfigLevel::User.as_str(), "user");
        assert_eq!(ConfigLevel::Project.as_str(), "project");
    }

    #[test]
    fn test_transport_type_as_str() {
        assert_eq!(TransportType::Stdio.as_str(), "stdio");
        assert_eq!(TransportType::Sse.as_str(), "sse");
        assert_eq!(TransportType::Http.as_str(), "http");
    }

    #[test]
    fn test_discovered_item_type_name() {
        let mcp_server = DiscoveredItem::MCPServer(DiscoveredMCPServer {
            type_name: "mcp-server",
            name: "test".to_string(),
            command: None,
            args: None,
            env: None,
            url: None,
            transport: None,
            source: ConfigSource {
                tool: ToolId::Claude,
                tool_name: "Claude Code".to_string(),
                path: "/test".to_string(),
                level: ConfigLevel::User,
            },
        });

        assert_eq!(mcp_server.type_name(), "mcp-server");
        assert_eq!(mcp_server.name(), "test");
    }

    #[test]
    fn test_tool_id_roundtrip() {
        for tool_id in [
            ToolId::Claude,
            ToolId::Cursor,
            ToolId::Windsurf,
            ToolId::Gemini,
            ToolId::Codex,
            ToolId::Cline,
            ToolId::GitHubCopilot,
            ToolId::VSCode,
        ] {
            let s = tool_id.as_str();
            assert_eq!(ToolId::parse_id(s), Some(tool_id));
        }
    }

    #[test]
    fn test_discovery_summary_default() {
        let summary = DiscoverySummary::default();
        assert_eq!(summary.mcp_servers, 0);
        assert_eq!(summary.total_items, 0);
    }
}
