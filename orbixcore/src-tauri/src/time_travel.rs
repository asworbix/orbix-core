use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::semantic_twin::{SemanticGraph, TwinEntity, EntityState, EntityHealth};
use crate::event_ledger::{EventLedger, EventType, LedgerEvent};

// ─────────────────────────────────────────────
//  Diff Structures
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FieldChange {
    pub field: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EntityDiff {
    pub entity_id: String,
    pub entity_name: String,
    pub entity_type: String,
    pub changes: Vec<FieldChange>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TwinDiff {
    pub t1: String,
    pub t2: String,
    pub entities_added: Vec<EntitySummary>,
    pub entities_removed: Vec<EntitySummary>,
    pub entities_changed: Vec<EntityDiff>,
    pub relations_added: usize,
    pub relations_removed: usize,
    pub total_events: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EntitySummary {
    pub id: String,
    pub name: String,
    pub entity_type: String,
}

// ─────────────────────────────────────────────
//  Replay Frame
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReplayFrame {
    pub frame_index: usize,
    pub timestamp: String,
    pub events: Vec<ReplayEvent>,
    pub entity_states: Vec<EntitySnapshot>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReplayEvent {
    pub entity_id: String,
    pub entity_name: String,
    pub event_type: String,
    pub description: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EntitySnapshot {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub state: String,
    pub health: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

// ─────────────────────────────────────────────
//  Time Travel Engine
// ─────────────────────────────────────────────

/// Reconstruct the graph state at a given timestamp by:
/// 1. Finding the nearest snapshot before that timestamp
/// 2. Replaying all events between the snapshot and the target time
pub fn state_at(ledger: &EventLedger, current_graph: &SemanticGraph, timestamp: &str) -> SemanticGraph {
    // If we have a snapshot before the timestamp, start from there
    if let Some(snapshot) = ledger.get_nearest_snapshot_before(timestamp) {
        if let Ok(mut graph) = serde_json::from_str::<SemanticGraph>(&snapshot.graph_json) {
            // Replay events between snapshot and target time
            let events = ledger.get_events_between(&snapshot.timestamp, timestamp);
            apply_events_to_graph(&mut graph, &events);
            return graph;
        }
    }

    // No snapshot available - reconstruct from current graph minus future events
    // This is a simplified approach: take current graph, and apply only events up to timestamp
    // from the earliest known event
    let mut graph = SemanticGraph {
        entities: Vec::new(),
        relations: Vec::new(),
        metadata: current_graph.metadata.clone(),
    };

    // Rebuild by replaying all events from start to timestamp
    let events: Vec<&LedgerEvent> = ledger.events.iter()
        .filter(|e| e.timestamp.as_str() <= timestamp)
        .collect();

    for event in &events {
        match event.event_type {
            EventType::EntityCreated => {
                if let Some(new_val) = &event.new_value {
                    if let Ok(entity) = serde_json::from_value::<TwinEntity>(new_val.clone()) {
                        graph.entities.push(entity);
                    }
                }
            }
            EventType::EntityDeleted => {
                graph.entities.retain(|e| e.id != event.entity_id);
                graph.relations.retain(|r| r.source_id != event.entity_id && r.target_id != event.entity_id);
            }
            EventType::StateChanged | EventType::EntityUpdated => {
                if let Some(entity) = graph.entities.iter_mut().find(|e| e.id == event.entity_id) {
                    if let Some(new_val) = &event.new_value {
                        if let Some(state_str) = new_val.as_str() {
                            if let Ok(state) = serde_json::from_value::<EntityState>(serde_json::json!(state_str)) {
                                entity.state = state;
                            }
                        }
                    }
                }
            }
            EventType::HealthChanged => {
                if let Some(entity) = graph.entities.iter_mut().find(|e| e.id == event.entity_id) {
                    if let Some(new_val) = &event.new_value {
                        if let Some(health_str) = new_val.as_str() {
                            if let Ok(health) = serde_json::from_value::<EntityHealth>(serde_json::json!(health_str)) {
                                entity.health = health;
                            }
                        }
                    }
                }
            }
            EventType::PositionUpdated => {
                if let Some(entity) = graph.entities.iter_mut().find(|e| e.id == event.entity_id) {
                    if let Some(new_val) = &event.new_value {
                        if let Some(obj) = new_val.as_object() {
                            if let Some(lat) = obj.get("lat").and_then(|v| v.as_f64()) {
                                entity.lat = Some(lat);
                            }
                            if let Some(lng) = obj.get("lng").and_then(|v| v.as_f64()) {
                                entity.lng = Some(lng);
                            }
                        }
                    }
                }
            }
            _ => {} // RelationCreated, RelationRemoved, AnomalyDetected, etc.
        }
    }

    graph.metadata.entity_count = graph.entities.len();
    graph.metadata.relation_count = graph.relations.len();
    graph
}

/// Compare two points in time and return what changed
pub fn diff(ledger: &EventLedger, t1: &str, t2: &str) -> TwinDiff {
    let events = ledger.get_events_between(t1, t2);

    let mut added: Vec<EntitySummary> = Vec::new();
    let mut removed: Vec<EntitySummary> = Vec::new();
    let mut changed: HashMap<String, EntityDiff> = HashMap::new();
    let mut rels_added = 0usize;
    let mut rels_removed = 0usize;

    for event in &events {
        match event.event_type {
            EventType::EntityCreated => {
                added.push(EntitySummary {
                    id: event.entity_id.clone(),
                    name: event.entity_name.clone(),
                    entity_type: event.entity_type.clone(),
                });
            }
            EventType::EntityDeleted => {
                removed.push(EntitySummary {
                    id: event.entity_id.clone(),
                    name: event.entity_name.clone(),
                    entity_type: event.entity_type.clone(),
                });
            }
            EventType::StateChanged | EventType::HealthChanged | EventType::EntityUpdated |
            EventType::PropertyChanged | EventType::PositionUpdated => {
                let diff_entry = changed.entry(event.entity_id.clone()).or_insert_with(|| EntityDiff {
                    entity_id: event.entity_id.clone(),
                    entity_name: event.entity_name.clone(),
                    entity_type: event.entity_type.clone(),
                    changes: Vec::new(),
                });
                let field = match event.event_type {
                    EventType::StateChanged => "state",
                    EventType::HealthChanged => "health",
                    EventType::PositionUpdated => "position",
                    _ => "properties",
                };
                diff_entry.changes.push(FieldChange {
                    field: field.to_string(),
                    old_value: event.old_value.clone().unwrap_or(serde_json::Value::Null),
                    new_value: event.new_value.clone().unwrap_or(serde_json::Value::Null),
                });
            }
            EventType::RelationCreated => { rels_added += 1; }
            EventType::RelationRemoved => { rels_removed += 1; }
            _ => {}
        }
    }

    TwinDiff {
        t1: t1.to_string(),
        t2: t2.to_string(),
        entities_added: added,
        entities_removed: removed,
        entities_changed: changed.into_values().collect(),
        relations_added: rels_added,
        relations_removed: rels_removed,
        total_events: events.len(),
    }
}

/// Generate replay frames between two timestamps for animated playback
pub fn replay_frames(ledger: &EventLedger, current_graph: &SemanticGraph, t1: &str, t2: &str, max_frames: usize) -> Vec<ReplayFrame> {
    let events: Vec<&LedgerEvent> = ledger.events.iter()
        .filter(|e| e.timestamp.as_str() > t1 && e.timestamp.as_str() <= t2)
        .collect();

    if events.is_empty() {
        return Vec::new();
    }

    // Group events into frames
    let frame_count = max_frames.min(events.len());
    let events_per_frame = (events.len() as f64 / frame_count as f64).ceil() as usize;

    let mut frames = Vec::new();

    // Start from state_at t1
    let mut graph = state_at(ledger, current_graph, t1);

    for (frame_idx, chunk) in events.chunks(events_per_frame.max(1)).enumerate() {
        let mut replay_events = Vec::new();

        for event in chunk {
            let desc = match event.event_type {
                EventType::EntityCreated => format!("{} created", event.entity_name),
                EventType::EntityDeleted => format!("{} removed", event.entity_name),
                EventType::StateChanged => {
                    let new_state = event.new_value.as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    format!("{} → {}", event.entity_name, new_state)
                }
                EventType::HealthChanged => {
                    let new_health = event.new_value.as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    format!("{} health → {}", event.entity_name, new_health)
                }
                EventType::PositionUpdated => format!("{} moved", event.entity_name),
                EventType::AnomalyDetected => format!("[WARN] Anomaly: {}", event.entity_name),
                EventType::AnomalyResolved => format!("[OK] Resolved: {}", event.entity_name),
                _ => format!("{}: {:?}", event.entity_name, event.event_type),
            };

            replay_events.push(ReplayEvent {
                entity_id: event.entity_id.clone(),
                entity_name: event.entity_name.clone(),
                event_type: format!("{:?}", event.event_type),
                description: desc,
            });

            // Apply event to running graph state
            apply_event_to_graph(&mut graph, event);
        }

        let last_ts = chunk.last().map(|e| e.timestamp.clone()).unwrap_or_default();

        let entity_states: Vec<EntitySnapshot> = graph.entities.iter().map(|e| {
            EntitySnapshot {
                id: e.id.clone(),
                name: e.name.clone(),
                entity_type: format!("{:?}", e.entity_type).to_lowercase(),
                state: format!("{:?}", e.state).to_lowercase(),
                health: format!("{:?}", e.health).to_lowercase(),
                lat: e.lat,
                lng: e.lng,
            }
        }).collect();

        frames.push(ReplayFrame {
            frame_index: frame_idx,
            timestamp: last_ts,
            events: replay_events,
            entity_states,
        });
    }

    frames
}

/// Query entities at a specific timestamp with optional type filter
pub fn query_at(ledger: &EventLedger, current_graph: &SemanticGraph, timestamp: &str, entity_type: Option<&str>) -> Vec<TwinEntity> {
    let graph = state_at(ledger, current_graph, timestamp);
    if let Some(et) = entity_type {
        graph.entities.into_iter()
            .filter(|e| format!("{:?}", e.entity_type).to_lowercase() == et.to_lowercase())
            .collect()
    } else {
        graph.entities
    }
}

// ─────────────────────────────────────────────
//  Internal Helpers
// ─────────────────────────────────────────────

fn apply_events_to_graph(graph: &mut SemanticGraph, events: &[&LedgerEvent]) {
    for event in events {
        apply_event_to_graph(graph, event);
    }
}

fn apply_event_to_graph(graph: &mut SemanticGraph, event: &LedgerEvent) {
    match event.event_type {
        EventType::EntityCreated => {
            if let Some(new_val) = &event.new_value {
                if let Ok(entity) = serde_json::from_value::<TwinEntity>(new_val.clone()) {
                    if !graph.entities.iter().any(|e| e.id == entity.id) {
                        graph.entities.push(entity);
                    }
                }
            }
        }
        EventType::EntityDeleted => {
            graph.entities.retain(|e| e.id != event.entity_id);
        }
        EventType::StateChanged => {
            if let Some(entity) = graph.entities.iter_mut().find(|e| e.id == event.entity_id) {
                if let Some(new_val) = &event.new_value {
                    if let Some(s) = new_val.as_str() {
                        if let Ok(state) = serde_json::from_value(serde_json::json!(s)) {
                            entity.state = state;
                        }
                    }
                }
            }
        }
        EventType::HealthChanged => {
            if let Some(entity) = graph.entities.iter_mut().find(|e| e.id == event.entity_id) {
                if let Some(new_val) = &event.new_value {
                    if let Some(s) = new_val.as_str() {
                        if let Ok(health) = serde_json::from_value(serde_json::json!(s)) {
                            entity.health = health;
                        }
                    }
                }
            }
        }
        EventType::PositionUpdated => {
            if let Some(entity) = graph.entities.iter_mut().find(|e| e.id == event.entity_id) {
                if let Some(new_val) = &event.new_value {
                    if let Some(obj) = new_val.as_object() {
                        if let Some(lat) = obj.get("lat").and_then(|v| v.as_f64()) {
                            entity.lat = Some(lat);
                        }
                        if let Some(lng) = obj.get("lng").and_then(|v| v.as_f64()) {
                            entity.lng = Some(lng);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    graph.metadata.entity_count = graph.entities.len();
}
