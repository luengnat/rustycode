#![allow(clippy::unwrap_used, clippy::expect_used)]

use rustycode_guard::codec::HookInput;
use rustycode_guard::pre_tool;
use serde_json::json;

fn make_input(tool: &str, input: serde_json::Value, cwd: Option<&str>) -> HookInput {
    HookInput {
        session_id: None,
        tool_name: tool.to_string(),
        tool_input: input,
        cwd: cwd.map(std::string::ToString::to_string),
        hook_event_name: None,
    }
}

#[test]
fn test_r01_sudo_block() {
    let input = make_input(
        "Bash",
        json!({"command": "sudo rm -rf /"}),
        Some("/workspace/project"),
    );
    let res = pre_tool::evaluate(&input);
    assert_eq!(res.permission_decision.as_deref(), Some("deny"));
}

#[test]
fn test_r02_protected_path() {
    let input = make_input(
        "Write",
        json!({"path": ".git/config"}),
        Some("/workspace/project"),
    );
    let res = pre_tool::evaluate(&input);
    assert_eq!(res.permission_decision.as_deref(), Some("deny"));
}

#[test]
fn test_r03_shell_writes() {
    let input = make_input(
        "Bash",
        json!({"command": "echo hi > /etc/passwd"}),
        Some("/workspace/project"),
    );
    let res = pre_tool::evaluate(&input);
    assert_eq!(res.permission_decision.as_deref(), Some("deny"));
}

#[test]
fn test_r15_large_content() {
    let large = String::from_utf8(vec![b'a'; 10_000_001]).expect("valid UTF-8");
    let input = make_input(
        "Write",
        json!({"path": "payload.txt", "content": large}),
        Some("/workspace/project"),
    );
    let res = pre_tool::evaluate(&input);
    assert_eq!(res.permission_decision.as_deref(), Some("deny"));
}
