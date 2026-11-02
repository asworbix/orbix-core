use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

// ─────────────────────────────────────────────
//  Knowledge Base Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct KBArticle {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: KBSource,
    pub word_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KBSource {
    Manual,
    Chat,
    Import,
}

// ─────────────────────────────────────────────
//  Search Index (TF-IDF-like)
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SearchResult {
    pub article_id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
    pub category: String,
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

fn term_frequency(tokens: &[String]) -> HashMap<String, f64> {
    let mut tf: HashMap<String, f64> = HashMap::new();
    let len = tokens.len() as f64;
    if len == 0.0 { return tf; }
    for t in tokens {
        *tf.entry(t.clone()).or_default() += 1.0;
    }
    for v in tf.values_mut() {
        *v /= len;
    }
    tf
}

// ─────────────────────────────────────────────
//  Knowledge Base Store
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct KBStore {
    pub articles: Vec<KBArticle>,
    pub categories: Vec<String>,
}

impl KBStore {
    pub fn kb_dir() -> PathBuf {
        let dir = crate::storage::data_dir().join("kb");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn load() -> Self {
        let path = Self::kb_dir().join("index.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::kb_dir().join("index.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = crate::storage::atomic_write(&path, json);
        }
    }

    // ── CRUD ──

    pub fn list(&self, category: Option<&str>, tag: Option<&str>) -> Vec<KBArticle> {
        self.articles.iter()
            .filter(|a| category.map_or(true, |c| a.category == c))
            .filter(|a| tag.map_or(true, |t| a.tags.iter().any(|at| at == t)))
            .cloned()
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&KBArticle> {
        self.articles.iter().find(|a| a.id == id)
    }

    pub fn create(
        &mut self,
        title: String,
        content: String,
        category: String,
        tags: Vec<String>,
        source: KBSource,
    ) -> KBArticle {
        let word_count = content.split_whitespace().count();
        let now = Utc::now().to_rfc3339();

        // Ensure category exists
        if !self.categories.contains(&category) {
            self.categories.push(category.clone());
        }

        let article = KBArticle {
            id: Uuid::new_v4().to_string(),
            title,
            content,
            category,
            tags,
            source,
            word_count,
            created_at: now.clone(),
            updated_at: now,
        };
        self.articles.insert(0, article.clone());
        self.save();
        article
    }

    pub fn update(
        &mut self,
        id: &str,
        title: Option<String>,
        content: Option<String>,
        category: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Option<KBArticle> {
        if let Some(a) = self.articles.iter_mut().find(|a| a.id == id) {
            if let Some(t) = title { a.title = t; }
            if let Some(c) = content {
                a.word_count = c.split_whitespace().count();
                a.content = c;
            }
            if let Some(c) = category {
                if !self.categories.contains(&c) {
                    self.categories.push(c.clone());
                }
                a.category = c;
            }
            if let Some(t) = tags { a.tags = t; }
            a.updated_at = Utc::now().to_rfc3339();
            let result = a.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let len = self.articles.len();
        self.articles.retain(|a| a.id != id);
        if self.articles.len() != len {
            self.save();
            true
        } else {
            false
        }
    }

    // ── Search ──

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() { return vec![]; }

        let doc_count = self.articles.len() as f64;
        if doc_count == 0.0 { return vec![]; }

        // Compute IDF for query terms
        let mut idf: HashMap<String, f64> = HashMap::new();
        for qt in &query_tokens {
            let doc_freq = self.articles.iter()
                .filter(|a| {
                    let text = format!("{} {} {}", a.title, a.content, a.tags.join(" ")).to_lowercase();
                    text.contains(qt)
                })
                .count() as f64;
            let idf_val = if doc_freq > 0.0 {
                (doc_count / doc_freq).ln() + 1.0
            } else {
                0.0
            };
            idf.insert(qt.clone(), idf_val);
        }

        // Score each article
        let mut scored: Vec<SearchResult> = self.articles.iter()
            .filter_map(|a| {
                let full_text = format!("{} {} {} {}", a.title, a.title, a.content, a.tags.join(" "));
                let tokens = tokenize(&full_text);
                let tf = term_frequency(&tokens);

                let mut score = 0.0;
                for qt in &query_tokens {
                    let tf_val = tf.get(qt).copied().unwrap_or(0.0);
                    let idf_val = idf.get(qt).copied().unwrap_or(0.0);
                    score += tf_val * idf_val;
                    // Boost for title match
                    if a.title.to_lowercase().contains(qt) {
                        score += 0.5;
                    }
                }

                if score > 0.0 {
                    // Generate snippet
                    let snippet = generate_snippet(&a.content, &query_tokens);
                    Some(SearchResult {
                        article_id: a.id.clone(),
                        title: a.title.clone(),
                        snippet,
                        score,
                        category: a.category.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }

    /// Get context for RAG - returns relevant chunks for a query
    pub fn get_context(&self, query: &str, max_chars: usize) -> Vec<KBContext> {
        let results = self.search(query, 3);
        let mut contexts = Vec::new();
        let mut total_chars = 0;

        for r in results {
            if let Some(article) = self.get(&r.article_id) {
                let chunk = if article.content.len() > 1500 {
                    // Find the most relevant chunk
                    best_chunk(&article.content, &tokenize(query), 1500)
                } else {
                    article.content.clone()
                };

                if total_chars + chunk.len() > max_chars { break; }
                total_chars += chunk.len();

                contexts.push(KBContext {
                    article_id: article.id.clone(),
                    title: article.title.clone(),
                    chunk,
                    relevance: r.score,
                });
            }
        }
        contexts
    }

    /// Semantic retrieval (W3): rank articles by embedding cosine similarity
    /// when an embeddings endpoint is configured (ORBIX_EMBED_ENDPOINT with a
    /// loaded embedding model), falling back to keyword get_context otherwise.
    /// The generation models the app ships with do not embed well, so this is
    /// only better than keywords with a real embedding model loaded.
    pub async fn semantic_context(&self, query: &str, max_chars: usize) -> Vec<KBContext> {
        let Some(endpoint) = crate::local_llm::embed_endpoint() else {
            return self.get_context(query, max_chars);
        };
        if self.articles.is_empty() {
            return Vec::new();
        }
        // Candidate = title + a leading snippet, so the embedding sees topic
        // and content without paying for the whole article.
        let candidates: Vec<String> = self.articles.iter()
            .map(|a| format!("{}\n{}", a.title, crate::truncate_str(&a.content, 500)))
            .collect();
        let ranked = match crate::embeddings::semantic_rank(&endpoint, query, &candidates, 3).await {
            Ok(r) => r,
            Err(_) => return self.get_context(query, max_chars), // fall back on any error
        };
        let mut contexts = Vec::new();
        let mut total = 0usize;
        for (idx, score) in ranked {
            let Some(article) = self.articles.get(idx) else { continue };
            let chunk = if article.content.len() > 1500 {
                best_chunk(&article.content, &tokenize(query), 1500)
            } else {
                article.content.clone()
            };
            if total + chunk.len() > max_chars { break; }
            total += chunk.len();
            contexts.push(KBContext {
                article_id: article.id.clone(),
                title: article.title.clone(),
                chunk,
                relevance: score as f64,
            });
        }
        contexts
    }

    pub fn list_categories(&self) -> Vec<String> {
        self.categories.clone()
    }

    pub fn list_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.articles.iter()
            .flat_map(|a| a.tags.clone())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct KBContext {
    pub article_id: String,
    pub title: String,
    pub chunk: String,
    pub relevance: f64,
}

// ─────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────

fn generate_snippet(content: &str, query_tokens: &[String]) -> String {
    let lower = content.to_lowercase();
    // Find first occurrence of any query token
    let mut best_pos = None;
    for qt in query_tokens {
        if let Some(pos) = lower.find(qt) {
            if best_pos.map_or(true, |bp: usize| pos < bp) {
                best_pos = Some(pos);
            }
        }
    }

    let start = best_pos.unwrap_or(0).saturating_sub(50);
    let end = (start + 200).min(content.len());

    let mut snippet = content[start..end].to_string();
    if start > 0 { snippet = format!("...{}", snippet); }
    if end < content.len() { snippet.push_str("..."); }
    snippet
}

fn best_chunk(content: &str, query_tokens: &[String], max_len: usize) -> String {
    let lower = content.to_lowercase();
    let mut best_pos = 0;
    let mut best_score = 0;

    // Slide window to find best chunk
    let step = 200;
    let mut pos = 0;
    while pos < content.len() {
        let end = (pos + max_len).min(content.len());
        let window = &lower[pos..end];
        let score: usize = query_tokens.iter()
            .map(|qt| window.matches(qt).count())
            .sum();
        if score > best_score {
            best_score = score;
            best_pos = pos;
        }
        pos += step;
    }

    let end = (best_pos + max_len).min(content.len());
    content[best_pos..end].to_string()
}
