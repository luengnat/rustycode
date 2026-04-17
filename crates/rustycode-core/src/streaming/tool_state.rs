//! Tool accumulation state for streaming SSE responses
//!
//! Tracks a tool call being accumulated across ContentBlockDelta chunks.

/// A tool call being accumulated across SSE streaming events.
///
/// As ContentBlockDelta events arrive with PartialJson chunks, this struct
/// accumulates the JSON until ContentBlockStop is reached.
#[derive(Debug, Clone)]
pub struct ToolAccumulator {
    /// Unique identifier for this tool call
    pub id: String,
    /// Name of the tool being called
    pub name: String,
    /// Accumulated JSON parameters (partial until ContentBlockStop)
    pub partial_json: String,
}

impl ToolAccumulator {
    /// Create a new tool accumulator
    ///
    /// # Arguments
    /// * `id` - Unique call ID from ContentBlockStart
    /// * `name` - Tool name from ContentBlockStart
    /// * `initial_json` - Eagerly-streamed JSON (from ContentBlockStart::input field), or empty string
    pub fn new(id: String, name: String, initial_json: String) -> Self {
        Self {
            id,
            name,
            partial_json: initial_json,
        }
    }

    /// Append a JSON delta chunk
    pub fn push_json(&mut self, chunk: &str) {
        self.partial_json.push_str(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_accumulator_new() {
        let tool = ToolAccumulator::new(
            "call_123".to_string(),
            "read_file".to_string(),
            String::new(),
        );
        assert_eq!(tool.id, "call_123");
        assert_eq!(tool.name, "read_file");
        assert!(tool.partial_json.is_empty());
    }

    #[test]
    fn test_tool_accumulator_with_initial_json() {
        let tool = ToolAccumulator::new(
            "call_456".to_string(),
            "write_file".to_string(),
            r#"{"path":"#.to_string(),
        );
        assert_eq!(tool.partial_json, r#"{"path":"#);
    }

    #[test]
    fn test_tool_accumulator_push_json() {
        let mut tool = ToolAccumulator::new(
            "id".to_string(),
            "tool".to_string(),
            "{\"p\":\"".to_string(),
        );
        tool.push_json("a\"}");
        assert_eq!(tool.partial_json, "{\"p\":\"a\"}");
    }
}
