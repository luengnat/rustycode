// Fixture recorder - captures LLM interactions for replay

use crate::fixture::{FixtureRecording, FixtureRole, FixtureToolUse};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Records LLM conversations as fixtures
pub struct FixtureRecorder {
    recording: Arc<Mutex<FixtureRecording>>,
    current_content: Arc<Mutex<String>>,
    current_tools: Arc<Mutex<Vec<FixtureToolUse>>>,
    in_content_block: Arc<Mutex<bool>>,
}

impl FixtureRecorder {
    /// Create a new recorder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            recording: Arc::new(Mutex::new(FixtureRecording::new(name))),
            current_content: Arc::new(Mutex::new(String::new())),
            current_tools: Arc::new(Mutex::new(Vec::new())),
            in_content_block: Arc::new(Mutex::new(false)),
        }
    }

    /// Create with description
    pub fn with_description(name: impl Into<String>, description: impl Into<String>) -> Self {
        let mut rec = FixtureRecording::new(name);
        rec.description = Some(description.into());

        Self {
            recording: Arc::new(Mutex::new(rec)),
            current_content: Arc::new(Mutex::new(String::new())),
            current_tools: Arc::new(Mutex::new(Vec::new())),
            in_content_block: Arc::new(Mutex::new(false)),
        }
    }

    /// Record a user message
    pub async fn record_user(&self, content: impl Into<String>) {
        let mut recording = self.recording.lock().await;
        recording.turns.push(crate::fixture::FixtureTurn {
            role: FixtureRole::User,
            content: content.into(),
            tool_uses: Vec::new(),
        });
    }

    /// Start recording an assistant response
    pub async fn start_assistant(&self) {
        *self.current_content.lock().await = String::new();
        *self.current_tools.lock().await = Vec::new();
        *self.in_content_block.lock().await = true;
    }

    /// Add content to the current assistant turn
    pub async fn add_content(&self, content: &str) {
        if *self.in_content_block.lock().await {
            self.current_content.lock().await.push_str(content);
        }
    }

    /// Add a tool use to the current assistant turn
    pub async fn add_tool_use(&self, name: impl Into<String>, input: serde_json::Value) {
        let tool_use = FixtureToolUse {
            name: name.into(),
            input,
            output: None,
            error: None,
        };
        self.current_tools.lock().await.push(tool_use);
    }

    /// Set the result of the last tool use
    pub async fn set_tool_result(&self, output: Option<String>, error: Option<String>) {
        let mut tools = self.current_tools.lock().await;
        if let Some(tool_use) = tools.last_mut() {
            tool_use.output = output;
            tool_use.error = error;
        }
    }

    /// Finish the current assistant turn
    pub async fn finish_assistant(&self) {
        let content = self.current_content.lock().await.clone();
        let tools = self.current_tools.lock().await.clone();

        let mut recording = self.recording.lock().await;
        recording.turns.push(crate::fixture::FixtureTurn {
            role: FixtureRole::Assistant,
            content,
            tool_uses: tools,
        });

        // Reset for next turn
        *self.current_content.lock().await = String::new();
        *self.current_tools.lock().await = Vec::new();
    }

    /// Finish recording and get the recording
    pub async fn finish(&self) -> FixtureRecording {
        self.recording.lock().await.clone()
    }

    /// Save the recording to a file
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
        let recording = self.recording.lock().await;
        let path = path.as_ref().to_path_buf();

        // Serialize in spawn_blocking to avoid blocking async runtime
        let json = tokio::task::spawn_blocking({
            let recording = (*recording).clone();
            move || serde_json::to_string_pretty(&recording)
        })
        .await
        .map_err(std::io::Error::other)?
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Use tokio::fs for non-blocking file I/O
        tokio::fs::write(&path, &json).await?;
        Ok(())
    }

    /// Add a file to the recording
    pub async fn add_file(&self, path: impl Into<String>, content: impl Into<String>) {
        let mut recording = self.recording.lock().await;
        recording.files.push(crate::fixture::FixtureFile {
            path: path.into(),
            content: content.into(),
        });
    }

    /// Get the number of turns recorded
    pub async fn turn_count(&self) -> usize {
        self.recording.lock().await.turns.len()
    }
}

/// Helper to create fixture recordings from actual LLM interactions
pub struct RecordingSession {
    recorder: FixtureRecorder,
    fixture_dir: std::path::PathBuf,
}

