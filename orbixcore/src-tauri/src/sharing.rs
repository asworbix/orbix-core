use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Share Bundle
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BundleType {
    Workflow,
    Pipeline,
    Agent,
    Mixed,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ShareBundle {
    pub id: String,
    pub version: u32,
    pub bundle_type: BundleType,
    pub name: String,
    pub description: String,
    pub author: String,
    pub items: Vec<SharedItem>,
    pub created_at: String,
    pub app_version: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SharedItem {
    pub item_type: String,  // "workflow" | "pipeline" | "agent"
    pub original_id: String,
    pub name: String,
    pub data: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImportConflictStrategy {
    Skip,
    Replace,
    Duplicate,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub replaced: usize,
    pub errors: Vec<String>,
}

// ─────────────────────────────────────────────
//  Share Store
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ShareStore {
    pub shared_items: Vec<ShareRecord>,
    pub import_history: Vec<ImportRecord>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ShareRecord {
    pub id: String,
    pub item_type: String,
    pub item_id: String,
    pub name: String,
    pub bundle_id: Option<String>,
    pub shared_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ImportRecord {
    pub id: String,
    pub bundle_name: String,
    pub bundle_type: BundleType,
    pub item_count: usize,
    pub imported_at: String,
}

impl ShareStore {
    pub fn load() -> Self {
        let path = crate::storage::data_dir().join("sharing.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = crate::storage::data_dir().join("sharing.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = crate::storage::atomic_write(&path, json);
        }
    }

    /// Create an export bundle
    pub fn create_bundle(
        &mut self,
        name: String,
        description: String,
        author: String,
        items: Vec<SharedItem>,
    ) -> ShareBundle {
        let bundle_type = if items.len() == 1 {
            match items[0].item_type.as_str() {
                "workflow" => BundleType::Workflow,
                "pipeline" => BundleType::Pipeline,
                "agent" => BundleType::Agent,
                _ => BundleType::Mixed,
            }
        } else {
            BundleType::Mixed
        };

        let bundle = ShareBundle {
            id: Uuid::new_v4().to_string(),
            version: 1,
            bundle_type,
            name: name.clone(),
            description,
            author: author.clone(),
            items: items.clone(),
            created_at: Utc::now().to_rfc3339(),
            app_version: "1.0.0".into(),
        };

        // Track shared items
        for item in &items {
            self.shared_items.push(ShareRecord {
                id: Uuid::new_v4().to_string(),
                item_type: item.item_type.clone(),
                item_id: item.original_id.clone(),
                name: item.name.clone(),
                bundle_id: Some(bundle.id.clone()),
                shared_at: Utc::now().to_rfc3339(),
            });
        }
        self.save();

        bundle
    }

    /// Serialize bundle to JSON string for export
    pub fn export_bundle(bundle: &ShareBundle) -> String {
        serde_json::to_string_pretty(bundle).unwrap_or_default()
    }

    /// Parse a bundle from JSON string
    pub fn parse_bundle(json: &str) -> Result<ShareBundle, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid bundle: {}", e))
    }

    /// Record an import
    pub fn record_import(&mut self, bundle: &ShareBundle, result: &ImportResult) {
        self.import_history.push(ImportRecord {
            id: Uuid::new_v4().to_string(),
            bundle_name: bundle.name.clone(),
            bundle_type: bundle.bundle_type.clone(),
            item_count: result.imported + result.replaced,
            imported_at: Utc::now().to_rfc3339(),
        });
        if self.import_history.len() > 50 {
            self.import_history.truncate(50);
        }
        self.save();
    }

    pub fn is_shared(&self, item_id: &str) -> bool {
        self.shared_items.iter().any(|s| s.item_id == item_id)
    }

    pub fn list_shared(&self) -> Vec<ShareRecord> {
        self.shared_items.clone()
    }

    pub fn list_imports(&self) -> Vec<ImportRecord> {
        self.import_history.clone()
    }
}
