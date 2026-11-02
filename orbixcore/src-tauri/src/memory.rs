use serde::{Deserialize, Serialize};
use crate::storage;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub value: String,
    pub category: MemoryCategory,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryCategory {
    Fact,
    Preference,
    Context,
    Skill,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct MemoryStore {
    pub entries: Vec<MemoryEntry>,
}

const MEMORY_FILE: &str = "memory.json";

impl MemoryStore {
    pub fn load() -> Self {
        storage::load_json::<MemoryStore>(MEMORY_FILE).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        storage::save_json(MEMORY_FILE, self)
    }

    pub fn add(&mut self, key: String, value: String, category: MemoryCategory, source: String) -> MemoryEntry {
        let now = chrono::Utc::now().to_rfc3339();

        // Update existing entry with same key, or create new
        if let Some(existing) = self.entries.iter_mut().find(|e| e.key == key) {
            existing.value = value;
            existing.category = category;
            existing.updated_at = now;
            existing.source = source;
            return existing.clone();
        }

        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            key,
            value,
            category,
            source,
            created_at: now.clone(),
            updated_at: now,
        };
        self.entries.push(entry.clone());
        entry
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let len_before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < len_before
    }

    pub fn update(&mut self, id: &str, value: String) -> Option<MemoryEntry> {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.value = value;
            entry.updated_at = chrono::Utc::now().to_rfc3339();
            Some(entry.clone())
        } else {
            None
        }
    }

    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.key.to_lowercase().contains(&q) || e.value.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn by_category(&self, category: &MemoryCategory) -> Vec<&MemoryEntry> {
        self.entries.iter().filter(|e| &e.category == category).collect()
    }

    /// Build a system prompt section from relevant memories
    pub fn build_context_prompt(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        parts.push("## User Memory\nHere are things you know about this user:\n".to_string());

        let facts = self.by_category(&MemoryCategory::Fact);
        if !facts.is_empty() {
            parts.push("### Facts".to_string());
            for f in facts {
                parts.push(format!("- {}: {}", f.key, f.value));
            }
        }

        let prefs = self.by_category(&MemoryCategory::Preference);
        if !prefs.is_empty() {
            parts.push("### Preferences".to_string());
            for p in prefs {
                parts.push(format!("- {}: {}", p.key, p.value));
            }
        }

        let ctx = self.by_category(&MemoryCategory::Context);
        if !ctx.is_empty() {
            parts.push("### Context".to_string());
            for c in ctx {
                // Truncate long values to keep system prompt reasonable
                let val = if c.value.len() > 500 { format!("{}...", &c.value[..500]) } else { c.value.clone() };
                parts.push(format!("- {}: {}", c.key, val));
            }
        }

        let skills = self.by_category(&MemoryCategory::Skill);
        if !skills.is_empty() {
            parts.push("### Learned Skills".to_string());
            for s in skills {
                parts.push(format!("- {}: {}", s.key, s.value));
            }
        }

        parts.join("\n")
    }
}
