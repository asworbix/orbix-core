use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

/// A saved workspace preset - captures the full UI + config state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspacePreset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub icon: String,

    // UI state
    pub active_tab: String,
    pub active_panel: Option<String>,

    // Role
    pub role_id: String,

    // Voice
    pub voice_settings: VoiceSnapshot,

    // Agents
    pub enabled_agent_ids: Vec<String>,
    pub default_agent_id: Option<String>,
    pub selected_local_model: Option<String>,

    // Connectors
    pub connector_instance_ids: Vec<String>,

    // Metadata
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceSnapshot {
    pub tts_voice: String,
    pub tts_rate: f32,
    pub auto_speak: bool,
    pub stt_language: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PresetStore {
    presets: Vec<WorkspacePreset>,
}

/// Built-in presets that ship with the app.
fn builtin_presets() -> Vec<WorkspacePreset> {
    let now = Utc::now().to_rfc3339();
    vec![
        WorkspacePreset {
            id: "preset-sales-hustle".into(),
            name: "Sales Hustle".into(),
            description: "CRM-focused workspace with connectors and inbox front and center.".into(),
            icon: "A".into(),
            active_tab: "build".into(),
            active_panel: Some("connectors".into()),
            role_id: "crm_sales".into(),
            voice_settings: VoiceSnapshot {
                tts_voice: "Samantha".into(),
                tts_rate: 175.0,
                auto_speak: false,
                stt_language: "en-US".into(),
            },
            enabled_agent_ids: vec![],
            default_agent_id: None,
            selected_local_model: None,
            connector_instance_ids: vec![],
            is_builtin: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        WorkspacePreset {
            id: "preset-dev-focus".into(),
            name: "Dev Focus".into(),
            description: "Developer workspace with tools, MCP servers, and code agents.".into(),
            icon: "B".into(),
            active_tab: "build".into(),
            active_panel: Some("tools".into()),
            role_id: "dev".into(),
            voice_settings: VoiceSnapshot {
                tts_voice: "Samantha".into(),
                tts_rate: 175.0,
                auto_speak: false,
                stt_language: "en-US".into(),
            },
            enabled_agent_ids: vec![],
            default_agent_id: None,
            selected_local_model: None,
            connector_instance_ids: vec![],
            is_builtin: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        WorkspacePreset {
            id: "preset-ops-command".into(),
            name: "Ops Command".into(),
            description: "Support & ops workspace with monitoring, incidents, and SLA tracking.".into(),
            icon: "C".into(),
            active_tab: "watch".into(),
            active_panel: Some("monitor".into()),
            role_id: "support".into(),
            voice_settings: VoiceSnapshot {
                tts_voice: "Samantha".into(),
                tts_rate: 175.0,
                auto_speak: false,
                stt_language: "en-US".into(),
            },
            enabled_agent_ids: vec![],
            default_agent_id: None,
            selected_local_model: None,
            connector_instance_ids: vec![],
            is_builtin: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        WorkspacePreset {
            id: "preset-marketing-hub".into(),
            name: "Marketing Hub".into(),
            description: "Marketing workspace with scheduler, dashboard, and export tools.".into(),
            icon: "D".into(),
            active_tab: "build".into(),
            active_panel: Some("scheduler".into()),
            role_id: "marketing".into(),
            voice_settings: VoiceSnapshot {
                tts_voice: "Samantha".into(),
                tts_rate: 175.0,
                auto_speak: false,
                stt_language: "en-US".into(),
            },
            enabled_agent_ids: vec![],
            default_agent_id: None,
            selected_local_model: None,
            connector_instance_ids: vec![],
            is_builtin: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        WorkspacePreset {
            id: "preset-finance-desk".into(),
            name: "Finance Desk".into(),
            description: "Finance workspace with reports, audit trail, and compliance.".into(),
            icon: "E".into(),
            active_tab: "build".into(),
            active_panel: Some("reports".into()),
            role_id: "finance".into(),
            voice_settings: VoiceSnapshot {
                tts_voice: "Samantha".into(),
                tts_rate: 175.0,
                auto_speak: false,
                stt_language: "en-US".into(),
            },
            enabled_agent_ids: vec![],
            default_agent_id: None,
            selected_local_model: None,
            connector_instance_ids: vec![],
            is_builtin: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        },
    ]
}

impl PresetStore {
    pub fn load() -> Self {
        let mut user_presets: Vec<WorkspacePreset> =
            crate::storage::load_json("presets.json").unwrap_or_default();

        // Merge builtins (don't duplicate)
        let builtins = builtin_presets();
        for b in &builtins {
            if !user_presets.iter().any(|p| p.id == b.id) {
                user_presets.push(b.clone());
            }
        }

        Self { presets: user_presets }
    }

    pub fn save(&self) -> Result<(), String> {
        crate::storage::save_json("presets.json", &self.presets)
    }

    pub fn list(&self) -> Vec<WorkspacePreset> {
        self.presets.clone()
    }

    pub fn get(&self, id: &str) -> Option<WorkspacePreset> {
        self.presets.iter().find(|p| p.id == id).cloned()
    }

    pub fn create(&mut self, name: String, description: String, icon: String, snapshot: WorkspacePreset) -> WorkspacePreset {
        let now = Utc::now().to_rfc3339();
        let preset = WorkspacePreset {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            icon,
            active_tab: snapshot.active_tab,
            active_panel: snapshot.active_panel,
            role_id: snapshot.role_id,
            voice_settings: snapshot.voice_settings,
            enabled_agent_ids: snapshot.enabled_agent_ids,
            default_agent_id: snapshot.default_agent_id,
            selected_local_model: snapshot.selected_local_model,
            connector_instance_ids: snapshot.connector_instance_ids,
            is_builtin: false,
            created_at: now.clone(),
            updated_at: now,
        };
        self.presets.push(preset.clone());
        self.save().ok();
        preset
    }

    pub fn update(&mut self, id: &str, name: Option<String>, description: Option<String>, icon: Option<String>) -> Option<WorkspacePreset> {
        let now = Utc::now().to_rfc3339();
        if let Some(p) = self.presets.iter_mut().find(|p| p.id == id && !p.is_builtin) {
            if let Some(n) = name { p.name = n; }
            if let Some(d) = description { p.description = d; }
            if let Some(i) = icon { p.icon = i; }
            p.updated_at = now;
            let result = p.clone();
            self.save().ok();
            Some(result)
        } else {
            None
        }
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let before = self.presets.len();
        self.presets.retain(|p| p.id != id || p.is_builtin);
        let removed = self.presets.len() < before;
        if removed { self.save().ok(); }
        removed
    }
}
