//! Chat and conversation models: messages and orb state.

use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrbState {
    Idle,
    Listening,
    Thinking,
    Executing,
    Learning,
    Error,
}

impl Default for OrbState {
    fn default() -> Self {
        OrbState::Idle
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,  // raw LLM output with action tags, for history context
    pub agent_id: Option<String>,
    pub timestamp: String,
    /// The agentic run's native turns behind an assistant reply: its
    /// tool-call turns and their `role: tool` results (capped). Replayed
    /// in later history so the model sees answers came from tools; with
    /// final answers alone it copied "just answer" and invented device
    /// facts (measured 2026-10-21, Gemma 4 E4B: 0/6 tool use vs 6/6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_turns: Option<Vec<serde_json::Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SendMessageInput {
    pub content: String,
    pub agent_id: Option<String>,
}
