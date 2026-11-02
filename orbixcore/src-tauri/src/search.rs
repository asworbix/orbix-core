use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Search Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ConversationSearchResult {
    pub conversation_id: String,
    pub conversation_title: String,
    pub message_id: String,
    pub role: String,
    pub content_snippet: String,
    pub score: f64,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SearchFilters {
    pub role: Option<String>,       // "user" | "assistant"
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub conversation_id: Option<String>,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self { role: None, date_from: None, date_to: None, conversation_id: None }
    }
}

// ─────────────────────────────────────────────
//  Inverted Index
// ─────────────────────────────────────────────

/// term → vec of (conversation_id, message_id, position_score)
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct SearchIndex {
    pub index: HashMap<String, Vec<IndexEntry>>,
    pub conversations: Vec<IndexedConversation>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct IndexEntry {
    pub conversation_id: String,
    pub message_id: String,
    pub role: String,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct IndexedConversation {
    pub id: String,
    pub title: String,
    pub message_count: usize,
    pub indexed_at: String,
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

impl SearchIndex {
    pub fn load() -> Self {
        let dir = crate::storage::data_dir();
        let path = dir.join("conversations_index.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let dir = crate::storage::data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("conversations_index.json");
        if let Ok(json) = serde_json::to_string(self) {
            let _ = crate::storage::atomic_write(&path, json);
        }
    }

    /// Index a conversation (incremental - replaces if already indexed)
    pub fn index_conversation(
        &mut self,
        conversation_id: &str,
        title: &str,
        messages: &[(String, String, String, String)], // (msg_id, role, content, timestamp)
    ) {
        // Remove old entries for this conversation
        for entries in self.index.values_mut() {
            entries.retain(|e| e.conversation_id != conversation_id);
        }
        self.conversations.retain(|c| c.id != conversation_id);

        // Index each message
        for (msg_id, role, content, timestamp) in messages {
            let tokens = tokenize(content);
            let unique_tokens: std::collections::HashSet<String> = tokens.into_iter().collect();
            for token in unique_tokens {
                self.index.entry(token).or_default().push(IndexEntry {
                    conversation_id: conversation_id.to_string(),
                    message_id: msg_id.clone(),
                    role: role.clone(),
                    timestamp: timestamp.clone(),
                });
            }
        }

        self.conversations.push(IndexedConversation {
            id: conversation_id.to_string(),
            title: title.to_string(),
            message_count: messages.len(),
            indexed_at: Utc::now().to_rfc3339(),
        });

        // Clean up empty index entries
        self.index.retain(|_, v| !v.is_empty());
        self.save();
    }

    /// Remove a conversation from the index
    pub fn remove_conversation(&mut self, conversation_id: &str) {
        for entries in self.index.values_mut() {
            entries.retain(|e| e.conversation_id != conversation_id);
        }
        self.conversations.retain(|c| c.id != conversation_id);
        self.index.retain(|_, v| !v.is_empty());
        self.save();
    }

    /// Search the index
    pub fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        limit: usize,
    ) -> Vec<ConversationSearchResult> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() { return vec![]; }

        // Collect matching entries with scores
        let mut scored: HashMap<String, (f64, IndexEntry)> = HashMap::new(); // message_id → (score, entry)

        let doc_count = self.conversations.len() as f64;
        if doc_count == 0.0 { return vec![]; }

        for qt in &query_tokens {
            if let Some(entries) = self.index.get(qt) {
                // IDF
                let df = entries.iter()
                    .map(|e| &e.conversation_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len() as f64;
                let idf = if df > 0.0 { (doc_count / df).ln() + 1.0 } else { 0.0 };

                for entry in entries {
                    // Apply filters
                    if let Some(ref role) = filters.role {
                        if &entry.role != role { continue; }
                    }
                    if let Some(ref cid) = filters.conversation_id {
                        if &entry.conversation_id != cid { continue; }
                    }
                    if let Some(ref from) = filters.date_from {
                        if entry.timestamp < *from { continue; }
                    }
                    if let Some(ref to) = filters.date_to {
                        if entry.timestamp > *to { continue; }
                    }

                    let key = entry.message_id.clone();
                    let e = scored.entry(key).or_insert((0.0, entry.clone()));
                    e.0 += idf;
                }
            }
        }

        // Convert to results
        let mut results: Vec<ConversationSearchResult> = scored.into_iter()
            .map(|(_, (score, entry))| {
                let title = self.conversations.iter()
                    .find(|c| c.id == entry.conversation_id)
                    .map(|c| c.title.clone())
                    .unwrap_or_else(|| "Untitled".into());

                ConversationSearchResult {
                    conversation_id: entry.conversation_id,
                    conversation_title: title,
                    message_id: entry.message_id,
                    role: entry.role,
                    content_snippet: String::new(), // filled by caller with actual content
                    score,
                    timestamp: entry.timestamp,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    pub fn indexed_count(&self) -> usize {
        self.conversations.len()
    }

    pub fn term_count(&self) -> usize {
        self.index.len()
    }
}
