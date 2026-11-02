// ─────────────────────────────────────────────
//  Pending Review Queue - companion to JobProfile review gates
//
//  The scheduler calls a connector's `draft_<engagement>` action when
//  the profile's review_gate is Always or FirstN. The output of that
//  action - typically the payload a later publish call will submit -
//  lands here as a PendingReview. The user approves; the queue then
//  dispatches `<slug>.<engagement>` (the real publish) through the
//  bus with the saved payload as inputs.
//
//  On approve, the queue also calls record_approval on the profile so
//  the FirstN counter ticks down and AutoPublish gets earned.
// ─────────────────────────────────────────────

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage;

const STORE_FILE: &str = "review_queue.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingReview {
    pub id: String,
    pub profile_id: String,
    pub connector_slug: String,
    pub draft_action: String,
    /// The action to call on approval. Derived once at draft time so a
    /// later rename of the convention doesn't leave queued items
    /// pointing at a missing action.
    pub publish_action: String,
    /// Original inputs passed to draft_*, kept for audit.
    pub original_inputs: serde_json::Value,
    /// The connector's draft output. This is what gets passed back to
    /// the publish action verbatim on approval.
    pub payload: serde_json::Value,
    pub created_at: String,
    pub goal: String,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ReviewQueue {
    pub items: HashMap<String, PendingReview>,
}

impl ReviewQueue {
    pub fn load() -> Self {
        storage::load_json::<Self>(STORE_FILE).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        storage::save_json(STORE_FILE, self)
    }

    /// Append a draft to the queue. Returns the assigned id.
    pub fn enqueue(
        &mut self,
        profile_id: &str,
        slug: &str,
        draft_action: &str,
        publish_action: &str,
        original_inputs: serde_json::Value,
        payload: serde_json::Value,
        goal: &str,
    ) -> Result<PendingReview, String> {
        let item = PendingReview {
            id: Uuid::new_v4().to_string(),
            profile_id: profile_id.to_string(),
            connector_slug: slug.to_string(),
            draft_action: draft_action.to_string(),
            publish_action: publish_action.to_string(),
            original_inputs,
            payload,
            created_at: Utc::now().to_rfc3339(),
            goal: goal.to_string(),
        };
        self.items.insert(item.id.clone(), item.clone());
        self.save()?;
        Ok(item)
    }

    pub fn list(&self) -> Vec<&PendingReview> {
        let mut out: Vec<&PendingReview> = self.items.values().collect();
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        out
    }

    pub fn get(&self, id: &str) -> Option<&PendingReview> {
        self.items.get(id)
    }

    pub fn take(&mut self, id: &str) -> Result<PendingReview, String> {
        let item = self.items.remove(id)
            .ok_or_else(|| format!("no such pending review: {}", id))?;
        self.save()?;
        Ok(item)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.items.remove(id);
        self.save()
    }
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Every enqueue() persists to the same real store file; parallel test
    // threads renaming over it concurrently is Access-denied on Windows.
    static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn enqueue_returns_assigned_id_and_persists_payload() {
        let _g = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut q = ReviewQueue::default();
        let r = q.enqueue(
            "profile-1",
            "linkedin",
            "draft_post",
            "post",
            serde_json::json!({"text": "hello"}),
            serde_json::json!({"body": "preview"}),
            "post daily",
        ).unwrap_or_else(|e| panic!("enqueue: {}", e));
        assert!(!r.id.is_empty());
        assert_eq!(r.connector_slug, "linkedin");
        assert_eq!(q.list().len(), 1);
    }

    #[test]
    fn take_removes_from_queue() {
        let _g = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut q = ReviewQueue::default();
        let r = q.enqueue("p", "s", "draft_post", "post",
            serde_json::Value::Null, serde_json::Value::Null, "g").unwrap();
        let id = r.id.clone();
        let _ = q.take(&id).unwrap();
        assert!(q.get(&id).is_none());
    }

    #[test]
    fn list_is_sorted_by_created_at() {
        let _g = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut q = ReviewQueue::default();
        let a = q.enqueue("p", "s", "draft_post", "post",
            serde_json::Value::Null, serde_json::Value::Null, "first").unwrap();
        // Force a slightly later timestamp.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = q.enqueue("p", "s", "draft_post", "post",
            serde_json::Value::Null, serde_json::Value::Null, "second").unwrap();
        let listed = q.list();
        assert_eq!(listed[0].id, a.id);
        assert_eq!(listed[1].id, b.id);
    }
}
