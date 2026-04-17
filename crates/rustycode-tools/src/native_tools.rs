//! Native tool implementations for cross-platform support.
//!
//! Replaces shell-dependent operations (`ls`, `grep`, `cat`) with native
//! Rust implementations to avoid dependencies on host environment binaries.

use std::fs;
use std::path::Path;
use walkdir::WalkDir;
use regex::Regex;
use anyhow::{Result, Context};

/// Native `cat` implementation
pub fn native_cat(path: &Path) -> Result<String> {
    fs::read_to_string(path).context(format!("Failed to read file: {:?}", path))
}

/// Native `ls` implementation
pub fn native_ls(path: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(path).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        files.push(entry.path().to_string_lossy().into_owned());
    }
    Ok(files)
}

/// Native `grep` implementation
pub fn native_grep(path: &Path, pattern: &str) -> Result<String> {
    let re = Regex::new(pattern).context("Invalid regex pattern")?;
    let content = native_cat(path)?;
    let matches: Vec<String> = content.lines()
        .filter(|line| re.is_match(line))
        .map(|line| line.to_string())
        .collect();
    
    Ok(matches.join("\n"))
}
