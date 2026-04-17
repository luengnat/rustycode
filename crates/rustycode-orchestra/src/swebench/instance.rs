//! SWE-bench instance loader -- parses the standard JSON format

use crate::error::{OrchestraV2Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single SWE-bench task instance (from the official dataset)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweBenchInstance {
    pub instance_id: String,
    pub repo: String,
    pub version: String,
    pub base_commit: String,
    pub problem_statement: String,
    pub hints_text: Option<String>,
    pub created_at: String,
    pub test_patch: String,
    pub patch: String,
    #[serde(rename = "FAIL_TO_PASS")]
    pub fail_to_pass: Vec<String>,
    #[serde(rename = "PASS_TO_PASS")]
    pub pass_to_pass: Vec<String>,
}

/// Load instances from a JSON file (SWE-bench format).
///
/// Accepts both JSON arrays (standard) and JSONL (one object per line).
pub fn load_instances(path: &Path) -> Result<Vec<SweBenchInstance>> {
    let content = std::fs::read_to_string(path).map_err(|e| OrchestraV2Error::IoError {
        context: format!("Failed to read SWE-bench instances from {}", path.display()),
        source: e,
    })?;

    let trimmed = content.trim();

    // Try JSON array first (the standard format)
    if trimmed.starts_with('[') {
        let instances: Vec<SweBenchInstance> = serde_json::from_str(trimmed).map_err(|e| {
            OrchestraV2Error::Parse(format!(
                "Failed to parse SWE-bench JSON array from {}: {}",
                path.display(),
                e
            ))
        })?;
        return Ok(instances);
    }

    // Fall back to JSONL (one JSON object per line)
    let mut instances = Vec::new();
    for (line_num, line) in trimmed.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let instance: SweBenchInstance = serde_json::from_str(line).map_err(|e| {
            OrchestraV2Error::Parse(format!(
                "Failed to parse SWE-bench instance at line {} in {}: {}",
                line_num + 1,
                path.display(),
                e
            ))
        })?;
        instances.push(instance);
    }

    Ok(instances)
}

/// Load a single instance by ID from a JSON file
pub fn load_instance_by_id(path: &Path, instance_id: &str) -> Result<Option<SweBenchInstance>> {
    let instances = load_instances(path)?;
    Ok(instances.into_iter().find(|i| i.instance_id == instance_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CRATE_TEST_LOCK;
    use std::io::Write;

    fn make_test_instance(id: &str) -> SweBenchInstance {
        SweBenchInstance {
            instance_id: id.to_string(),
            repo: "django/django".to_string(),
            version: "3.0".to_string(),
            base_commit: "abc123".to_string(),
            problem_statement: "Fix the bug".to_string(),
            hints_text: None,
            created_at: "2024-01-01T00:00:00".to_string(),
            test_patch: "".to_string(),
            patch: "".to_string(),
            fail_to_pass: vec!["test_foo".to_string()],
            pass_to_pass: vec!["test_bar".to_string()],
        }
    }

    #[test]
    fn load_instances_json_array() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instances.json");

        let instances = vec![
            make_test_instance("django__12345"),
            make_test_instance("flask__67890"),
        ];
        let json = serde_json::to_string_pretty(&instances).unwrap();
        std::fs::write(&path, json).unwrap();

        let loaded = load_instances(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].instance_id, "django__12345");
        assert_eq!(loaded[1].instance_id, "flask__67890");
    }

    #[test]
    fn load_instances_jsonl() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instances.jsonl");

        let i1 = make_test_instance("django__111");
        let i2 = make_test_instance("flask__222");

        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "{}", serde_json::to_string(&i1).unwrap()).unwrap();
        writeln!(file, "{}", serde_json::to_string(&i2).unwrap()).unwrap();

        let loaded = load_instances(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].instance_id, "django__111");
        assert_eq!(loaded[1].instance_id, "flask__222");
    }

    #[test]
    fn load_instances_empty_file() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "[]").unwrap();

        let loaded = load_instances(&path).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_instance_by_id_found() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instances.json");

        let instances = vec![
            make_test_instance("django__12345"),
            make_test_instance("flask__67890"),
        ];
        std::fs::write(&path, serde_json::to_string(&instances).unwrap()).unwrap();

        let found = load_instance_by_id(&path, "flask__67890").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().instance_id, "flask__67890");
    }

    #[test]
    fn load_instance_by_id_not_found() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instances.json");

        let instances = vec![make_test_instance("django__12345")];
        std::fs::write(&path, serde_json::to_string(&instances).unwrap()).unwrap();

        let found = load_instance_by_id(&path, "nonexistent").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn load_instances_missing_file() {
        let _guard = CRATE_TEST_LOCK.lock();
        let result = load_instances(Path::new("/nonexistent/path.json"));
        assert!(result.is_err());
    }

    #[test]
    fn load_instances_invalid_json() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json at all").unwrap();

        let result = load_instances(&path);
        assert!(result.is_err());
    }

    #[test]
    fn instance_fields_deserialize() {
        let _guard = CRATE_TEST_LOCK.lock();
        let json = r#"{
            "instance_id": "test__001",
            "repo": "test/repo",
            "version": "1.0",
            "base_commit": "deadbeef",
            "problem_statement": "Something is broken",
            "hints_text": "Check the foo module",
            "created_at": "2024-06-15T12:00:00",
            "test_patch": "--- a/test.py\n+++ b/test.py\n",
            "patch": "--- a/src.py\n+++ b/src.py\n",
            "FAIL_TO_PASS": ["test_a", "test_b"],
            "PASS_TO_PASS": ["test_c"]
        }"#;

        let instance: SweBenchInstance = serde_json::from_str(json).unwrap();
        assert_eq!(instance.instance_id, "test__001");
        assert_eq!(instance.repo, "test/repo");
        assert_eq!(instance.fail_to_pass, vec!["test_a", "test_b"]);
        assert_eq!(instance.pass_to_pass, vec!["test_c"]);
        assert_eq!(
            instance.hints_text,
            Some("Check the foo module".to_string())
        );
    }
}
