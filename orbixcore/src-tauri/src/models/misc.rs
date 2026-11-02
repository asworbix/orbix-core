//! Assorted command I/O models.

use serde::{Serialize, Deserialize};
use crate::presets;

#[derive(Serialize, Deserialize, Debug)]
pub struct CliTemplate {
    pub name: String,
    pub description: String,
    pub template_type: String,
    pub code: String,
    pub config: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePresetInput {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub active_tab: String,
    pub active_panel: Option<String>,
    pub role_id: String,
    pub voice_settings: presets::VoiceSnapshot,
    pub enabled_agent_ids: Vec<String>,
    pub default_agent_id: Option<String>,
    pub selected_local_model: Option<String>,
    pub connector_instance_ids: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct AuditSummary {
    pub count: usize,
    pub chain_ok: bool,
    pub last_seq: Option<u64>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectorModulesSummary {
    pub count: usize,
    pub slugs: Vec<String>,
    pub dir: String,
}
