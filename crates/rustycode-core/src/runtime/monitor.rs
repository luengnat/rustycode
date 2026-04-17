//! Agent runtime monitoring and repetition detection.
//! Shared logic for all UI modes (Headless, TUI, CLI).

pub struct AgentMonitor {
    pub repetition_check_threshold: usize,
}

impl AgentMonitor {
    pub fn new(threshold: usize) -> Self {
        Self { repetition_check_threshold: threshold }
    }
}

pub fn detect_and_truncate_repeated_blocks(text: &str) -> Option<String> {
    if text.len() < 200 {
        return None;
    }

    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 6 {
        return None; 
    }

    for block_size in 3..=8.min(paragraphs.len() / 2) {
        let first_block: Vec<&str> = paragraphs[..block_size].to_vec();
        let first_block_text = first_block.join("\n\n");

        if first_block_text.len() < 100 {
            continue;
        }

        let mut repetitions = 1;
        let mut pos = block_size;

        while pos + block_size <= paragraphs.len() {
            let candidate: Vec<&str> = paragraphs[pos..pos + block_size].to_vec();
            let candidate_text = candidate.join("\n\n");

            if blocks_match(&first_block_text, &candidate_text) {
                repetitions += 1;
                pos += block_size;
            } else {
                break;
            }
        }

        if repetitions >= 3 {
            let end_of_repetitions = block_size * repetitions;
            let mut result_parts: Vec<&str> = paragraphs[..block_size].to_vec();

            if end_of_repetitions < paragraphs.len() {
                result_parts.extend_from_slice(&paragraphs[end_of_repetitions..]);
            }

            return Some(result_parts.join("\n\n"));
        }
    }

    None
}

fn blocks_match(a: &str, b: &str) -> bool {
    let a_normalized: String = a.chars().filter(|c| !c.is_whitespace()).collect();
    let b_normalized: String = b.chars().filter(|c| !c.is_whitespace()).collect();
    if a_normalized.len() < 50 { return false; }
    if a_normalized == b_normalized { return true; }
    let min_len = a_normalized.len().min(b_normalized.len());
    if min_len < 50 { return false; }
    let check_len = (min_len as f64 * 0.8) as usize;
    if check_len < 50 { return false; }
    let matching: usize = a_normalized[..check_len]
        .chars()
        .zip(b_normalized[..check_len].chars())
        .map(|(a, b)| if a == b { 1 } else { 0 })
        .sum();
    (matching as f64 / check_len as f64) > 0.9
}

pub fn strip_repeated_preamble_phrases(text: &str) -> String {
    let sentences: Vec<&str> = text
        .split_inclusive(['.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.len() < 3 {
        return text.to_string();
    }

    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for s in &sentences {
        *counts.entry(s).or_insert(0) += 1;
    }

    let repeated: std::collections::HashSet<&&str> = counts
        .iter()
        .filter(|(_, &c)| c >= 3)
        .map(|(s, _)| s)
        .collect();

    if repeated.is_empty() {
        return text.to_string();
    }

    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut result = String::new();
    for s in &sentences {
        if repeated.contains(&s) {
            if !seen.contains(s) {
                seen.insert(s);
                result.push_str(s);
                result.push(' ');
            }
        } else {
            result.push_str(s);
        }
    }

    result.trim().to_string()
}
