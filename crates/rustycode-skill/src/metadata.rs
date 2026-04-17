use rustycode_protocol::frontmatter::{FrontmatterMap, FrontmatterValue};

/// Extract a string value from frontmatter map if present
pub fn extract_string(map: &FrontmatterMap, key: &str) -> Option<String> {
    map.get(key).and_then(|v| {
        if let FrontmatterValue::String(s) = v {
            Some(s.clone())
        } else {
            None
        }
    })
}

/// Extract an array of strings from frontmatter map
pub fn extract_string_array(map: &FrontmatterMap, key: &str) -> Vec<String> {
    if let Some(FrontmatterValue::Array(arr)) = map.get(key) {
        arr.iter()
            .filter_map(|fv| {
                if let FrontmatterValue::String(s) = fv {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Extract a boolean value from frontmatter map with a default
pub fn extract_bool(map: &FrontmatterMap, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| {
            if let FrontmatterValue::Bool(b) = v {
                Some(*b)
            } else {
                None
            }
        })
        .unwrap_or(default)
}

/// Convert a frontmatter map into a lightweight SkillMetadata-friendly tuple
pub fn frontmatter_to_metadata(
    fm: &FrontmatterMap,
) -> (
    Vec<String>,
    Option<crate::EffortLevel>,
    Option<String>,
    bool,
    Vec<String>,
) {
    let allowed_tools = extract_string_array(fm, "allowed-tools");
    let effort = extract_string(fm, "effort").and_then(|s| match s.to_lowercase().as_str() {
        "low" => Some(crate::EffortLevel::Low),
        "medium" => Some(crate::EffortLevel::Medium),
        "high" => Some(crate::EffortLevel::High),
        _ => None,
    });
    let argument_hint = extract_string(fm, "argument-hint");
    let user_invocable = extract_bool(fm, "user-invocable", true);
    let categories = extract_string_array(fm, "categories");
    (
        allowed_tools,
        effort,
        argument_hint,
        user_invocable,
        categories,
    )
}

pub mod __private_dummy {}
