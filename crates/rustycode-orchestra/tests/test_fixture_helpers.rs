// Tests for fixture provider helper methods

use rustycode_orchestra::fixture::{FixtureMode, FixtureProvider, FixtureRole};

#[tokio::test]
async fn test_provider_helper_methods() {
    let provider = FixtureProvider::new(FixtureMode::Replay, "tests/fixtures/recordings");
    provider
        .load_fixture("agent-multi-turn-conversation")
        .unwrap();

    // Count all turns
    assert_eq!(provider.count_turns(None), 6);

    // Count by role
    assert_eq!(provider.count_turns(Some(FixtureRole::User)), 3);
    assert_eq!(provider.count_turns(Some(FixtureRole::Assistant)), 3);

    // Get assistant turns
    let assistant_turns = provider.assistant_turns();
    assert_eq!(assistant_turns.len(), 3);

    // Get user turns
    let user_turns = provider.user_turns();
    assert_eq!(user_turns.len(), 3);

    // Verify first assistant turn
    assert!(assistant_turns[0].content.contains("I'll read"));

    // Verify first user turn
    assert!(user_turns[0].content.contains("Read the file"));
}
