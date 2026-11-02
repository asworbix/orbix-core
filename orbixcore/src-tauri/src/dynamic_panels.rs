// Dynamic UI runtime - Bob (or the user) describes a panel as a JSON spec
// of typed blocks; the frontend renders it via <DynamicPanel>. Storage is
// a single JSON file under the orbix data dir.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A single render unit inside a panel. The schema is intentionally loose:
/// `kind` selects the renderer; the rest of the props are passed through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub kind: String,
    #[serde(flatten)]
    pub props: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicPanel {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub pinned: bool,
    /// Optional binding to a conversation thread. None = global / workspace.
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Who created this - `"bob"`, `"user"`, or an agent id.
    #[serde(default = "default_author")]
    pub created_by: String,
}

fn default_author() -> String {
    "user".to_string()
}

/// Patch shape - every field optional.
#[derive(Debug, Default, Deserialize)]
pub struct DynamicPanelPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub blocks: Option<Vec<Block>>,
    pub pinned: Option<bool>,
    pub conversation_id: Option<Option<String>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DynamicPanelsStore {
    #[serde(default)]
    pub panels: Vec<DynamicPanel>,
}

impl DynamicPanelsStore {
    pub fn load() -> Self {
        super::storage::load_json::<DynamicPanelsStore>("dynamic_panels.json").unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        super::storage::save_json("dynamic_panels.json", self)
    }

    pub fn list(&self) -> Vec<DynamicPanel> {
        self.panels.clone()
    }

    pub fn get(&self, id: &str) -> Option<DynamicPanel> {
        self.panels.iter().find(|p| p.id == id).cloned()
    }

    /// Create or replace by id. If `id` is empty, a new uuid is assigned.
    pub fn upsert(&mut self, mut panel: DynamicPanel, author: &str) -> DynamicPanel {
        let now = Utc::now().to_rfc3339();
        if panel.id.is_empty() {
            panel.id = Uuid::new_v4().to_string();
            panel.created_at = now.clone();
            panel.created_by = author.to_string();
        }
        panel.updated_at = now;

        if let Some(existing) = self.panels.iter_mut().find(|p| p.id == panel.id) {
            *existing = panel.clone();
        } else {
            self.panels.push(panel.clone());
        }
        panel
    }

    pub fn patch(&mut self, id: &str, patch: DynamicPanelPatch) -> Result<DynamicPanel, String> {
        let p = self
            .panels
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("panel {id} not found"))?;
        if let Some(t) = patch.title {
            p.title = t;
        }
        if let Some(d) = patch.description {
            p.description = Some(d);
        }
        if let Some(b) = patch.blocks {
            p.blocks = b;
        }
        if let Some(pin) = patch.pinned {
            p.pinned = pin;
        }
        if let Some(c) = patch.conversation_id {
            p.conversation_id = c;
        }
        p.updated_at = Utc::now().to_rfc3339();
        Ok(p.clone())
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let len_before = self.panels.len();
        self.panels.retain(|p| p.id != id);
        self.panels.len() != len_before
    }
}
