//! Agent domain models: agents, providers, activity, cost, permissions.

use serde::{Serialize, Deserialize};
use crate::agent_router;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentActivity {
    pub id: String,
    pub agent_name: String,
    pub action: String,
    pub status: String, // "start", "complete", "error"
    pub workflow_name: String,
    pub timestamp: String,
    /// The actual SQL query or shell command being executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Raw output / result of the execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Step type: "sql", "shell", "api", "prompt"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_type: Option<String>,
    // ── Rationale Log fields (Section 13) ──
    /// Agent's stated reason for this action (max 20 words)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Model used: "local/gemma-4-27b", "anthropic/claude-sonnet-4-6", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_used: Option<String>,
    /// Input tokens consumed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    /// Output tokens consumed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    /// Cost in USD (0.0 for local inference)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Where inference ran: "local", "anthropic-us", "vertex-eu-west3"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_location: Option<String>,
    /// Agent spawn depth (0=top, 1=child, 2=grandchild)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_depth: Option<u32>,
}

/// Largest number of activity rows kept in memory. The activity feed and the
/// session-cost ticker only ever read the recent tail, but every LLM turn and
/// workflow step appends one, so an unbounded Vec grows for the life of the
/// process. Retain the most recent entries and drop the oldest.
pub const ACTIVITY_LOG_CAP: usize = 2000;

/// Push an activity row and keep the log within ACTIVITY_LOG_CAP by dropping
/// the oldest entries. Call this at every append site instead of a bare push.
pub fn push_capped(log: &mut Vec<AgentActivity>, item: AgentActivity) {
    log.push(item);
    if log.len() > ACTIVITY_LOG_CAP {
        let overflow = log.len() - ACTIVITY_LOG_CAP;
        log.drain(0..overflow);
    }
}

/// Session-level cost tracking for the cost ticker
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct SessionCost {
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_cost_usd: f64,
    pub call_count: u64,
    pub local_calls: u64,
    pub cloud_calls: u64,
}

/// RBAC permission scopes
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    ReadLocal,
    WriteLocal,
    ExecShell,
    CallApiRead,
    CallApiWrite,
    SpawnAgent,
    RelayCloud,
    NotifyHuman,
    AdminFull,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub provider: AgentProvider,
    pub model: String,
    pub endpoint: String,
    pub api_key: String,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    #[serde(default)]
    pub connector_ids: Vec<String>,
    #[serde(default)]
    pub workflow_ids: Vec<String>,
    #[serde(default)]
    pub capabilities: agent_router::AgentCapabilities,
    // ── P2: employee fields. An agent with a charter is an employee:
    //    it runs shifts (the scoped agentic loop) instead of being a
    //    bare model connection. ──
    /// Plain-language job description. Empty = plain model connection.
    #[serde(default)]
    pub charter: String,
    /// Tool names (from tool_schemas::bob_tools) this agent may call
    /// during a shift. Empty = safe read-only defaults.
    #[serde(default)]
    pub granted_tools: Vec<String>,
    /// Wake interval in minutes; None = manual shifts only.
    #[serde(default)]
    pub schedule_minutes: Option<u64>,
    #[serde(default)]
    pub last_run_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentProvider {
    Local,      // deprecated - kept for deserialization compat
    Anthropic,
    OpenAI,     // deprecated - kept for deserialization compat
    Ollama,     // deprecated - kept for deserialization compat
    LmStudio,   // deprecated - kept for deserialization compat
    Custom,     // deprecated - kept for deserialization compat
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentInput {
    pub name: String,
    pub provider: AgentProvider,
    pub model: String,
    pub endpoint: String,
    pub api_key: String,
    pub is_default: bool,
    #[serde(default)]
    pub connector_ids: Vec<String>,
    #[serde(default)]
    pub workflow_ids: Vec<String>,
    #[serde(default)]
    pub capabilities: agent_router::AgentCapabilities,
    // ── P2: employee fields. An agent with a charter is an employee:
    //    it runs shifts (the scoped agentic loop) instead of being a
    //    bare model connection. ──
    /// Plain-language job description. Empty = plain model connection.
    #[serde(default)]
    pub charter: String,
    /// Tool names (from tool_schemas::bob_tools) this agent may call
    /// during a shift. Empty = safe read-only defaults.
    #[serde(default)]
    pub granted_tools: Vec<String>,
    /// Wake interval in minutes; None = manual shifts only.
    #[serde(default)]
    pub schedule_minutes: Option<u64>,
    #[serde(default)]
    pub last_run_at: Option<String>,
}

#[derive(Serialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub full_disk_access: bool,
    pub notifications: bool,
    pub local_network: bool,
}
