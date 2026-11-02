use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Workflow Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<WorkflowStep>,
    pub trigger: WorkflowTrigger,
    /// Where the final result goes when a run finishes.
    #[serde(default)]
    pub deliver: Deliver,
    pub created_at: String,
    pub updated_at: String,
}

/// Where a workflow's result lands. Chat is the default: Bob posts it as
/// himself, which also covers scheduled and triggered runs that used to
/// finish in silence.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Deliver {
    Chat,
    /// A branded .docx in Documents, plus a chat line with the path.
    Document,
    /// A My UI panel holding the result.
    Panel,
    /// A system notification with the first line, plus chat.
    Notify,
    /// Send the result into a paired app (peer_send), plus chat.
    App(String),
}

impl Default for Deliver {
    fn default() -> Self {
        Deliver::Chat
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkflowStep {
    pub id: String,
    pub order: usize,
    pub step_type: StepType,
    pub label: String,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    Prompt,
    Action,
    Condition,
    Wait,
    Loop,
    Relay,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTrigger {
    Manual,
    Command(String),
    Schedule(String),
}

impl Default for WorkflowTrigger {
    fn default() -> Self {
        WorkflowTrigger::Manual
    }
}

// ─────────────────────────────────────────────
//  Execution Status
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Idle,
    Running,
    Paused,
    Completed,
    Failed,
    Stopped,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkflowRunState {
    pub workflow_id: String,
    pub status: WorkflowRunStatus,
    pub current_step: usize,
    pub total_steps: usize,
    pub output: Vec<String>,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────
//  Workflow Store
// ─────────────────────────────────────────────

pub struct WorkflowStore {
    pub workflows: Vec<Workflow>,
    pub run_state: Option<WorkflowRunState>,
}

impl Default for WorkflowStore {
    fn default() -> Self {
        Self {
            workflows: Vec::new(),
            run_state: None,
        }
    }
}

impl WorkflowStore {
    /// Load from disk, falling back to empty
    pub fn load() -> Self {
        let workflows = crate::storage::load_workflows().unwrap_or_default();
        Self {
            workflows,
            run_state: None,
        }
    }

    /// Persist current workflows to disk
    pub fn save(&self) {
        crate::storage::save_workflows(&self.workflows).ok();
    }

    // ── CRUD ──

    pub fn list(&self) -> Vec<Workflow> {
        self.workflows.clone()
    }

    pub fn get(&self, id: &str) -> Option<Workflow> {
        self.workflows.iter().find(|w| w.id == id).cloned()
    }

    pub fn create(&mut self, name: String, description: String) -> Workflow {
        let now = Utc::now().to_rfc3339();
        let workflow = Workflow {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            steps: Vec::new(),
            trigger: WorkflowTrigger::Manual,
            deliver: Default::default(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.workflows.push(workflow.clone());
        workflow
    }

    pub fn update(&mut self, id: &str, name: Option<String>, description: Option<String>) -> Option<Workflow> {
        if let Some(w) = self.workflows.iter_mut().find(|w| w.id == id) {
            if let Some(n) = name { w.name = n; }
            if let Some(d) = description { w.description = d; }
            w.updated_at = Utc::now().to_rfc3339();
            Some(w.clone())
        } else {
            None
        }
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let len = self.workflows.len();
        self.workflows.retain(|w| w.id != id);
        self.workflows.len() < len
    }

    // ── Execution ──

    // LEGACY single-slot run tracking (pre-W2). The desktop run path now
    // uses the durable, keyed `run::RunStore` (see run_one_workflow), which
    // is resumable and supports concurrent runs. These methods remain only
    // for the mobile_api warp handlers, whose sync context cannot yet drive
    // the async run engine; migrating them is a tracked follow-up.
    pub fn start_run(&mut self, workflow_id: &str) -> Result<WorkflowRunState, String> {
        let workflow = self.get(workflow_id).ok_or("Workflow not found")?;
        if workflow.steps.is_empty() {
            return Err("Workflow has no steps".into());
        }
        let state = WorkflowRunState {
            workflow_id: workflow_id.to_string(),
            status: WorkflowRunStatus::Running,
            current_step: 0,
            total_steps: workflow.steps.len(),
            output: Vec::new(),
            error: None,
        };
        self.run_state = Some(state.clone());
        Ok(state)
    }

    pub fn advance_step(&mut self, output: String) -> Option<WorkflowRunState> {
        if let Some(ref mut state) = self.run_state {
            state.output.push(output);
            state.current_step += 1;
            if state.current_step >= state.total_steps {
                state.status = WorkflowRunStatus::Completed;
            }
            Some(state.clone())
        } else {
            None
        }
    }

    pub fn fail_run(&mut self, error: String) -> Option<WorkflowRunState> {
        if let Some(ref mut state) = self.run_state {
            state.status = WorkflowRunStatus::Failed;
            state.error = Some(error);
            Some(state.clone())
        } else {
            None
        }
    }

    pub fn stop_run(&mut self) -> Option<WorkflowRunState> {
        if let Some(ref mut state) = self.run_state {
            state.status = WorkflowRunStatus::Stopped;
            Some(state.clone())
        } else {
            None
        }
    }

    pub fn get_run_state(&self) -> Option<WorkflowRunState> {
        self.run_state.clone()
    }
}
