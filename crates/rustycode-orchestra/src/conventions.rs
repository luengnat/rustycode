//! Agent Convention Discovery
//!
//! Provides utilities for discovering and loading AGENTS.md or CLAUDE.md files
//! to provide context-aware instructions to AI coding agents.

use std::path::{Path, PathBuf};
use std::fs;

/// Discovers the nearest AGENTS.md, or CLAUDE.md, scanning upward from the given path.
pub fn find_convention_file(start_path: &Path) -> Option<PathBuf> {
    let mut current = start_path;
    loop {
        let agents_md = current.join("AGENTS.md");
        if agents_md.exists() {
            return Some(agents_md);
        }
        
        let claude_md = current.join("CLAUDE.md");
        if claude_md.exists() {
            return Some(claude_md);
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    None
}

/// Reads the content of a convention file.
pub fn load_conventions(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}