impl RecordingSession {
    /// Create a new recording session
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        fixture_dir: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            recorder: FixtureRecorder::with_description(name, description),
            fixture_dir: fixture_dir.into(),
        }
    }

    /// Get the recorder
    pub fn recorder(&self) -> &FixtureRecorder {
        &self.recorder
    }

    /// Save the recording to the fixture directory
    pub async fn save(&self) -> Result<(), std::io::Error> {
        let name = {
            let rec = self.recorder.recording.lock().await;
            rec.name.clone()
        };
        let path = self.fixture_dir.join(format!("{}.json", name));
        self.recorder.save(&path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_recorder_basic() {
        let recorder = FixtureRecorder::new("test-recording");

        recorder.record_user("Create a file called test.txt").await;
        recorder.start_assistant().await;
        recorder.add_content("I'll create that file").await;
        recorder.finish_assistant().await;

        let recording = recorder.finish().await;
        assert_eq!(recording.name, "test-recording");
        assert_eq!(recording.turns.len(), 2);
        assert_eq!(recording.turns[0].role, FixtureRole::User);
        assert_eq!(recording.turns[1].role, FixtureRole::Assistant);
    }

    #[tokio::test]
    async fn test_recorder_with_tools() {
        let recorder = FixtureRecorder::new("test-tools");

        recorder.record_user("Read the file").await;
        recorder.start_assistant().await;

        recorder
            .add_tool_use("read_file", serde_json::json!({"path": "test.txt"}))
            .await;
        recorder
            .set_tool_result(Some("file content".to_string()), None)
            .await;

        recorder.finish_assistant().await;

        let recording = recorder.finish().await;
        assert_eq!(recording.turns[1].tool_uses.len(), 1);
        assert_eq!(recording.turns[1].tool_uses[0].name, "read_file");
    }

    // --- Additional tests ---

    #[tokio::test]
    async fn test_recorder_with_description() {
        let recorder = FixtureRecorder::with_description("desc-test", "A test description");
        recorder.record_user("Hello").await;
        let recording = recorder.finish().await;
        assert_eq!(recording.name, "desc-test");
        assert_eq!(recording.description.as_deref(), Some("A test description"));
    }

    #[tokio::test]
    async fn test_recorder_turn_count() {
        let recorder = FixtureRecorder::new("count-test");
        assert_eq!(recorder.turn_count().await, 0);
        recorder.record_user("First").await;
        assert_eq!(recorder.turn_count().await, 1);
        recorder.record_user("Second").await;
        assert_eq!(recorder.turn_count().await, 2);
    }

    #[tokio::test]
    async fn test_recorder_add_file() {
        let recorder = FixtureRecorder::new("file-test");
        recorder.add_file("src/main.rs", "fn main() {}").await;
        let recording = recorder.finish().await;
        assert_eq!(recording.files.len(), 1);
        assert_eq!(recording.files[0].path, "src/main.rs");
        assert_eq!(recording.files[0].content, "fn main() {}");
    }

    #[tokio::test]
    async fn test_recorder_tool_with_error() {
        let recorder = FixtureRecorder::new("tool-err");
        recorder.record_user("Do something").await;
        recorder.start_assistant().await;
        recorder
            .add_tool_use("bash", serde_json::json!({"cmd": "false"}))
            .await;
        recorder
            .set_tool_result(None, Some("exit code 1".into()))
            .await;
        recorder.finish_assistant().await;

        let recording = recorder.finish().await;
        assert_eq!(recording.turns[1].tool_uses.len(), 1);
        assert!(recording.turns[1].tool_uses[0].output.is_none());
        assert_eq!(
            recording.turns[1].tool_uses[0].error.as_deref(),
            Some("exit code 1")
        );
    }

    #[tokio::test]
    async fn test_recorder_multiple_assistant_turns() {
        let recorder = FixtureRecorder::new("multi");
        recorder.record_user("Turn 1").await;
        recorder.start_assistant().await;
        recorder.add_content("Response 1").await;
        recorder.finish_assistant().await;

        recorder.record_user("Turn 2").await;
        recorder.start_assistant().await;
        recorder.add_content("Response 2").await;
        recorder.finish_assistant().await;

        let recording = recorder.finish().await;
        assert_eq!(recording.turns.len(), 4);
        assert_eq!(recording.turns[1].content, "Response 1");
        assert_eq!(recording.turns[3].content, "Response 2");
    }

    #[tokio::test]
    async fn test_recorder_content_not_recorded_without_start() {
        let recorder = FixtureRecorder::new("no-start");
        // add_content without start_assistant should be ignored
        recorder.add_content("Should be ignored").await;
        recorder.record_user("Hello").await;
        let recording = recorder.finish().await;
        assert_eq!(recording.turns.len(), 1);
        assert_eq!(recording.turns[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_recorder_set_tool_result_no_tools() {
        let recorder = FixtureRecorder::new("no-tools");
        recorder.start_assistant().await;
        // set_tool_result with no tools should be a no-op
        recorder.set_tool_result(Some("output".into()), None).await;
        recorder.finish_assistant().await;

        let recording = recorder.finish().await;
        assert_eq!(recording.turns[0].tool_uses.len(), 0);
    }

    #[tokio::test]
    async fn test_recording_session_save_path() {
        let dir = tempfile::tempdir().unwrap();
        let session = RecordingSession::new("save-test", "desc", dir.path());
        session.recorder().record_user("Hello").await;
        session.recorder().start_assistant().await;
        session.recorder().add_content("Hi").await;
        session.recorder().finish_assistant().await;
        session.save().await.unwrap();

        let saved = std::fs::read_to_string(dir.path().join("save-test.json")).unwrap();
        assert!(saved.contains("save-test"));
        assert!(saved.contains("Hello"));
    }
}
