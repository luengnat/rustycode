//! Prediction output in SWE-bench evaluation format

use crate::error::{OrchestraV2Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single prediction produced by RustyCode for a SWE-bench instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub instance_id: String,
    pub model_patch: String,
    pub model_name_or_path: String,
}

/// Write predictions to a JSON file in SWE-bench evaluation format.
///
/// Produces a JSON array (one entry per instance), which is the format
/// expected by the `swebench.harness.run_evaluation` evaluator.
pub fn write_predictions(predictions: &[Prediction], path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(predictions).map_err(|e| {
        OrchestraV2Error::Serialization(format!("Failed to serialize predictions: {}", e))
    })?;

    // Write atomically: write to temp file then rename
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, &json).map_err(|e| OrchestraV2Error::IoError {
        context: format!("Failed to write predictions to {}", temp_path.display()),
        source: e,
    })?;

    std::fs::rename(&temp_path, path).map_err(|e| OrchestraV2Error::IoError {
        context: format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        ),
        source: e,
    })?;

    Ok(())
}

/// Write predictions in JSONL format (one prediction per line).
///
/// Useful for streaming results as instances complete.
pub fn write_predictions_jsonl(predictions: &[Prediction], path: &Path) -> Result<()> {
    let mut lines: Vec<String> = Vec::with_capacity(predictions.len());
    for pred in predictions {
        let line = serde_json::to_string(pred).map_err(|e| {
            OrchestraV2Error::Serialization(format!("Failed to serialize prediction: {}", e))
        })?;
        lines.push(line);
    }

    let content = lines.join("\n") + "\n";

    std::fs::write(path, content).map_err(|e| OrchestraV2Error::IoError {
        context: format!("Failed to write JSONL predictions to {}", path.display()),
        source: e,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CRATE_TEST_LOCK;

    fn make_prediction(id: &str, patch: &str) -> Prediction {
        Prediction {
            instance_id: id.to_string(),
            model_patch: patch.to_string(),
            model_name_or_path: "rustycode-orchestra2".to_string(),
        }
    }

    #[test]
    fn prediction_serialization() {
        let pred = make_prediction("django__12345", "--- a/foo.py\n+++ b/foo.py\n");
        let json = serde_json::to_string(&pred).unwrap();
        assert!(json.contains("\"instance_id\":"));
        assert!(json.contains("\"model_patch\":"));
        assert!(json.contains("\"model_name_or_path\":"));

        let back: Prediction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.instance_id, pred.instance_id);
        assert_eq!(back.model_patch, pred.model_patch);
        assert_eq!(back.model_name_or_path, pred.model_name_or_path);
    }

    #[test]
    fn write_predictions_json_array() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("predictions.json");

        let preds = vec![
            make_prediction("django__001", "patch1"),
            make_prediction("flask__002", "patch2"),
        ];

        write_predictions(&preds, &path).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: Vec<Prediction> = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].instance_id, "django__001");
        assert_eq!(loaded[1].instance_id, "flask__002");
    }

    #[test]
    fn write_predictions_jsonl_format() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("predictions.jsonl");

        let preds = vec![
            make_prediction("django__001", "patch1"),
            make_prediction("flask__002", "patch2"),
        ];

        write_predictions_jsonl(&preds, &path).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        let p1: Prediction = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(p1.instance_id, "django__001");

        let p2: Prediction = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(p2.instance_id, "flask__002");
    }

    #[test]
    fn write_predictions_empty() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("predictions.json");

        write_predictions(&[], &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: Vec<Prediction> = serde_json::from_str(&content).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn write_predictions_atomic_no_leftover_tmp() {
        let _guard = CRATE_TEST_LOCK.lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("predictions.json");

        let preds = vec![make_prediction("test__001", "patch")];
        write_predictions(&preds, &path).unwrap();

        // The temp file should have been renamed, not left behind
        let tmp_path = path.with_extension("tmp");
        assert!(!tmp_path.exists());
        assert!(path.exists());
    }
}
