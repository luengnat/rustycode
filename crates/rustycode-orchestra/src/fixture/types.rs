// Fixture data types for recording and replay

use serde::{Deserialize, Serialize};

/// A complete fixture recording
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureRecording {
    /// Unique name for this fixture
    pub name: String,

    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Conversation turns
    pub turns: Vec<FixtureTurn>,

    /// Files referenced in fixture (for setup/assertions)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FixtureFile>,
}

/// A single turn in a recorded conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureTurn {
    /// Role in the conversation
    pub role: FixtureRole,

    /// Text content of the message
    pub content: String,

    /// Tool uses in this turn (for assistant turns)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_uses: Vec<FixtureToolUse>,
}

/// Role in a conversation turn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum FixtureRole {
    User,
    Assistant,
}

/// A single tool use within a conversation turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureToolUse {
    /// Name of the tool used
    pub name: String,

    /// Input parameters to the tool
    pub input: serde_json::Value,

    /// Output from the tool (for replay)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Error from the tool (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A file referenced in a fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureFile {
    /// Path to the file (relative to project root)
    pub path: String,

    /// Content of the file
    pub content: String,
}

/// Streaming event from a fixture replay
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FixtureEvent {
    /// Content block start
    ContentBlockStart { index: usize },

    /// Content delta
    ContentDelta { delta: String },

    /// Tool use start
    ToolUseStart {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool use result
    ToolUseResult {
        id: String,
        output: Option<String>,
        error: Option<String>,
    },

    /// Content block stop
    ContentBlockStop { index: usize },

    /// End of message
    MessageStop,
}

/// Error types for fixture operations
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FixtureError {
    #[error("Fixture not found: {0}")]
    NotFound(String),

    #[error("Failed to parse fixture JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("No more turns in fixture")]
    NoMoreTurns,

    #[error("Fixture replay out of sync - expected {expected}, got {actual}")]
    OutOfSync { expected: String, actual: String },
}

/// Helper to create a simple fixture recording
impl FixtureRecording {
    /// Create a new empty fixture recording
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            turns: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a user turn
    pub fn add_user_turn(mut self, content: impl Into<String>) -> Self {
        self.turns.push(FixtureTurn {
            role: FixtureRole::User,
            content: content.into(),
            tool_uses: Vec::new(),
        });
        self
    }

    /// Add an assistant turn
    pub fn add_assistant_turn(
        mut self,
        content: impl Into<String>,
        tool_uses: Vec<FixtureToolUse>,
    ) -> Self {
        self.turns.push(FixtureTurn {
            role: FixtureRole::Assistant,
            content: content.into(),
            tool_uses,
        });
        self
    }

    /// Add a file
    pub fn add_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.push(FixtureFile {
            path: path.into(),
            content: content.into(),
        });
        self
    }

    /// Load from JSON file
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, FixtureError> {
        let content = std::fs::read_to_string(path.as_ref())?;
        let recording: Self = serde_json::from_str(&content)?;
        Ok(recording)
    }

    /// Save to JSON file
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), FixtureError> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path.as_ref(), content)?;
        Ok(())
    }
}

