//! MCP server models.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    pub enabled: bool,
    pub status: McpStatus,
    pub created_at: String,
    /// HTTP servers: send the user's token for this connected service
    /// (services.rs slug) as the bearer.
    #[serde(default)]
    pub bearer_from_service: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpStatus {
    Stopped,
    Running,
    Error,
    Unknown,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McpServerInput {
    pub name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub url: Option<String>,
}
