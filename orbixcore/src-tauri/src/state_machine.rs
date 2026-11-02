use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use chrono::Utc;

// ─────────────────────────────────────────────
//  State Transition
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub timestamp: String,
    pub reason: Option<String>,
    pub actor: Option<String>,
}

// ─────────────────────────────────────────────
//  State Graph Definition
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateGraph {
    pub name: String,
    pub initial_state: String,
    pub terminal_states: HashSet<String>,
    pub transitions: HashMap<String, HashSet<String>>,  // from_state -> set of valid to_states
}

impl StateGraph {
    pub fn new(name: impl Into<String>, initial: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            initial_state: initial.into(),
            terminal_states: HashSet::new(),
            transitions: HashMap::new(),
        }
    }

    /// Add an allowed transition from one state to another.
    pub fn allow(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        let from = from.into();
        let to = to.into();
        self.transitions.entry(from).or_default().insert(to);
        self
    }

    /// Mark a state as terminal (no further transitions allowed from it).
    pub fn terminal(mut self, state: impl Into<String>) -> Self {
        self.terminal_states.insert(state.into());
        self
    }

    /// Check if a transition is valid.
    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        if self.terminal_states.contains(from) {
            return false;
        }
        self.transitions.get(from).map_or(false, |targets| targets.contains(to))
    }

    /// Get all valid next states from a given state.
    pub fn next_states(&self, from: &str) -> Vec<String> {
        if self.terminal_states.contains(from) {
            return Vec::new();
        }
        self.transitions.get(from)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if a state is terminal.
    pub fn is_terminal(&self, state: &str) -> bool {
        self.terminal_states.contains(state)
    }

    /// All known states (from transitions keys + values + terminal + initial).
    pub fn all_states(&self) -> HashSet<String> {
        let mut states = HashSet::new();
        states.insert(self.initial_state.clone());
        states.extend(self.terminal_states.iter().cloned());
        for (from, tos) in &self.transitions {
            states.insert(from.clone());
            states.extend(tos.iter().cloned());
        }
        states
    }
}

// ─────────────────────────────────────────────
//  Managed Entity State
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityState {
    pub entity_id: String,
    pub entity_type: String,
    pub current_state: String,
    pub history: Vec<StateTransition>,
    pub created_at: String,
    pub updated_at: String,
}

// ─────────────────────────────────────────────
//  State Machine Registry
// ─────────────────────────────────────────────

pub struct StateMachineRegistry {
    graphs: HashMap<String, StateGraph>,       // entity_type -> graph
    entities: HashMap<String, EntityState>,     // entity_id -> state
}

impl StateMachineRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            graphs: HashMap::new(),
            entities: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    /// Register a state graph for an entity type.
    pub fn register(&mut self, entity_type: impl Into<String>, graph: StateGraph) {
        self.graphs.insert(entity_type.into(), graph);
    }

    /// Initialize an entity at the initial state of its type's graph.
    pub fn init_entity(&mut self, entity_id: impl Into<String>, entity_type: impl Into<String>) -> Result<EntityState, String> {
        let entity_type = entity_type.into();
        let graph = self.graphs.get(&entity_type)
            .ok_or_else(|| format!("No state graph registered for type: {}", entity_type))?;

        let now = Utc::now().to_rfc3339();
        let state = EntityState {
            entity_id: entity_id.into(),
            entity_type,
            current_state: graph.initial_state.clone(),
            history: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.entities.insert(state.entity_id.clone(), state.clone());
        Ok(state)
    }

    /// Transition an entity to a new state. Returns error if transition is invalid.
    pub fn transition(
        &mut self,
        entity_id: &str,
        to_state: impl Into<String>,
        reason: Option<String>,
        actor: Option<String>,
    ) -> Result<EntityState, String> {
        let to_state = to_state.into();
        let entity = self.entities.get_mut(entity_id)
            .ok_or_else(|| format!("Entity not found: {}", entity_id))?;
        let graph = self.graphs.get(&entity.entity_type)
            .ok_or_else(|| format!("No graph for type: {}", entity.entity_type))?;

        if !graph.can_transition(&entity.current_state, &to_state) {
            return Err(format!(
                "Invalid transition: {} -> {} (entity: {}, type: {})",
                entity.current_state, to_state, entity_id, entity.entity_type
            ));
        }

        let transition = StateTransition {
            from: entity.current_state.clone(),
            to: to_state.clone(),
            timestamp: Utc::now().to_rfc3339(),
            reason,
            actor,
        };

        entity.current_state = to_state;
        entity.history.push(transition);
        entity.updated_at = Utc::now().to_rfc3339();

        Ok(entity.clone())
    }

    /// Get the current state of an entity.
    pub fn get_state(&self, entity_id: &str) -> Option<&EntityState> {
        self.entities.get(entity_id)
    }

    /// Get valid next states for an entity.
    pub fn next_states(&self, entity_id: &str) -> Result<Vec<String>, String> {
        let entity = self.entities.get(entity_id)
            .ok_or_else(|| format!("Entity not found: {}", entity_id))?;
        let graph = self.graphs.get(&entity.entity_type)
            .ok_or_else(|| format!("No graph for type: {}", entity.entity_type))?;
        Ok(graph.next_states(&entity.current_state))
    }

    /// Get a registered state graph.
    pub fn get_graph(&self, entity_type: &str) -> Option<&StateGraph> {
        self.graphs.get(entity_type)
    }

    /// List all registered entity types.
    pub fn entity_types(&self) -> Vec<String> {
        self.graphs.keys().cloned().collect()
    }

    /// Remove an entity from tracking.
    pub fn remove_entity(&mut self, entity_id: &str) -> bool {
        self.entities.remove(entity_id).is_some()
    }

    /// Register default state graphs from the architecture doc.
    fn register_defaults(&mut self) {
        // Job state graph
        self.register("job", StateGraph::new("Job", "pending")
            .allow("pending", "running")
            .allow("running", "completed")
            .allow("running", "failed")
            .allow("running", "cancelled")
            .allow("failed", "retrying")
            .allow("retrying", "running")
            .allow("retrying", "dead")
            .terminal("completed")
            .terminal("dead")
            .terminal("cancelled")
        );

        // Plugin state graph
        self.register("plugin", StateGraph::new("Plugin", "registered")
            .allow("registered", "initialized")
            .allow("initialized", "active")
            .allow("active", "deactivated")
            .allow("deactivated", "unloaded")
            .allow("active", "error")
            .allow("error", "active")
            .allow("error", "deactivated")
            .terminal("unloaded")
        );

        // Session state graph
        self.register("session", StateGraph::new("Session", "created")
            .allow("created", "active")
            .allow("active", "idle")
            .allow("idle", "active")
            .allow("idle", "expired")
            .allow("active", "revoked")
            .allow("idle", "revoked")
            .allow("expired", "revoked")
            .terminal("expired")
            .terminal("revoked")
        );

        // Workflow state graph
        self.register("workflow", StateGraph::new("Workflow", "idle")
            .allow("idle", "running")
            .allow("running", "paused")
            .allow("paused", "running")
            .allow("running", "completed")
            .allow("running", "failed")
            .allow("running", "stopped")
            .allow("paused", "stopped")
            .terminal("completed")
            .terminal("failed")
            .terminal("stopped")
        );
    }
}

impl Default for StateMachineRegistry {
    fn default() -> Self {
        Self::new()
    }
}
