use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use chrono::Utc;
use tauri::Emitter;

// ─────────────────────────────────────────────
//  Core Event
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreEvent {
    pub id: String,
    pub event_type: String,
    pub source: String,
    pub timestamp: String,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl CoreEvent {
    pub fn new(event_type: impl Into<String>, source: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            source: source.into(),
            timestamp: Utc::now().to_rfc3339(),
            payload,
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

// ─────────────────────────────────────────────
//  Event Bus Stats
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventBusStats {
    pub total_published: u64,
    pub total_delivered: u64,
    pub total_dropped: u64,
    pub subscriber_count: usize,
    pub buffer_capacity: usize,
    pub backpressure_events: u64,
}

// ─────────────────────────────────────────────
//  Subscription
// ─────────────────────────────────────────────

struct Subscription {
    id: String,
    event_filter: Option<String>, // None = all events, Some = specific event_type
    sender: mpsc::Sender<CoreEvent>,
}

// ─────────────────────────────────────────────
//  Event Bus
// ─────────────────────────────────────────────

pub struct EventBus {
    subscribers: Arc<RwLock<Vec<Subscription>>>,
    stats: Arc<RwLock<EventBusStats>>,
    buffer_capacity: usize,
    running: Arc<AtomicBool>,
}

impl EventBus {
    pub fn new(buffer_capacity: usize) -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(EventBusStats {
                total_published: 0,
                total_delivered: 0,
                total_dropped: 0,
                subscriber_count: 0,
                buffer_capacity,
                backpressure_events: 0,
            })),
            buffer_capacity,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Subscribe to all events or a specific event type.
    /// Returns (subscription_id, receiver channel).
    pub async fn subscribe(
        &self,
        event_filter: Option<String>,
    ) -> (String, mpsc::Receiver<CoreEvent>) {
        let (tx, rx) = mpsc::channel(self.buffer_capacity);
        let id = Uuid::new_v4().to_string();
        let sub = Subscription {
            id: id.clone(),
            event_filter,
            sender: tx,
        };
        let mut subs = self.subscribers.write().await;
        subs.push(sub);
        let mut stats = self.stats.write().await;
        stats.subscriber_count = subs.len();
        (id, rx)
    }

    /// Unsubscribe by subscription ID.
    pub async fn unsubscribe(&self, sub_id: &str) {
        let mut subs = self.subscribers.write().await;
        subs.retain(|s| s.id != sub_id);
        let mut stats = self.stats.write().await;
        stats.subscriber_count = subs.len();
    }

    /// Publish an event to all matching subscribers.
    /// Uses try_send for backpressure - if a subscriber's buffer is full,
    /// the event is dropped for that subscriber.
    pub async fn publish(&self, event: CoreEvent) {
        if !self.running.load(Ordering::Relaxed) {
            return;
        }

        let subs = self.subscribers.read().await;
        let mut delivered = 0u64;
        let mut dropped = 0u64;
        let mut backpressure = 0u64;

        for sub in subs.iter() {
            // Check filter
            if let Some(ref filter) = sub.event_filter {
                if &event.event_type != filter {
                    continue;
                }
            }

            match sub.sender.try_send(event.clone()) {
                Ok(_) => delivered += 1,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    dropped += 1;
                    backpressure += 1;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    dropped += 1;
                }
            }
        }

        let mut stats = self.stats.write().await;
        stats.total_published += 1;
        stats.total_delivered += delivered;
        stats.total_dropped += dropped;
        stats.backpressure_events += backpressure;
    }

    /// Publish and emit as a Tauri event too (for frontend reactivity).
    pub async fn publish_and_emit(&self, event: CoreEvent, app: &tauri::AppHandle) {
        app.emit("core-event", &event).ok();
        self.publish(event).await;
    }

    /// Get current bus statistics.
    pub async fn stats(&self) -> EventBusStats {
        self.stats.read().await.clone()
    }

    /// Shut down the event bus - stops accepting new events.
    pub async fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
        let mut subs = self.subscribers.write().await;
        subs.clear();
    }

    /// Number of active subscribers.
    pub async fn subscriber_count(&self) -> usize {
        self.subscribers.read().await.len()
    }

    /// Remove closed subscriber channels (cleanup dead subscribers).
    pub async fn cleanup(&self) {
        let mut subs = self.subscribers.write().await;
        subs.retain(|s| !s.sender.is_closed());
        let mut stats = self.stats.write().await;
        stats.subscriber_count = subs.len();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ─────────────────────────────────────────────
//  Single backend-to-UI event door (W0.7)
// ─────────────────────────────────────────────

/// Emits the named event to the webview exactly as `app.emit` did
/// (frontend listeners unchanged) and mirrors it onto the core EventBus
/// so backend subscribers and observability see every UI-bound event.
/// Returns the emit result so call-site `.ok()` / `?` chains still work.
pub fn emit_ui<M>(manager: &M, name: &str, payload: impl serde::Serialize) -> tauri::Result<()>
where
    M: tauri::Manager<tauri::Wry> + tauri::Emitter<tauri::Wry>,
{
    let value = serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null);
    if let Some(state) = manager.try_state::<crate::AppState>() {
        let bus = state.core.core_event_bus.clone();
        let event = CoreEvent::new(name, "ui-emit", value);
        tauri::async_runtime::spawn(async move {
            bus.read().await.publish(event).await;
        });
    }
    manager.emit(name, &payload)
}
