//! Roadmap file parser
//!
//! Parses ROADMAP.md files containing:
//! - Milestone vision and success criteria
//! - Slice definitions with objectives
//! - Boundary map between slices

use crate::files::parsers::common::{
    extract_all_sections, extract_bold_field, extract_section, parse_bullets,
};
use indexmap::IndexMap;
use regex::Regex;
use std::sync::LazyLock;

static ARROW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\S+)\s*→\s*(\S+)").unwrap());
static PRODUCES_INLINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Produces:\s*(.+)$").unwrap());
static PRODUCES_MULTI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Produces:\s*$").unwrap());
static CONSUMES_INLINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Consumes[^:]*:\s*(.+)$").unwrap());
static CONSUMES_MULTI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Consumes[^:]*:\s*$").unwrap());

/// Roadmap structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Roadmap {
    pub title: String,
    pub vision: String,
    pub success_criteria: Vec<String>,
    pub slices: Vec<RoadmapSlice>,
    pub boundary_map: Vec<BoundaryMapEntry>,
}

/// Roadmap slice entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoadmapSlice {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: String,
}

/// Boundary map entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoundaryMapEntry {
    pub from_slice: String,
    pub to_slice: String,
    pub produces: String,
    pub consumes: String,
}

/// Parse roadmap content
pub fn parse_roadmap(content: &str) -> Roadmap {
    let lines: Vec<&str> = content.lines().collect();

    let h1 = lines.iter().find(|l| l.starts_with("# "));
    let title = h1.map(|s| &s[2..]).unwrap_or("").trim().to_string();
    let vision = extract_bold_field(content, "Vision").unwrap_or_default();

    let sc_section = extract_section(content, "Success Criteria", 2).unwrap_or_default();
    let success_criteria = if !sc_section.is_empty() {
        parse_bullets(&sc_section)
    } else {
        Vec::new()
    };

    // Parse slices
    let slices = parse_roadmap_slices(content);

    // Parse boundary map
    let boundary_map = parse_boundary_map(content);

    Roadmap {
        title,
        vision,
        success_criteria,
        slices,
        boundary_map,
    }
}

fn parse_boundary_map(content: &str) -> Vec<BoundaryMapEntry> {
    let mut boundary_map = Vec::new();

    if let Some(bm_section) = extract_section(content, "Boundary Map", 2) {
        let h3_sections: IndexMap<String, String> = extract_all_sections(&bm_section, 3);

        for (heading, section_content) in h3_sections {
            if let Some(caps) = ARROW_RE.captures(&heading) {
                let from_slice = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                let to_slice = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();

                let produces = extract_produces(&section_content);
                let consumes = extract_consumes(&section_content);

                boundary_map.push(BoundaryMapEntry {
                    from_slice,
                    to_slice,
                    produces,
                    consumes,
                });
            }
        }
    }

    boundary_map
}

/// Parse slices from roadmap content
fn parse_roadmap_slices(content: &str) -> Vec<RoadmapSlice> {
    let mut slices = Vec::new();

    // Find the Slices section
    if let Some(slices_section) = extract_section(content, "Slices", 2) {
        // Extract subsections (each slice is an H3)
        let h3_sections = extract_all_sections(&slices_section, 3);

        for (heading, section_content) in h3_sections {
            // Parse heading for ID and title
            let heading_parts: Vec<&str> = heading.trim().splitn(2, ' ').collect();
            let id = heading_parts
                .first()
                .map(|s| s.trim().trim_end_matches(':'))
                .unwrap_or("S??")
                .to_string();
            let title = heading_parts
                .get(1)
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    heading
                        .clone()
                        .trim_start_matches(&format!("{}:", id))
                        .trim()
                        .to_string()
                });

            // Extract objective (H4 within H3 slice section)
            let objective = extract_section(&section_content, "Objective", 4)
                .or_else(|| extract_section(&section_content, "Goal", 4))
                .unwrap_or_default();

            // Extract status (H4 within H3 slice section, default to Pending)
            let status =
                if let Some(status_section) = extract_section(&section_content, "Status", 4) {
                    status_section
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else {
                    "Pending".to_string()
                };

            slices.push(RoadmapSlice {
                id,
                title,
                objective,
                status,
            });
        }
    }

    slices
}

fn extract_produces(section: &str) -> String {
    // First try inline format: "Produces: value"
    if let Some(caps) = PRODUCES_INLINE_RE.captures(section) {
        return caps
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
    }

    // Then try multiline format: "Produces:" on its own line
    if let Some(mat) = PRODUCES_MULTI_RE.find(section) {
        let after_prod = &section[mat.end()..];
        let cons_idx = after_prod.find("Consumes").unwrap_or(after_prod.len());
        return after_prod[..cons_idx].trim().to_string();
    }

    String::new()
}

