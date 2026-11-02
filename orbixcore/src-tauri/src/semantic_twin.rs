use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use uuid::Uuid;

// ─────────────────────────────────────────────
//  Entity & Relation Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TwinEntityType {
    // ── CRM ──
    Company,
    Contact,
    Lead,
    Deal,
    Opportunity,
    // ── ERP / Commerce ──
    Product,
    Order,
    Invoice,
    Payment,
    Expense,
    // ── Finance ──
    Account,
    Transaction,
    // ── PPM ──
    Project,
    Task,
    Milestone,
    // ── Support ──
    Ticket,
    KnowledgeArticle,
    // ── People / Org ──
    Employee,
    Department,
    // ── Generic ──
    Record,  // Catch-all for anything else
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntityState {
    // Generic record lifecycle states - work for any business entity
    Draft,      // Not yet committed
    Active,     // Live, in use
    Pending,    // Awaiting action (approval, payment, etc.)
    InProgress, // Being worked on (deals, projects, tickets)
    OnHold,     // Paused, not cancelled
    Completed,  // Finished successfully (closed deal, paid invoice, etc.)
    Cancelled,  // Stopped, won't complete
    Archived,   // Kept for history but no longer active
    Warning,    // Needs attention
    Critical,   // Urgent - blocking / at risk
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntityHealth {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    // ── Ownership / hierarchy ──
    OwnedBy,       // Record owned by a user/team (deal owned_by salesperson)
    ManagedBy,     // Record managed by (project managed_by PM)
    ReportsTo,     // Employee → Manager
    ParentOf,      // Parent account → subsidiary, project → sub-project
    BelongsTo,     // Contact belongs_to company
    // ── CRM ──
    ContactAt,     // Contact contact_at company
    DealFor,       // Deal deal_for company
    LeadFor,       // Lead lead_for company
    AssignedTo,    // Task / ticket / deal assigned_to employee
    // ── ERP / Finance ──
    InvoiceFor,    // Invoice invoice_for order
    OrderFor,      // Order order_for company/customer
    PaymentFor,    // Payment payment_for invoice
    ExpenseFor,    // Expense expense_for project/department
    LinkedTo,      // Account linked_to transaction
    // ── PPM ──
    DependsOn,     // Task depends_on milestone
    Blocks,        // Ticket blocks deployment
    PartOf,        // Task part_of project
    // ── Generic ──
    RelatedTo,     // Loose connection
    References,    // Record references another (knowledge article references ticket)
}

// ─────────────────────────────────────────────
//  Core Structures
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TwinEntity {
    pub id: String,
    pub entity_type: TwinEntityType,
    pub name: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub state: EntityState,
    pub health: EntityHealth,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TwinRelation {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: RelationType,
    pub label: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub strength: f64, // 0.0 – 1.0, visual weight
    pub created_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphMetadata {
    pub name: String,
    pub description: String,
    pub entity_count: usize,
    pub relation_count: usize,
    pub last_updated: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SemanticGraph {
    pub entities: Vec<TwinEntity>,
    pub relations: Vec<TwinRelation>,
    pub metadata: GraphMetadata,
}

// ─────────────────────────────────────────────
//  Twin Store
// ─────────────────────────────────────────────

pub struct TwinStore {
    pub graph: SemanticGraph,
}

impl Default for TwinStore {
    fn default() -> Self {
        Self {
            graph: SemanticGraph {
                entities: Vec::new(),
                relations: Vec::new(),
                metadata: GraphMetadata {
                    name: "Records Twin".into(),
                    description: "Semantic digital twin of business records - every change, every relationship, tracked.".into(),
                    entity_count: 0,
                    relation_count: 0,
                    last_updated: Utc::now().to_rfc3339(),
                },
            },
        }
    }
}

impl TwinStore {
    /// Upsert a business record into the twin by (entity_type, external_id).
    /// If a record with the same type + properties.external_id exists, update it.
    /// Otherwise create a new entity.
    /// Returns the (entity, was_created) tuple.
    pub fn upsert_record(
        &mut self,
        entity_type: TwinEntityType,
        external_id: &str,
        name: &str,
        state: EntityState,
        properties: HashMap<String, serde_json::Value>,
    ) -> (TwinEntity, bool) {
        // Look for existing record
        let existing_idx = self.graph.entities.iter().position(|e| {
            e.entity_type == entity_type &&
            e.properties.get("external_id").and_then(|v| v.as_str()) == Some(external_id)
        });

        if let Some(idx) = existing_idx {
            let entity = &mut self.graph.entities[idx];
            entity.name = name.to_string();
            entity.state = state;
            for (k, v) in properties {
                entity.properties.insert(k, v);
            }
            entity.properties.insert("external_id".to_string(), serde_json::json!(external_id));
            entity.updated_at = Utc::now().to_rfc3339();
            let result = entity.clone();
            self.update_metadata();
            (result, false)
        } else {
            let mut props = properties;
            props.insert("external_id".to_string(), serde_json::json!(external_id));
            let entity = TwinEntity {
                id: Uuid::new_v4().to_string(),
                entity_type,
                name: name.to_string(),
                properties: props,
                state,
                health: EntityHealth::Healthy,
                lat: None,
                lng: None,
                tags: Vec::new(),
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
            };
            self.graph.entities.push(entity.clone());
            self.update_metadata();
            (entity, true)
        }
    }

    pub fn add_entity(&mut self, mut entity: TwinEntity) -> TwinEntity {
        if entity.id.is_empty() {
            entity.id = Uuid::new_v4().to_string();
        }
        if entity.created_at.is_empty() {
            entity.created_at = Utc::now().to_rfc3339();
        }
        entity.updated_at = Utc::now().to_rfc3339();
        self.graph.entities.push(entity.clone());
        self.update_metadata();
        entity
    }

    pub fn update_entity(&mut self, id: &str, updates: HashMap<String, serde_json::Value>) -> Option<TwinEntity> {
        let idx = self.graph.entities.iter().position(|e| e.id == id)?;
        {
            let entity = &mut self.graph.entities[idx];
            if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
                entity.name = name.to_string();
            }
            if let Some(state) = updates.get("state").and_then(|v| v.as_str()) {
                entity.state = serde_json::from_value(serde_json::json!(state)).unwrap_or(entity.state.clone());
            }
            if let Some(health) = updates.get("health").and_then(|v| v.as_str()) {
                entity.health = serde_json::from_value(serde_json::json!(health)).unwrap_or(entity.health.clone());
            }
            if let Some(lat) = updates.get("lat").and_then(|v| v.as_f64()) {
                entity.lat = Some(lat);
            }
            if let Some(lng) = updates.get("lng").and_then(|v| v.as_f64()) {
                entity.lng = Some(lng);
            }
            for (k, v) in &updates {
                if !["name", "state", "health", "lat", "lng"].contains(&k.as_str()) {
                    entity.properties.insert(k.clone(), v.clone());
                }
            }
            entity.updated_at = Utc::now().to_rfc3339();
        }
        self.update_metadata();
        Some(self.graph.entities[idx].clone())
    }

    pub fn remove_entity(&mut self, id: &str) -> bool {
        let before = self.graph.entities.len();
        self.graph.entities.retain(|e| e.id != id);
        // Remove relations involving this entity
        self.graph.relations.retain(|r| r.source_id != id && r.target_id != id);
        self.update_metadata();
        self.graph.entities.len() < before
    }

    pub fn get_entity(&self, id: &str) -> Option<&TwinEntity> {
        self.graph.entities.iter().find(|e| e.id == id)
    }

    pub fn get_entities_by_type(&self, entity_type: &TwinEntityType) -> Vec<&TwinEntity> {
        self.graph.entities.iter().filter(|e| &e.entity_type == entity_type).collect()
    }

    pub fn add_relation(&mut self, mut relation: TwinRelation) -> TwinRelation {
        if relation.id.is_empty() {
            relation.id = Uuid::new_v4().to_string();
        }
        if relation.created_at.is_empty() {
            relation.created_at = Utc::now().to_rfc3339();
        }
        if relation.strength <= 0.0 {
            relation.strength = 0.5;
        }
        self.graph.relations.push(relation.clone());
        self.update_metadata();
        relation
    }

    pub fn remove_relation(&mut self, id: &str) -> bool {
        let before = self.graph.relations.len();
        self.graph.relations.retain(|r| r.id != id);
        self.update_metadata();
        self.graph.relations.len() < before
    }

    pub fn get_relations_for(&self, entity_id: &str) -> Vec<&TwinRelation> {
        self.graph.relations.iter()
            .filter(|r| r.source_id == entity_id || r.target_id == entity_id)
            .collect()
    }

    pub fn get_subgraph(&self, center_id: &str, depth: usize) -> SemanticGraph {
        let mut entity_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut frontier: Vec<String> = vec![center_id.to_string()];

        for _ in 0..depth {
            let mut next_frontier = Vec::new();
            for eid in &frontier {
                if entity_ids.insert(eid.clone()) {
                    for rel in &self.graph.relations {
                        if rel.source_id == *eid && !entity_ids.contains(&rel.target_id) {
                            next_frontier.push(rel.target_id.clone());
                        }
                        if rel.target_id == *eid && !entity_ids.contains(&rel.source_id) {
                            next_frontier.push(rel.source_id.clone());
                        }
                    }
                }
            }
            frontier = next_frontier;
        }
        // Add remaining frontier
        for eid in frontier {
            entity_ids.insert(eid);
        }

        let entities: Vec<TwinEntity> = self.graph.entities.iter()
            .filter(|e| entity_ids.contains(&e.id))
            .cloned()
            .collect();
        let relations: Vec<TwinRelation> = self.graph.relations.iter()
            .filter(|r| entity_ids.contains(&r.source_id) && entity_ids.contains(&r.target_id))
            .cloned()
            .collect();

        SemanticGraph {
            metadata: GraphMetadata {
                name: format!("Subgraph around {}", center_id),
                description: format!("Depth-{} subgraph", depth),
                entity_count: entities.len(),
                relation_count: relations.len(),
                last_updated: Utc::now().to_rfc3339(),
            },
            entities,
            relations,
        }
    }

    fn update_metadata(&mut self) {
        self.graph.metadata.entity_count = self.graph.entities.len();
        self.graph.metadata.relation_count = self.graph.relations.len();
        self.graph.metadata.last_updated = Utc::now().to_rfc3339();
    }
}

// ─────────────────────────────────────────────
//  Stats
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TwinStats {
    pub total_entities: usize,
    pub total_relations: usize,
    pub entities_by_type: HashMap<String, usize>,
    pub entities_by_state: HashMap<String, usize>,
    pub entities_by_health: HashMap<String, usize>,
    pub healthy_pct: f64,
    pub active_pct: f64,
}

impl TwinStore {
    pub fn stats(&self) -> TwinStats {
        let total = self.graph.entities.len();
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut by_state: HashMap<String, usize> = HashMap::new();
        let mut by_health: HashMap<String, usize> = HashMap::new();

        for e in &self.graph.entities {
            *by_type.entry(format!("{:?}", e.entity_type).to_lowercase()).or_insert(0) += 1;
            *by_state.entry(format!("{:?}", e.state).to_lowercase()).or_insert(0) += 1;
            *by_health.entry(format!("{:?}", e.health).to_lowercase()).or_insert(0) += 1;
        }

        let healthy_count = self.graph.entities.iter().filter(|e| e.health == EntityHealth::Healthy).count();
        let active_count = self.graph.entities.iter().filter(|e| matches!(e.state, EntityState::Active | EntityState::InProgress | EntityState::Pending)).count();

        TwinStats {
            total_entities: total,
            total_relations: self.graph.relations.len(),
            entities_by_type: by_type,
            entities_by_state: by_state,
            entities_by_health: by_health,
            healthy_pct: if total > 0 { (healthy_count as f64 / total as f64) * 100.0 } else { 0.0 },
            active_pct: if total > 0 { (active_count as f64 / total as f64) * 100.0 } else { 0.0 },
        }
    }
}

// ─────────────────────────────────────────────
//  Pre-Flight Simulation
// ─────────────────────────────────────────────

/// A proposed action that an agent wants to take.
/// Evaluated in a sandbox before committing to the real graph.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PreflightAction {
    pub id: String,
    pub agent_name: String,
    pub description: String,
    pub target_entity_id: Option<String>,
    pub target_entity_name: Option<String>,
    pub action_type: PreflightActionType,
    pub payload: HashMap<String, serde_json::Value>,
    pub created_at: String,
    pub risk_level: RiskLevel,
    pub status: PreflightStatus,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PreflightActionType {
    CreateEntity,
    UpdateEntity,
    DeleteEntity,
    CreateRelation,
    RemoveRelation,
    ChangeState,
    ChangeHealth,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel { Low, Medium, High, Critical }

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PreflightStatus { Pending, Simulated, Approved, Rejected, Committed }

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SimulationResult {
    pub sandbox_graph: SemanticGraph,
    pub projected_changes: Vec<ProjectedChange>,
    pub risk_summary: RiskSummary,
    pub actions_applied: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProjectedChange {
    pub entity_id: String,
    pub entity_name: String,
    pub field: String,
    pub current_value: Option<String>,
    pub projected_value: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RiskSummary {
    pub total_actions: usize,
    pub entities_affected: usize,
    pub relations_affected: usize,
    pub highest_risk: String,
    pub creates: usize,
    pub updates: usize,
    pub deletes: usize,
}

pub struct PreflightStore {
    pub actions: Vec<PreflightAction>,
}

impl Default for PreflightStore {
    fn default() -> Self { Self { actions: Vec::new() } }
}

impl PreflightStore {
    /// Submit a proposed action for pre-flight evaluation.
    pub fn submit(&mut self, mut action: PreflightAction) -> PreflightAction {
        if action.id.is_empty() { action.id = Uuid::new_v4().to_string(); }
        if action.created_at.is_empty() { action.created_at = Utc::now().to_rfc3339(); }
        action.status = PreflightStatus::Pending;
        self.actions.push(action.clone());
        action
    }

    /// Get all pending/simulated actions.
    pub fn pending(&self) -> Vec<&PreflightAction> {
        self.actions.iter().filter(|a| a.status == PreflightStatus::Pending || a.status == PreflightStatus::Simulated).collect()
    }

    /// Simulate all pending actions against a COPY of the current graph.
    /// Returns projected state without modifying the real graph.
    pub fn simulate(&mut self, real_graph: &SemanticGraph) -> SimulationResult {
        let mut sandbox = TwinStore { graph: real_graph.clone() };
        let mut changes: Vec<ProjectedChange> = Vec::new();
        let (mut creates, mut updates, mut deletes, mut relations_affected) = (0, 0, 0, 0);
        let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut highest = RiskLevel::Low;

        for action in self.actions.iter_mut().filter(|a| a.status == PreflightStatus::Pending) {
            if action.risk_level > highest { highest = action.risk_level.clone(); }

            match action.action_type {
                PreflightActionType::CreateEntity => {
                    let name = action.payload.get("name").and_then(|v| v.as_str()).unwrap_or("New Entity").to_string();
                    let et_str = action.payload.get("entity_type").and_then(|v| v.as_str()).unwrap_or("record");
                    let et: TwinEntityType = serde_json::from_value(serde_json::json!(et_str)).unwrap_or(TwinEntityType::Record);
                    let entity = TwinEntity {
                        id: Uuid::new_v4().to_string(), entity_type: et, name: name.clone(),
                        properties: action.payload.iter().filter(|(k, _)| !matches!(k.as_str(), "name" | "entity_type")).map(|(k, v)| (k.clone(), v.clone())).collect(),
                        state: EntityState::Active, health: EntityHealth::Healthy,
                        lat: action.payload.get("lat").and_then(|v| v.as_f64()),
                        lng: action.payload.get("lng").and_then(|v| v.as_f64()),
                        tags: Vec::new(), created_at: Utc::now().to_rfc3339(), updated_at: Utc::now().to_rfc3339(),
                    };
                    changes.push(ProjectedChange { entity_id: entity.id.clone(), entity_name: name, field: "entity".into(), current_value: None, projected_value: "created".into() });
                    affected.insert(entity.id.clone());
                    sandbox.add_entity(entity);
                    creates += 1;
                }
                PreflightActionType::UpdateEntity | PreflightActionType::ChangeState | PreflightActionType::ChangeHealth => {
                    let tid = action.target_entity_id.as_deref().unwrap_or("");
                    let tname = action.target_entity_name.as_deref().unwrap_or("").to_lowercase();
                    if let Some(existing) = sandbox.graph.entities.iter().find(|e| e.id == tid || e.name.to_lowercase() == tname) {
                        let (eid, ename) = (existing.id.clone(), existing.name.clone());
                        for (key, val) in &action.payload {
                            let current = match key.as_str() {
                                "state" => Some(format!("{:?}", existing.state)),
                                "health" => Some(format!("{:?}", existing.health)),
                                "name" => Some(existing.name.clone()),
                                _ => existing.properties.get(key).map(|v| v.to_string()),
                            };
                            changes.push(ProjectedChange { entity_id: eid.clone(), entity_name: ename.clone(), field: key.clone(), current_value: current, projected_value: val.to_string() });
                        }
                        affected.insert(eid.clone());
                        sandbox.update_entity(&eid, action.payload.clone());
                        updates += 1;
                    }
                }
                PreflightActionType::DeleteEntity => {
                    let tid = action.target_entity_id.as_deref().unwrap_or("");
                    let tname = action.target_entity_name.as_deref().unwrap_or("").to_lowercase();
                    if let Some(existing) = sandbox.graph.entities.iter().find(|e| e.id == tid || e.name.to_lowercase() == tname) {
                        changes.push(ProjectedChange { entity_id: existing.id.clone(), entity_name: existing.name.clone(), field: "entity".into(), current_value: Some("exists".into()), projected_value: "deleted".into() });
                        affected.insert(existing.id.clone());
                        let eid = existing.id.clone();
                        sandbox.remove_entity(&eid);
                        deletes += 1;
                    }
                }
                PreflightActionType::CreateRelation | PreflightActionType::RemoveRelation => { relations_affected += 1; }
            }
            action.status = PreflightStatus::Simulated;
        }

        SimulationResult {
            sandbox_graph: sandbox.graph,
            projected_changes: changes,
            risk_summary: RiskSummary {
                total_actions: creates + updates + deletes + relations_affected,
                entities_affected: affected.len(), relations_affected,
                highest_risk: format!("{:?}", highest).to_lowercase(),
                creates, updates, deletes,
            },
            actions_applied: creates + updates + deletes,
        }
    }

    /// Approve all simulated actions.
    pub fn approve_all(&mut self) {
        for a in &mut self.actions { if a.status == PreflightStatus::Simulated { a.status = PreflightStatus::Approved; } }
    }

    /// Reject all pending/simulated actions.
    pub fn reject_all(&mut self) {
        for a in &mut self.actions { if matches!(a.status, PreflightStatus::Pending | PreflightStatus::Simulated) { a.status = PreflightStatus::Rejected; } }
    }

    /// Commit approved actions to the real graph.
    pub fn commit(&mut self, real_store: &mut TwinStore) -> Vec<PreflightAction> {
        let mut committed: Vec<PreflightAction> = Vec::new();
        for action in &mut self.actions {
            if action.status != PreflightStatus::Approved { continue; }
            match action.action_type {
                PreflightActionType::CreateEntity => {
                    let name = action.payload.get("name").and_then(|v| v.as_str()).unwrap_or("Entity").to_string();
                    let et_str = action.payload.get("entity_type").and_then(|v| v.as_str()).unwrap_or("record");
                    let et: TwinEntityType = serde_json::from_value(serde_json::json!(et_str)).unwrap_or(TwinEntityType::Record);
                    real_store.add_entity(TwinEntity {
                        id: Uuid::new_v4().to_string(), entity_type: et, name,
                        properties: action.payload.iter().filter(|(k, _)| !matches!(k.as_str(), "name" | "entity_type")).map(|(k, v)| (k.clone(), v.clone())).collect(),
                        state: EntityState::Active, health: EntityHealth::Healthy,
                        lat: action.payload.get("lat").and_then(|v| v.as_f64()),
                        lng: action.payload.get("lng").and_then(|v| v.as_f64()),
                        tags: Vec::new(), created_at: Utc::now().to_rfc3339(), updated_at: Utc::now().to_rfc3339(),
                    });
                }
                PreflightActionType::UpdateEntity | PreflightActionType::ChangeState | PreflightActionType::ChangeHealth => {
                    let tid = action.target_entity_id.as_deref().unwrap_or("");
                    let tname = action.target_entity_name.as_deref().unwrap_or("").to_lowercase();
                    let found = real_store.graph.entities.iter().find(|e| e.id == tid || e.name.to_lowercase() == tname).map(|e| e.id.clone());
                    if let Some(eid) = found { real_store.update_entity(&eid, action.payload.clone()); }
                }
                PreflightActionType::DeleteEntity => {
                    let tid = action.target_entity_id.as_deref().unwrap_or("");
                    let tname = action.target_entity_name.as_deref().unwrap_or("").to_lowercase();
                    let found = real_store.graph.entities.iter().find(|e| e.id == tid || e.name.to_lowercase() == tname).map(|e| e.id.clone());
                    if let Some(eid) = found { real_store.remove_entity(&eid); }
                }
                _ => {}
            }
            action.status = PreflightStatus::Committed;
            committed.push(action.clone());
        }
        self.actions.retain(|a| !matches!(a.status, PreflightStatus::Committed | PreflightStatus::Rejected));
        committed
    }
}
