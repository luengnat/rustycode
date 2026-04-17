use crate::codec::HookInput;

pub fn evaluate(_input: &HookInput) -> crate::codec::HookResult {
    // PostToolUse advisory: Warns about potential issues in production code but does not block.
    crate::codec::HookResult::warn("Post-tool usage detected. Review tests and logs for anomalies.")
}
