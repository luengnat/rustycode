// Fixture-based LLM provider for testing

use crate::fixture::{FixtureError, FixtureMode, FixtureRecording, FixtureRole};
use async_trait::async_trait;
use futures::Stream;
use rustycode_llm::provider_v2::{
    CompletionRequest, CompletionResponse, ContentDelta, LLMProvider, ProviderError, SSEEvent,
    StreamChunk,
};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// Fixture-based provider for testing
///
/// In replay mode, returns pre-recorded responses without LLM calls.
pub struct FixtureProvider {
    mode: FixtureMode,
    fixture_dir: PathBuf,
    current_fixture: Arc<Mutex<Option<FixtureRecording>>>,
    turn_index: Arc<Mutex<usize>>,
}

impl FixtureProvider {
    /// Create a new fixture provider
    pub fn new(mode: FixtureMode, fixture_dir: impl Into<PathBuf>) -> Self {
        Self {
            mode,
            fixture_dir: fixture_dir.into(),
            current_fixture: Arc::new(Mutex::new(None)),
            turn_index: Arc::new(Mutex::new(0)),
        }
    }

    /// Load a fixture by name
    pub fn load_fixture(&self, name: &str) -> Result<(), FixtureError> {
        let path = self.fixture_dir.join(format!("{}.json", name));
        let recording = FixtureRecording::load(&path)?;
        *self
            .current_fixture
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(recording);
        *self.turn_index.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        Ok(())
    }

    /// Get the current fixture recording
    pub fn get_recording(&self) -> Option<FixtureRecording> {
        self.current_fixture
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Reset the turn index
    pub fn reset(&self) {
        *self.turn_index.lock().unwrap_or_else(|e| e.into_inner()) = 0;
    }

    /// Get current turn index
    pub fn turn_index(&self) -> usize {
        *self.turn_index.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Set turn index directly (useful for skipping to specific turns)
    pub fn set_turn_index(&self, index: usize) {
        *self.turn_index.lock().unwrap_or_else(|e| e.into_inner()) = index;
    }

    /// Get all assistant turns from the fixture
    pub fn assistant_turns(&self) -> Vec<crate::fixture::FixtureTurn> {
        self.current_fixture
            .lock()
            .unwrap()
            .as_ref()
            .map(|f| {
                f.turns
                    .iter()
                    .filter(|t| t.role == FixtureRole::Assistant)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all user turns from the fixture
    pub fn user_turns(&self) -> Vec<crate::fixture::FixtureTurn> {
        self.current_fixture
            .lock()
            .unwrap()
            .as_ref()
            .map(|f| {
                f.turns
                    .iter()
                    .filter(|t| t.role == FixtureRole::User)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Count turns by role
    pub fn count_turns(&self, role: Option<FixtureRole>) -> usize {
        self.current_fixture
            .lock()
            .unwrap()
            .as_ref()
            .map(|f| {
                f.turns
                    .iter()
                    .filter(|t| role.as_ref().is_none_or(|r| t.role == *r))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Convert fixture turn to completion response
    fn fixture_to_response(&self, turn: &crate::fixture::FixtureTurn) -> CompletionResponse {
        CompletionResponse {
            content: turn.content.clone(),
            model: "fixture-replay".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
        }
    }
}

#[async_trait]
impl LLMProvider for FixtureProvider {
    fn name(&self) -> &'static str {
        "fixture"
    }

    async fn is_available(&self) -> bool {
        matches!(self.mode, FixtureMode::Replay)
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["fixture-replay".to_string()])
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let fixture = {
            let fixture_guard = self
                .current_fixture
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            fixture_guard.clone()
        };

        let fixture = fixture.ok_or_else(|| ProviderError::api("No fixture loaded"))?;

        let mut idx_guard = self.turn_index.lock().unwrap_or_else(|e| e.into_inner());
        let idx = *idx_guard;
        if idx >= fixture.turns.len() {
            return Err(ProviderError::api("No more turns in fixture"));
        }

        let turn = &fixture.turns[idx];
        *idx_guard += 1;

        if turn.role != FixtureRole::Assistant {
            return Err(ProviderError::api(format!(
                "Expected assistant turn, got {:?}",
                turn.role
            )));
        }

        Ok(self.fixture_to_response(turn))
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let fixture = {
            let fixture_guard = self
                .current_fixture
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            fixture_guard.clone()
        };

        let fixture = fixture.ok_or_else(|| ProviderError::api("No fixture loaded"))?;

        let mut idx_guard = self.turn_index.lock().unwrap_or_else(|e| e.into_inner());
        let idx = *idx_guard;
        if idx >= fixture.turns.len() {
            return Err(ProviderError::api("No more turns in fixture"));
        }

        let turn = fixture.turns[idx].clone();
        *idx_guard += 1;

        if turn.role != FixtureRole::Assistant {
            return Err(ProviderError::api(format!(
                "Expected assistant turn, got {:?}",
                turn.role
            )));
        }

        // Create a simple stream
        let stream = async_stream::stream! {
            // Yield content chunks
            for chunk in turn.content.as_bytes().chunks(10) {
                let text = String::from_utf8_lossy(chunk).to_string();
                yield Ok(SSEEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::Text { text },
                });
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }

            yield Ok(SSEEvent::ContentBlockStop { index: 0 });
            yield Ok(SSEEvent::MessageStop);
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_provider_replay() {
        let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures");
        assert_eq!(provider.name(), "fixture");
    }

    #[test]
    fn test_fixture_provider_mode_from_env() {
        assert_eq!(FixtureMode::from_env(), FixtureMode::Off);
    }
}