fn extract_consumes(section: &str) -> String {
    // First try inline format: "Consumes: value"
    if let Some(caps) = CONSUMES_INLINE_RE.captures(section) {
        return caps
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
    }

    // Then try multiline format: "Consumes:" on its own line
    if let Some(mat) = CONSUMES_MULTI_RE.find(section) {
        return section[mat.end()..].trim().to_string();
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_roadmap_basic() {
        let content = r#"# My Roadmap

**Vision:** Build great things

## Success Criteria

- First criterion
- Second criterion

## Slices

### S01: First Slice

#### Objective
Build the first thing

#### Status
Pending

### S02: Second Slice

#### Goal
Build the second thing

#### Status
Done

## Boundary Map

### S01 → S02

Produces:
Output from S01

Consumes:
Input for S02
"#;

        let roadmap = parse_roadmap(content);

        assert_eq!(roadmap.title, "My Roadmap");
        assert_eq!(roadmap.vision, "Build great things");
        assert_eq!(roadmap.success_criteria.len(), 2);
        assert_eq!(roadmap.slices.len(), 2);
        assert_eq!(roadmap.slices[0].id, "S01");
        assert_eq!(roadmap.slices[0].title, "First Slice");
        assert_eq!(roadmap.slices[0].status, "Pending");
        assert_eq!(roadmap.slices[1].id, "S02");
        assert_eq!(roadmap.boundary_map.len(), 1);
        assert_eq!(roadmap.boundary_map[0].from_slice, "S01");
        assert_eq!(roadmap.boundary_map[0].to_slice, "S02");
    }

    #[test]
    fn test_parse_roadmap_with_colon_in_id() {
        let content = r#"# Test

## Slices

### S01: Title With Spaces

#### Objective
Test objective

#### Status
In Progress
"#;

        let roadmap = parse_roadmap(content);

        assert_eq!(roadmap.slices.len(), 1);
        assert_eq!(roadmap.slices[0].id, "S01");
        assert_eq!(roadmap.slices[0].title, "Title With Spaces");
        assert_eq!(roadmap.slices[0].status, "In Progress");
    }

    #[test]
    fn test_parse_boundary_map_with_consumes_inline() {
        let content = r#"# Test

## Boundary Map

### S01 → S02

Produces: Artifact A

Consumes: Dependency B
"#;

        let roadmap = parse_roadmap(content);

        assert_eq!(roadmap.boundary_map.len(), 1);
        assert_eq!(roadmap.boundary_map[0].produces, "Artifact A");
        assert_eq!(roadmap.boundary_map[0].consumes, "Dependency B");
    }

    // --- Serde roundtrips ---

    #[test]
    fn roadmap_serde_roundtrip() {
        let r = Roadmap {
            title: "My Roadmap".into(),
            vision: "Build things".into(),
            success_criteria: vec!["All tests pass".into()],
            slices: vec![RoadmapSlice {
                id: "S01".into(),
                title: "First".into(),
                objective: "Do stuff".into(),
                status: "Done".into(),
            }],
            boundary_map: vec![BoundaryMapEntry {
                from_slice: "S01".into(),
                to_slice: "S02".into(),
                produces: "artifact".into(),
                consumes: "dep".into(),
            }],
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: Roadmap = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.title, "My Roadmap");
        assert_eq!(decoded.slices.len(), 1);
        assert_eq!(decoded.boundary_map[0].from_slice, "S01");
    }

    #[test]
    fn roadmap_slice_serde_roundtrip() {
        let s = RoadmapSlice {
            id: "S05".into(),
            title: "Auth Module".into(),
            objective: "Add auth".into(),
            status: "In Progress".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: RoadmapSlice = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "S05");
        assert_eq!(decoded.status, "In Progress");
    }

    #[test]
    fn boundary_map_entry_serde_roundtrip() {
        let e = BoundaryMapEntry {
            from_slice: "S01".into(),
            to_slice: "S02".into(),
            produces: "API layer".into(),
            consumes: "data model".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let decoded: BoundaryMapEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.produces, "API layer");
    }

    // --- Parse edge cases ---

    #[test]
    fn parse_roadmap_empty() {
        let roadmap = parse_roadmap("");
        assert!(roadmap.title.is_empty());
        assert!(roadmap.slices.is_empty());
    }

    #[test]
    fn parse_roadmap_no_slices() {
        let content = "# Just a Title\n\nSome text\n";
        let roadmap = parse_roadmap(content);
        assert_eq!(roadmap.title, "Just a Title");
        assert!(roadmap.slices.is_empty());
        assert!(roadmap.boundary_map.is_empty());
    }
}
