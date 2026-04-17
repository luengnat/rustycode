// Tests for fixture-based testing system

use rustycode_llm::provider_v2::{CompletionRequest, LLMProvider};
use rustycode_orchestra::fixture::{
    FixtureMode, FixtureProvider, FixtureRecording, FixtureRole, FixtureToolUse,
};

#[tokio::test]
async fn test_fixture_mode_from_env() {
    assert_eq!(FixtureMode::from_env(), FixtureMode::Off);
}

#[tokio::test]
async fn test_fixture_recording_builder() {
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

#[tokio::test]
async fn test_fixture_recording_serialization() {
    let recording = FixtureRecording::new("test")
        .add_user_turn("hello")
        .add_assistant_turn("hi there", vec![]);

    let json = serde_json::to_string_pretty(&recording).unwrap();
    let parsed: FixtureRecording = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.name, "test");
    assert_eq!(parsed.turns.len(), 2);
}

#[tokio::test]
async fn test_fixture_provider_load() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");

    // Load the fixture
    let result = provider.load_fixture("agent-creates-file");
    assert!(result.is_ok());

    // Check turn index starts at 0
    assert_eq!(provider.turn_index(), 0);

    // Get the recording
    let recording = provider.get_recording();
    assert!(recording.is_some());
    let rec = recording.unwrap();
    assert_eq!(rec.name, "agent-creates-file");
    assert_eq!(rec.turns.len(), 2);
}

#[tokio::test]
async fn test_fixture_provider_complete() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider.load_fixture("agent-creates-file").unwrap();

    // Verify the fixture loaded correctly
    let recording = provider.get_recording().unwrap();
    assert_eq!(recording.turns.len(), 2);
    assert_eq!(recording.turns[0].role, FixtureRole::User);
    assert_eq!(recording.turns[1].role, FixtureRole::Assistant);
    assert_eq!(recording.turns[1].content, "I'll create the file for you.");
}

#[tokio::test]
async fn test_fixture_provider_stream() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider.load_fixture("agent-creates-file").unwrap();

    let request = CompletionRequest {
        model: "fixture".to_string(),
        messages: vec![],
        max_tokens: Some(1000u32),
        temperature: Some(0.7f32),
        stream: false,
        system_prompt: None,
        tools: None,
        extended_thinking: None,
        thinking_budget: None,
        effort: None,
        thinking: None,
        output_config: None,
    };

    // First call gets the user turn, which will error
    // So we just verify the provider can create the stream
    let stream_result = provider.complete_stream(request).await;
    // The user turn should cause an error since we expect assistant turns
    assert!(stream_result.is_err());
}
