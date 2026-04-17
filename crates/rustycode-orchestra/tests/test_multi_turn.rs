// Multi-turn conversation tests for fixture system

use rustycode_llm::provider_v2::{CompletionRequest, LLMProvider};
use rustycode_orchestra::fixture::{FixtureMode, FixtureProvider, FixtureRole};

#[tokio::test]
async fn test_multi_turn_fixture_load() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");

    let result = provider.load_fixture("agent-multi-turn-conversation");
    assert!(result.is_ok());

    let recording = provider.get_recording().unwrap();
    assert_eq!(recording.name, "agent-multi-turn-conversation");
    assert_eq!(recording.turns.len(), 6); // 3 user + 3 assistant
}

#[tokio::test]
async fn test_multi_turn_sequence() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider
        .load_fixture("agent-multi-turn-conversation")
        .unwrap();

    let recording = provider.get_recording().unwrap();

    // Verify turn sequence
    assert_eq!(recording.turns[0].role, FixtureRole::User);
    assert!(recording.turns[0].content.contains("Read the file"));

    assert_eq!(recording.turns[1].role, FixtureRole::Assistant);
    assert!(recording.turns[1].content.contains("I'll read"));
    assert_eq!(recording.turns[1].tool_uses.len(), 1);
    assert_eq!(recording.turns[1].tool_uses[0].name, "read_file");

    assert_eq!(recording.turns[2].role, FixtureRole::User);
    assert!(recording.turns[2].content.contains("enable debug mode"));

    assert_eq!(recording.turns[3].role, FixtureRole::Assistant);
    assert_eq!(recording.turns[3].tool_uses[0].name, "write_file");

    assert_eq!(recording.turns[4].role, FixtureRole::User);
    assert!(recording.turns[4].content.contains("Verify"));

    assert_eq!(recording.turns[5].role, FixtureRole::Assistant);
    assert_eq!(recording.turns[5].tool_uses[0].name, "read_file");
}

#[tokio::test]
async fn test_multi_turn_tool_outputs() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider
        .load_fixture("agent-multi-turn-conversation")
        .unwrap();

    let recording = provider.get_recording().unwrap();

    // First tool use - read_file with original config
    let tool1 = &recording.turns[1].tool_uses[0];
    assert_eq!(tool1.name, "read_file");
    assert!(tool1.output.as_ref().unwrap().contains("port = 8080"));

    // Second tool use - write_file with new config
    let tool2 = &recording.turns[3].tool_uses[0];
    assert_eq!(tool2.name, "write_file");
    assert!(tool2.output.as_ref().unwrap().contains("successfully"));

    // Third tool use - read_file verification
    let tool3 = &recording.turns[5].tool_uses[0];
    assert_eq!(tool3.name, "read_file");
    assert!(tool3.output.as_ref().unwrap().contains("port = 9000"));
    assert!(tool3.output.as_ref().unwrap().contains("debug = true"));
}

#[tokio::test]
async fn test_multi_turn_progressive_state() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider
        .load_fixture("agent-multi-turn-conversation")
        .unwrap();

    // Simulate progressing through turns
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

    // First assistant response (turn index 1)
    let response1 = provider.complete(request.clone()).await;
    assert!(response1.is_err()); // User turn at index 0

    // The provider increments index even on error, so next would be user turn
    // For proper multi-turn, we'd need to skip user turns
    provider.reset();

    // Skip to first assistant turn manually
    provider.set_turn_index(1);
    let response1 = provider.complete(request.clone()).await.unwrap();
    assert!(response1.content.contains("I'll read"));
}

#[tokio::test]
async fn test_conversation_context_flow() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider
        .load_fixture("agent-multi-turn-conversation")
        .unwrap();

    let recording = provider.get_recording().unwrap();

    // Verify conversation shows context building:
    // Turn 1: Read file -> gets port=8080, debug=false
    // Turn 2: Modify -> sets port=9000, debug=true
    // Turn 3: Verify -> confirms port=9000, debug=true

    let first_read = &recording.turns[1].tool_uses[0];
    let write = &recording.turns[3].tool_uses[0];
    let verify = &recording.turns[5].tool_uses[0];

    // State progression
    assert!(first_read.output.as_ref().unwrap().contains("8080"));
    assert!(write.input["content"].as_str().unwrap().contains("9000"));
    assert!(verify.output.as_ref().unwrap().contains("9000"));
}