/// Helper to create tool uses
impl FixtureToolUse {
    /// Create a new tool use
    pub fn new(name: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            input,
            output: None,
            error: None,
        }
    }

    /// Set the output
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Set an error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_recording_builder() {
        let recording = FixtureRecording::new("test-fixture")
            .with_description("A test fixture")
            .add_user_turn("Create a file")
            .add_assistant_turn(
                "I'll create that file",
                vec![FixtureToolUse::new(
                    "write_file",
                    serde_json::json!({"path": "test.txt", "content": "hello"}),
                )
                .with_output("File created")],
            );

        assert_eq!(recording.name, "test-fixture");
        assert_eq!(recording.turns.len(), 2);
        assert_eq!(recording.turns[0].role, FixtureRole::User);
        assert_eq!(recording.turns[1].role, FixtureRole::Assistant);
        assert_eq!(recording.turns[1].tool_uses.len(), 1);
    }

    #[test]
    fn test_fixture_serialization() {
        let recording = FixtureRecording::new("test")
            .add_user_turn("hello")
            .add_assistant_turn("hi there", vec![]);

        let json = serde_json::to_string_pretty(&recording).unwrap();
        let parsed: FixtureRecording = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.turns.len(), 2);
    }

    // --- FixtureRole serde ---

    #[test]
    fn fixture_role_serde_variants() {
        assert_eq!(
            serde_json::to_string(&FixtureRole::User).unwrap(),
            "\"user\""
        );
        assert_eq!(
            serde_json::to_string(&FixtureRole::Assistant).unwrap(),
            "\"assistant\""
        );
        let d: FixtureRole = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(d, FixtureRole::User);
    }

    // --- FixtureToolUse ---

    #[test]
    fn fixture_tool_use_new() {
        let tu = FixtureToolUse::new("bash", serde_json::json!({"cmd": "ls"}));
        assert_eq!(tu.name, "bash");
        assert!(tu.output.is_none());
        assert!(tu.error.is_none());
    }

    #[test]
    fn fixture_tool_use_with_output() {
        let tu = FixtureToolUse::new("bash", serde_json::json!({})).with_output("file.txt");
        assert_eq!(tu.output, Some("file.txt".into()));
    }

    #[test]
    fn fixture_tool_use_with_error() {
        let tu = FixtureToolUse::new("bash", serde_json::json!({})).with_error("permission denied");
        assert_eq!(tu.error, Some("permission denied".into()));
    }

    #[test]
    fn fixture_tool_use_serde_roundtrip() {
        let tu = FixtureToolUse::new("read_file", serde_json::json!({"path": "/tmp/a"}))
            .with_output("contents");
        let json = serde_json::to_string(&tu).unwrap();
        let decoded: FixtureToolUse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "read_file");
        assert_eq!(decoded.output, Some("contents".into()));
    }

    // --- FixtureFile serde ---

    #[test]
    fn fixture_file_serde_roundtrip() {
        let ff = FixtureFile {
            path: "src/main.rs".into(),
            content: "fn main() {}".into(),
        };
        let json = serde_json::to_string(&ff).unwrap();
        let decoded: FixtureFile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.path, "src/main.rs");
    }

    // --- FixtureRecording with files and save/load ---

    #[test]
    fn fixture_recording_add_file() {
        let rec = FixtureRecording::new("f1")
            .add_file("a.txt", "hello")
            .add_file("b.txt", "world");
        assert_eq!(rec.files.len(), 2);
        assert_eq!(rec.files[0].path, "a.txt");
    }

    #[test]
    fn fixture_recording_save_load_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fixture.json");

        let rec = FixtureRecording::new("roundtrip")
            .with_description("test desc")
            .add_user_turn("hi")
            .add_assistant_turn("hello", vec![])
            .add_file("f.txt", "data");

        rec.save(&path).unwrap();
        let loaded = FixtureRecording::load(&path).unwrap();

        assert_eq!(loaded.name, "roundtrip");
        assert_eq!(loaded.description, Some("test desc".into()));
        assert_eq!(loaded.turns.len(), 2);
        assert_eq!(loaded.files.len(), 1);
    }

    #[test]
    fn fixture_recording_skip_serializing_if() {
        let rec = FixtureRecording::new("minimal").add_user_turn("hi");
        let json = serde_json::to_string(&rec).unwrap();
        // description is None, should not appear
        assert!(!json.contains("description"));
        // files is empty, should not appear
        assert!(!json.contains("files"));
    }

    // --- FixtureError display ---

    #[test]
    fn fixture_error_display() {
        let e = FixtureError::NotFound("test.json".into());
        assert!(format!("{}", e).contains("test.json"));

        let e2 = FixtureError::NoMoreTurns;
        assert!(format!("{}", e2).contains("No more turns"));

        let e3 = FixtureError::OutOfSync {
            expected: "user".into(),
            actual: "assistant".into(),
        };
        let s = format!("{}", e3);
        assert!(s.contains("user"));
        assert!(s.contains("assistant"));
    }

    // --- FixtureEvent variants ---

    #[test]
    fn fixture_event_variants() {
        let e1 = FixtureEvent::ContentBlockStart { index: 0 };
        let e2 = FixtureEvent::ContentDelta {
            delta: "hello".into(),
        };
        let e3 = FixtureEvent::ToolUseStart {
            id: "t1".into(),
            name: "bash".into(),
            input: serde_json::json!({}),
        };
        let e4 = FixtureEvent::ToolUseResult {
            id: "t1".into(),
            output: Some("ok".into()),
            error: None,
        };
        let e5 = FixtureEvent::ContentBlockStop { index: 0 };
        let e6 = FixtureEvent::MessageStop;

        // Just verify they compile and are the right variant
        match e1 {
            FixtureEvent::ContentBlockStart { .. } => {}
            _ => panic!("wrong variant"),
        }
        match e2 {
            FixtureEvent::ContentDelta { .. } => {}
            _ => panic!("wrong variant"),
        }
        match e3 {
            FixtureEvent::ToolUseStart { .. } => {}
            _ => panic!("wrong variant"),
        }
        match e4 {
            FixtureEvent::ToolUseResult { .. } => {}
            _ => panic!("wrong variant"),
        }
        match e5 {
            FixtureEvent::ContentBlockStop { .. } => {}
            _ => panic!("wrong variant"),
        }
        match e6 {
            FixtureEvent::MessageStop => {}
            _ => panic!("wrong variant"),
        }
    }

    // --- FixtureRecording new defaults ---

    #[test]
    fn fixture_recording_new_defaults() {
        let rec = FixtureRecording::new("test");
        assert_eq!(rec.name, "test");
        assert!(rec.description.is_none());
        assert!(rec.turns.is_empty());
        assert!(rec.files.is_empty());
    }
}
