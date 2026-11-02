//! The process board (W2.8): a CRM-style kanban of stages and items
//! (leads, tickets, deals) that move through columns. This is the sales
//! "pipeline" the user sees - a data board, NOT an execution pipeline.
//! Execution lives in run.rs (Run) and workflow.rs. The types here are
//! named Board* so code never conflates the two; the user-facing feature
//! and the persisted pipeline.json keep the domain term "pipeline".

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Pipeline Stage Definitions
// ─────────────────────────────────────────────

/// A named pipeline with configurable stages
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProcessBoard {
    pub id: String,
    pub name: String,
    pub role_id: String,
    pub stages: Vec<BoardStage>,
    pub stale_threshold_hours: Option<u64>,
    pub created_at: String,
    /// Seeded demo boards: excluded from proactive nudges (stale-item
    /// suggestions about fabricated data confused users).
    #[serde(default)]
    pub demo: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BoardStage {
    pub id: String,
    pub name: String,
    pub order: usize,
    pub color: String,
}

impl ProcessBoard {
    pub fn new(name: String, role_id: String, stages: Vec<(String, String)>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            role_id,
            demo: false,
            stages: stages.into_iter().enumerate().map(|(i, (name, color))| BoardStage {
                id: Uuid::new_v4().to_string(),
                name,
                order: i,
                color,
            }).collect(),
            stale_threshold_hours: Some(48),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

// ─────────────────────────────────────────────
//  Pipeline Item
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemSource {
    Email,
    Webhook,
    FileWatch,
    Manual,
    Workflow,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Unread,
    Read,
    Processed,
    Archived,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StageTransition {
    pub from_stage: Option<String>,
    pub to_stage: String,
    pub timestamp: String,
    pub note: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BoardItem {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub source: ItemSource,
    pub source_id: Option<String>,
    pub source_detail: Option<String>,
    pub pipeline_id: Option<String>,
    pub stage_id: Option<String>,
    pub status: ItemStatus,
    pub data: Option<serde_json::Value>,
    pub ai_summary: Option<String>,
    pub history: Vec<StageTransition>,
    pub entered_stage_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl BoardItem {
    pub fn new(title: String, snippet: String, source: ItemSource) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            snippet,
            source,
            source_id: None,
            source_detail: None,
            pipeline_id: None,
            stage_id: None,
            status: ItemStatus::Unread,
            data: None,
            ai_summary: None,
            history: Vec::new(),
            entered_stage_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn move_to_stage(&mut self, stage_id: String, note: Option<String>) {
        let transition = StageTransition {
            from_stage: self.stage_id.clone(),
            to_stage: stage_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            note,
        };
        self.history.push(transition);
        self.stage_id = Some(stage_id);
        self.entered_stage_at = Some(Utc::now().to_rfc3339());
        self.updated_at = Utc::now().to_rfc3339();
    }
}

// ─────────────────────────────────────────────
//  Pipeline Store
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct BoardStore {
    pub pipelines: Vec<ProcessBoard>,
    pub items: Vec<BoardItem>,
}

impl BoardStore {
    pub fn load() -> Self {
        let data_dir = crate::storage::data_dir();
        let path = data_dir.join("pipeline.json");
        let mut store: Self = if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };
        // Migration: older builds seeded "Logi" demo boards into user
        // stores (the seeder is a stub now). Mark them demo so proactive
        // nudges stop nagging about fabricated data. Name-exact match on
        // the legacy seed set only.
        const LEGACY_SEED_BOARDS: &[&str] =
            &["Order Fulfillment", "Freight Tenders", "Returns & RMA"];
        for p in store.pipelines.iter_mut() {
            if LEGACY_SEED_BOARDS.contains(&p.name.as_str()) {
                p.demo = true;
            }
        }
        store
    }

    pub fn save(&self) {
        let data_dir = crate::storage::data_dir();
        let _ = std::fs::create_dir_all(&data_dir);
        let path = data_dir.join("pipeline.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = crate::storage::atomic_write(&path, json);
        }
    }

    /// Ensure default pipelines exist for a role
    /// Seed one generic starter board the first time the panel opens.
    /// Boards are user-defined from there; the old role-keyed seeding
    /// went away with role profiles.
    pub fn ensure_seed_pipelines(&mut self) {
        if !self.pipelines.is_empty() {
            return;
        }
        self.pipelines.push(ProcessBoard::new(
            "Tasks".into(),
            "default".into(),
            vec![
                ("To do".into(), "#7b93ff".into()),
                ("In progress".into(), "#fbbf24".into()),
                ("Done".into(), "#34d399".into()),
            ],
        ));
        self.save();
    }

    /// Get items for the unified inbox (all non-archived)
    pub fn inbox_items(&self, source_filter: Option<&ItemSource>, limit: usize) -> Vec<&BoardItem> {
        self.items.iter()
            .filter(|i| i.status != ItemStatus::Archived)
            .filter(|i| source_filter.map_or(true, |sf| &i.source == sf))
            .take(limit)
            .collect()
    }

    /// Get items for a specific pipeline grouped by stage
    pub fn pipeline_items(&self, pipeline_id: &str) -> Vec<&BoardItem> {
        self.items.iter()
            .filter(|i| i.pipeline_id.as_deref() == Some(pipeline_id))
            .filter(|i| i.status != ItemStatus::Archived)
            .collect()
    }

    /// Get stale items (in a stage longer than threshold)
    pub fn stale_items(&self) -> Vec<&BoardItem> {
        let now = Utc::now();
        self.items.iter()
            .filter(|item| {
                if let (Some(pid), Some(entered)) = (&item.pipeline_id, &item.entered_stage_at) {
                    if let Some(pipeline) = self.pipelines.iter().find(|p| &p.id == pid) {
                        if pipeline.demo { return false; }
                        let threshold_hours = pipeline.stale_threshold_hours.unwrap_or(48);
                        if let Ok(entered_dt) = entered.parse::<chrono::DateTime<Utc>>() {
                            let hours = (now - entered_dt).num_hours();
                            return hours > threshold_hours as i64;
                        }
                    }
                }
                false
            })
            .collect()
    }
}

// ─────────────────────────────────────────────
//  Default Pipelines per Role
// ─────────────────────────────────────────────


/// Summarize item data using local LLM
pub async fn ai_summarize_item(title: &str, snippet: &str, data: Option<&serde_json::Value>) -> Result<String, String> {
    let data_str = data.map(|d| serde_json::to_string_pretty(d).unwrap_or_default()).unwrap_or_default();
    let prompt = format!(
        "Summarize this pipeline item in 1-2 concise sentences:\n\nTitle: {}\nSnippet: {}\nData: {}\n\nSummary:",
        title, snippet, data_str
    );

    let client = reqwest::Client::new();
    let resp = client.post("http://127.0.0.1:8787/v1/chat/completions")
        .json(&serde_json::json!({
            "model": "local",
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.3,
            "max_tokens": 150,
        }))
        .send().await
        .map_err(|e| format!("LLM error: {}", e))?;

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(body["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string())
}
