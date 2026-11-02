//! BOUNDARY (W0.7): this is a PERSISTENCE concern (append-only twin
//! history for time travel), not an event delivery path. All backend-to-UI
//! delivery goes through event_bus::emit_ui.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use uuid::Uuid;

use crate::semantic_twin::SemanticGraph;

// ─────────────────────────────────────────────
//  Event Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    EntityCreated,
    EntityUpdated,
    EntityDeleted,
    StateChanged,
    HealthChanged,
    RelationCreated,
    RelationRemoved,
    PropertyChanged,
    PositionUpdated,
    AnomalyDetected,
    AnomalyResolved,
}

// ─────────────────────────────────────────────
//  Ledger Event
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LedgerEvent {
    pub id: String,
    pub timestamp: String,
    pub entity_id: String,
    pub entity_name: String,
    pub entity_type: String,
    pub event_type: EventType,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub properties: HashMap<String, serde_json::Value>,
    pub source: String, // which agent/system emitted this
    pub correlation_id: Option<String>, // link related events
}

// ─────────────────────────────────────────────
//  Snapshot
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Snapshot {
    pub id: String,
    pub timestamp: String,
    pub entity_count: usize,
    pub relation_count: usize,
    pub graph_json: String, // serialized SemanticGraph
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SnapshotInfo {
    pub id: String,
    pub timestamp: String,
    pub entity_count: usize,
    pub relation_count: usize,
}

// ─────────────────────────────────────────────
//  Event Ledger
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
pub struct EventLedger {
    pub events: Vec<LedgerEvent>,
    pub snapshots: Vec<Snapshot>,
    pub max_events: usize,
    pub snapshot_interval: usize, // create snapshot every N events
    events_since_snapshot: usize,
}

impl Default for EventLedger {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            snapshots: Vec::new(),
            max_events: 10_000,
            snapshot_interval: 100,
            events_since_snapshot: 0,
        }
    }
}

impl EventLedger {
    pub fn append(&mut self, mut event: LedgerEvent) {
        if event.id.is_empty() {
            event.id = Uuid::new_v4().to_string();
        }
        if event.timestamp.is_empty() {
            event.timestamp = Utc::now().to_rfc3339();
        }
        self.events.push(event);
        self.events_since_snapshot += 1;

        // Trim if over max
        if self.events.len() > self.max_events {
            let excess = self.events.len() - self.max_events;
            self.events.drain(0..excess);
        }
    }

    pub fn append_simple(
        &mut self,
        entity_id: &str,
        entity_name: &str,
        entity_type: &str,
        event_type: EventType,
        old_value: Option<serde_json::Value>,
        new_value: Option<serde_json::Value>,
        source: &str,
    ) -> LedgerEvent {
        let event = LedgerEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            entity_id: entity_id.to_string(),
            entity_name: entity_name.to_string(),
            entity_type: entity_type.to_string(),
            event_type,
            old_value,
            new_value,
            properties: HashMap::new(),
            source: source.to_string(),
            correlation_id: None,
        };
        self.append(event.clone());
        event
    }

    pub fn should_snapshot(&self) -> bool {
        self.events_since_snapshot >= self.snapshot_interval
    }

    pub fn create_snapshot(&mut self, graph: &SemanticGraph) -> SnapshotInfo {
        let graph_json = serde_json::to_string(graph).unwrap_or_default();
        let snapshot = Snapshot {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            entity_count: graph.entities.len(),
            relation_count: graph.relations.len(),
            graph_json,
        };
        let info = SnapshotInfo {
            id: snapshot.id.clone(),
            timestamp: snapshot.timestamp.clone(),
            entity_count: snapshot.entity_count,
            relation_count: snapshot.relation_count,
        };
        self.snapshots.push(snapshot);
        self.events_since_snapshot = 0;
        info
    }

    pub fn get_events_in_range(&self, start: &str, end: &str) -> Vec<&LedgerEvent> {
        self.events.iter()
            .filter(|e| e.timestamp.as_str() >= start && e.timestamp.as_str() <= end)
            .collect()
    }

    pub fn get_events_for_entity(&self, entity_id: &str) -> Vec<&LedgerEvent> {
        self.events.iter()
            .filter(|e| e.entity_id == entity_id)
            .collect()
    }

    pub fn get_events_by_type(&self, event_type: &EventType) -> Vec<&LedgerEvent> {
        self.events.iter()
            .filter(|e| &e.event_type == event_type)
            .collect()
    }

    pub fn get_nearest_snapshot_before(&self, timestamp: &str) -> Option<&Snapshot> {
        self.snapshots.iter()
            .filter(|s| s.timestamp.as_str() <= timestamp)
            .last()
    }

    pub fn get_events_after(&self, timestamp: &str) -> Vec<&LedgerEvent> {
        self.events.iter()
            .filter(|e| e.timestamp.as_str() > timestamp)
            .collect()
    }

    pub fn get_events_between(&self, start: &str, end: &str) -> Vec<&LedgerEvent> {
        self.events.iter()
            .filter(|e| e.timestamp.as_str() > start && e.timestamp.as_str() <= end)
            .collect()
    }

    pub fn snapshot_infos(&self) -> Vec<SnapshotInfo> {
        self.snapshots.iter().map(|s| SnapshotInfo {
            id: s.id.clone(),
            timestamp: s.timestamp.clone(),
            entity_count: s.entity_count,
            relation_count: s.relation_count,
        }).collect()
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn recent_events(&self, limit: usize) -> Vec<&LedgerEvent> {
        let start = if self.events.len() > limit { self.events.len() - limit } else { 0 };
        self.events[start..].iter().collect()
    }
}
