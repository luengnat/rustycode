//! Secrets manifest parser
//!
//! Parses SECRETS.md files containing:
//! - Milestone secrets tracking
//! - Service dashboard URLs
//! - Collection status and guidance

use crate::files::parsers::common::{extract_all_sections, extract_bold_field};

/// Valid secrets manifest entry statuses
pub const VALID_STATUSES: &[&str] = &["pending", "collected", "skipped"];

/// Secrets manifest structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecretsManifest {
    pub milestone: String,
    pub generated_at: String,
    pub entries: Vec<SecretsManifestEntry>,
}

/// Secrets manifest entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecretsManifestEntry {
    pub key: String,
    pub service: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dashboard_url: String,
    pub guidance: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub format_hint: String,
    #[serde(default = "default_secret_status")]
    pub status: String,
    #[serde(default = "default_destination")]
    pub destination: String,
}

fn default_secret_status() -> String {
    "pending".to_string()
}

fn default_destination() -> String {
    "dotenv".to_string()
}

/// Parse secrets manifest
pub fn parse_secrets_manifest(content: &str) -> SecretsManifest {
    let milestone = extract_bold_field(content, "Milestone").unwrap_or_default();
    let generated_at = extract_bold_field(content, "Generated").unwrap_or_default();

    let h3_sections = extract_all_sections(content, 3);
    let mut entries = Vec::new();

    for (heading, section_content) in h3_sections {
        let key = heading.trim().to_string();
        if key.is_empty() {
            continue;
        }

        let service = extract_bold_field(&section_content, "Service").unwrap_or_default();
        let dashboard_url = extract_bold_field(&section_content, "Dashboard").unwrap_or_default();
        let format_hint = extract_bold_field(&section_content, "Format hint").unwrap_or_default();
        let raw_status = extract_bold_field(&section_content, "Status")
            .unwrap_or_else(|| "pending".to_string())
            .to_lowercase();
        let status = if VALID_STATUSES.contains(&raw_status.as_str()) {
            raw_status
        } else {
            "pending".to_string()
        };
        let destination = extract_bold_field(&section_content, "Destination")
            .unwrap_or_else(|| "dotenv".to_string());

        // Extract numbered guidance list
        let guidance = extract_numbered_guidance(&section_content);

        entries.push(SecretsManifestEntry {
            key,
            service,
            dashboard_url,
            guidance,
            format_hint,
            status,
            destination,
        });
    }

    SecretsManifest {
        milestone,
        generated_at,
        entries,
    }
}

fn extract_numbered_guidance(section: &str) -> Vec<String> {
    let mut guidance = Vec::new();
    let num_re = regex::Regex::new(r"^\s*\d+\.\s+(.+)").unwrap();

    for line in section.lines() {
        if let Some(caps) = num_re.captures(line) {
            guidance.push(
                caps.get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            );
        }
    }

    guidance
}

/// Format secrets manifest
pub fn format_secrets_manifest(manifest: &SecretsManifest) -> String {
    let mut lines = Vec::new();

    lines.push("# Secrets Manifest".to_string());
    lines.push(String::new());
    lines.push(format!("**Milestone:** {}", manifest.milestone));
    lines.push(format!("**Generated:** {}", manifest.generated_at));

    for entry in &manifest.entries {
        lines.push(String::new());
        lines.push(format!("### {}", entry.key));
        lines.push(String::new());
        lines.push(format!("**Service:** {}", entry.service));
        if !entry.dashboard_url.is_empty() {
            lines.push(format!("**Dashboard:** {}", entry.dashboard_url));
        }
        if !entry.format_hint.is_empty() {
            lines.push(format!("**Format hint:** {}", entry.format_hint));
        }
        lines.push(format!("**Status:** {}", entry.status));
        lines.push(format!("**Destination:** {}", entry.destination));
        lines.push(String::new());
        for (i, guidance_item) in entry.guidance.iter().enumerate() {
            lines.push(format!("{}. {}", i + 1, guidance_item));
        }
    }

    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_secrets_manifest_basic() {
        let content = r#"# Secrets Manifest

**Milestone:** M001

### API_KEY

**Service:** API

**Status:** pending
"#;

        let manifest = parse_secrets_manifest(content);

        assert_eq!(manifest.milestone, "M001");
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].key, "API_KEY");
        assert_eq!(manifest.entries[0].service, "API");
        assert_eq!(manifest.entries[0].status, "pending");
    }

    #[test]
    fn test_format_secrets_manifest() {
        let manifest = SecretsManifest {
            milestone: "M001".to_string(),
            generated_at: "2025-03-18".to_string(),
            entries: vec![SecretsManifestEntry {
                key: "API_KEY".to_string(),
                service: "API".to_string(),
                dashboard_url: String::new(),
                guidance: vec!["Get key from dashboard".to_string()],
                format_hint: String::new(),
                status: "pending".to_string(),
                destination: "dotenv".to_string(),
            }],
        };

        let formatted = format_secrets_manifest(&manifest);

        assert!(formatted.contains("M001"));
        assert!(formatted.contains("API_KEY"));
    }

    // --- Serde roundtrips ---

    #[test]
    fn secrets_manifest_serde_roundtrip() {
        let manifest = SecretsManifest {
            milestone: "M01".into(),
            generated_at: "2025-01-01".into(),
            entries: vec![SecretsManifestEntry {
                key: "DB_URL".into(),
                service: "PostgreSQL".into(),
                dashboard_url: "https://db.example.com".into(),
                guidance: vec!["Find in dashboard".into()],
                format_hint: "postgres://...".into(),
                status: "collected".into(),
                destination: "dotenv".into(),
            }],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: SecretsManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.milestone, "M01");
        assert_eq!(decoded.entries.len(), 1);
        assert_eq!(decoded.entries[0].status, "collected");
    }

    #[test]
    fn secrets_manifest_entry_skip_empty_fields() {
        let entry = SecretsManifestEntry {
            key: "K".into(),
            service: "S".into(),
            dashboard_url: String::new(),
            guidance: vec![],
            format_hint: String::new(),
            status: "pending".into(),
            destination: "dotenv".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("dashboard_url"));
        assert!(!json.contains("format_hint"));
    }

    #[test]
    fn secrets_manifest_entry_default_status() {
        let json = r#"{"key":"K","service":"S","dashboard_url":"","guidance":[],"format_hint":""}"#;
        let entry: SecretsManifestEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.status, "pending");
        assert_eq!(entry.destination, "dotenv");
    }

    // --- Parse edge cases ---

    #[test]
    fn parse_secrets_manifest_empty() {
        let manifest = parse_secrets_manifest("");
        assert!(manifest.milestone.is_empty());
        assert!(manifest.entries.is_empty());
    }

    #[test]
    fn parse_secrets_manifest_with_guidance() {
        let content = r#"# Secrets Manifest

**Milestone:** M02

### SECRET_TOKEN

**Service:** Auth

1. Go to settings
2. Copy the token
3. Paste here
"#;
        let manifest = parse_secrets_manifest(content);
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].guidance.len(), 3);
        assert_eq!(manifest.entries[0].guidance[0], "Go to settings");
    }

    #[test]
    fn parse_secrets_manifest_invalid_status_defaults_to_pending() {
        let content = r#"# Secrets Manifest

**Milestone:** M03

### KEY

**Service:** Svc

**Status:** bogus_status
"#;
        let manifest = parse_secrets_manifest(content);
        assert_eq!(manifest.entries[0].status, "pending");
    }

    #[test]
    fn parse_secrets_manifest_collected_status() {
        let content = r#"# Secrets Manifest

**Milestone:** M03

### KEY

**Service:** Svc

**Status:** collected
"#;
        let manifest = parse_secrets_manifest(content);
        assert_eq!(manifest.entries[0].status, "collected");
    }

    #[test]
    fn parse_secrets_manifest_skipped_status() {
        let content = r#"# Secrets Manifest

**Milestone:** M03

### KEY

**Service:** Svc

**Status:** skipped
"#;
        let manifest = parse_secrets_manifest(content);
        assert_eq!(manifest.entries[0].status, "skipped");
    }

    #[test]
    fn parse_secrets_manifest_with_dashboard_and_format() {
        let content = r#"# Secrets Manifest

**Milestone:** M04

### API_KEY

**Service:** Cloud

**Dashboard:** https://cloud.example.com

**Format hint:** base64-encoded

**Status:** pending

**Destination:** vault
"#;
        let manifest = parse_secrets_manifest(content);
        assert_eq!(
            manifest.entries[0].dashboard_url,
            "https://cloud.example.com"
        );
        assert_eq!(manifest.entries[0].format_hint, "base64-encoded");
        assert_eq!(manifest.entries[0].destination, "vault");
    }

    #[test]
    fn format_secrets_manifest_roundtrip() {
        let manifest = SecretsManifest {
            milestone: "M05".into(),
            generated_at: "2025-06-01".into(),
            entries: vec![
                SecretsManifestEntry {
                    key: "A".into(),
                    service: "SvcA".into(),
                    dashboard_url: String::new(),
                    guidance: vec!["Step 1".into()],
                    format_hint: String::new(),
                    status: "pending".into(),
                    destination: "dotenv".into(),
                },
                SecretsManifestEntry {
                    key: "B".into(),
                    service: "SvcB".into(),
                    dashboard_url: "https://b.example.com".into(),
                    guidance: vec![],
                    format_hint: "uuid".into(),
                    status: "collected".into(),
                    destination: "env".into(),
                },
            ],
        };
        let formatted = format_secrets_manifest(&manifest);
        assert!(formatted.contains("### A"));
        assert!(formatted.contains("### B"));
        assert!(formatted.contains("1. Step 1"));
        assert!(formatted.contains("https://b.example.com"));
    }

    #[test]
    fn valid_statuses_contains_expected() {
        assert!(VALID_STATUSES.contains(&"pending"));
        assert!(VALID_STATUSES.contains(&"collected"));
        assert!(VALID_STATUSES.contains(&"skipped"));
        assert_eq!(VALID_STATUSES.len(), 3);
    }
}
