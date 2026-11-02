//! The canonical Run abstraction (W2).
//!
//! Every execution in Orbix - a workflow, an agent chain, a single prompt,
//! a scheduled job - is a `Run`: one durable, resumable record with a state
//! machine and a persisted per-step log. This replaces the old single
//! in-memory `WorkflowStore.run_state: Option<WorkflowRunState>` slot, which
//! could track exactly one execution at a time and vanished on restart.
//!
//! Runs are keyed by id in `RunStore`, so concurrent executions never clobber
//! each other, and the whole store is written atomically to disk on every
//! transition, so a `kill -9` mid-run leaves a consistent record that the
//! startup resume pass can pick up.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use crate::workflow::WorkflowRunStatus as RunStatus;

/// The canonical execution vocabulary (W2.8), and how the types relate:
///
/// - **Prompt**: a single natural-language instruction. The atom that
///   starts work. Stored on the Run it initiated (`Run.prompt`).
/// - **Task**: one unit of work with an input, an assignee (an Agent or a
///   tool), and an output. A Run is composed of Tasks; each is persisted as
///   a `RunStep` and projected back out via `Run::tasks()`.
/// - **Workflow** / **AgentChain**: saved *definitions* (the ordered steps /
///   the agent topology). Executing one produces a Run.
/// - **Agent**: the executor a Task is dispatched to.
/// - **Run**: one durable, resumable execution of any of the above.
///
/// So: a Prompt starts a Run; a Run runs Tasks; a Task is dispatched to an
/// Agent. Workflow and AgentChain are the reusable definitions Runs execute.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Prompt {
    pub id: String,
    /// The natural-language instruction.
    pub text: String,
    /// Where it came from: "user", "schedule", "trigger", "chain", ...
    pub source: String,
    pub created_at: String,
}

impl Prompt {
    pub fn new(text: impl Into<String>, source: impl Into<String>, now: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            text: text.into(),
            source: source.into(),
            created_at: now,
        }
    }
}

/// A unit of work within a Run. The canonical projection of a `RunStep`:
/// same identity (index + label), plus the status/output/error the Run
/// engine tracks. `assignee` names the Agent or tool that ran it when known.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Task {
    pub index: usize,
    pub label: String,
    pub status: StepStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub attempts: u32,
}

/// What a Run is executing. Extensible: the five historical execution
/// concepts (workflow, chain, pipeline, job-profile, prompt) all become a
/// Run with a kind, sharing one status machine and one persisted log.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunKind {
    Workflow { workflow_id: String },
    Chain { chain_id: String },
    Prompt,
    Task { task_id: String },
}

/// Per-step status within a Run.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RunStep {
    pub index: usize,
    pub label: String,
    pub status: StepStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// How many times this step has been attempted (for retry/backoff).
    #[serde(default)]
    pub attempts: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Run {
    pub id: String,
    pub kind: RunKind,
    pub status: RunStatus,
    pub steps: Vec<RunStep>,
    /// Index of the next step to execute. Equals `steps.len()` when done.
    pub current_step: usize,
    #[serde(default)]
    pub inputs: serde_json::Value,
    /// The instruction that started this Run, when it came from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Run {
    pub fn total_steps(&self) -> usize {
        self.steps.len()
    }

    /// True when this Run was interrupted (still Running/Paused on load) and
    /// can be continued from `current_step`.
    pub fn is_resumable(&self) -> bool {
        matches!(self.status, RunStatus::Running | RunStatus::Paused)
    }

    /// Concatenated outputs of completed steps, in order. The run loop feeds
    /// this forward as context.
    pub fn outputs(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter_map(|s| s.output.clone())
            .collect()
    }

    /// The Run's steps as canonical Tasks (W2.8).
    pub fn tasks(&self) -> Vec<Task> {
        self.steps
            .iter()
            .map(|s| Task {
                index: s.index,
                label: s.label.clone(),
                status: s.status.clone(),
                output: s.output.clone(),
                error: s.error.clone(),
                attempts: s.attempts,
            })
            .collect()
    }

    /// Legacy wire/UI snapshot. The frontend still listens for
    /// `workflow-status-change` carrying a `WorkflowRunState`, so Runs of
    /// workflow kind project down to it without a frontend change.
    pub fn to_wire(&self) -> crate::workflow::WorkflowRunState {
        let workflow_id = match &self.kind {
            RunKind::Workflow { workflow_id } => workflow_id.clone(),
            RunKind::Chain { chain_id } => chain_id.clone(),
            RunKind::Task { task_id } => task_id.clone(),
            RunKind::Prompt => self.id.clone(),
        };
        crate::workflow::WorkflowRunState {
            workflow_id,
            status: self.status.clone(),
            current_step: self.current_step,
            total_steps: self.total_steps(),
            output: self.outputs(),
            error: self.error.clone(),
        }
    }
}

const RUNS_FILE: &str = "runs.json";
/// Keep the store bounded: retain at most this many terminal (done/failed/
/// stopped) runs, newest first. Active runs are never pruned.
const MAX_TERMINAL_RUNS: usize = 200;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct RunStore {
    pub runs: HashMap<String, Run>,
}

impl RunStore {
    pub fn load() -> Self {
        crate::storage::load_json::<RunStore>(RUNS_FILE).unwrap_or_default()
    }

    pub fn save(&self) {
        crate::storage::save_json(RUNS_FILE, self).ok();
    }

    pub fn get(&self, id: &str) -> Option<Run> {
        self.runs.get(id).cloned()
    }

    /// Most-recent-first list, optionally filtered by status.
    pub fn list(&self, status: Option<RunStatus>, limit: usize) -> Vec<Run> {
        let mut v: Vec<Run> = self
            .runs
            .values()
            .filter(|r| status.as_ref().map(|s| &r.status == s).unwrap_or(true))
            .cloned()
            .collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v.truncate(limit);
        v
    }

    /// Runs that were mid-flight when the process last stopped. The startup
    /// resume pass continues these from `current_step`.
    pub fn resumable(&self) -> Vec<Run> {
        let mut v: Vec<Run> = self.runs.values().filter(|r| r.is_resumable()).cloned().collect();
        v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        v
    }

    /// Create a workflow Run with one RunStep per workflow step. `now` is
    /// injected so the store stays deterministic in tests (the app passes
    /// `Utc::now().to_rfc3339()`).
    pub fn create_workflow_run(
        &mut self,
        run_id: String,
        workflow_id: String,
        step_labels: Vec<String>,
        now: String,
    ) -> Run {
        let steps = step_labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| RunStep {
                index,
                label,
                status: StepStatus::Pending,
                output: None,
                error: None,
                attempts: 0,
            })
            .collect();
        let run = Run {
            id: run_id.clone(),
            kind: RunKind::Workflow { workflow_id },
            status: RunStatus::Running,
            steps,
            current_step: 0,
            inputs: serde_json::Value::Null,
            prompt: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.runs.insert(run_id, run.clone());
        self.prune();
        run
    }

    /// Attach the originating prompt to a Run (W2.8). Called right after
    /// creation when the instruction is known.
    pub fn set_prompt(&mut self, id: &str, prompt: Prompt, now: String) -> Option<Run> {
        self.update(id, now, |run| run.prompt = Some(prompt))
    }

    /// Create a Run for an agent-chain execution (W2.8). One Task per node,
    /// so a chain run appears in the unified registry like a workflow run,
    /// replacing the orchestrator's separate single-slot run tracking.
    pub fn create_chain_run(
        &mut self,
        run_id: String,
        chain_id: String,
        node_labels: Vec<String>,
        now: String,
    ) -> Run {
        let steps = node_labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| RunStep {
                index,
                label,
                status: StepStatus::Pending,
                output: None,
                error: None,
                attempts: 0,
            })
            .collect();
        let run = Run {
            id: run_id.clone(),
            kind: RunKind::Chain { chain_id },
            status: RunStatus::Running,
            steps,
            current_step: 0,
            inputs: serde_json::Value::Null,
            prompt: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.runs.insert(run_id, run.clone());
        self.prune();
        run
    }

    /// Apply a mutation to one Run, stamp `updated_at`, and persist. Returns
    /// the updated Run. This is the single write path so every transition is
    /// durable.
    pub fn update<F: FnOnce(&mut Run)>(&mut self, id: &str, now: String, f: F) -> Option<Run> {
        let run = self.runs.get_mut(id)?;
        f(run);
        run.updated_at = now;
        let snapshot = run.clone();
        self.prune();
        self.save();
        Some(snapshot)
    }

    /// Mark step `index` as running (attempt incremented) and persist.
    pub fn begin_step(&mut self, id: &str, index: usize, now: String) -> Option<Run> {
        self.update(id, now, |run| {
            if let Some(step) = run.steps.get_mut(index) {
                step.status = StepStatus::Running;
                step.attempts += 1;
            }
            run.current_step = index;
        })
    }

    /// Record a successful step and advance. Completes the Run when the last
    /// step finishes.
    pub fn complete_step(&mut self, id: &str, index: usize, output: String, now: String) -> Option<Run> {
        self.update(id, now, |run| {
            if let Some(step) = run.steps.get_mut(index) {
                step.status = StepStatus::Done;
                step.output = Some(output);
            }
            run.current_step = index + 1;
            if run.current_step >= run.steps.len() {
                run.status = RunStatus::Completed;
            }
        })
    }

    /// Mark a step (and the Run) failed.
    pub fn fail_step(&mut self, id: &str, index: usize, error: String, now: String) -> Option<Run> {
        self.update(id, now, |run| {
            if let Some(step) = run.steps.get_mut(index) {
                step.status = StepStatus::Failed;
                step.error = Some(error.clone());
            }
            run.status = RunStatus::Failed;
            run.error = Some(error);
        })
    }

    pub fn stop(&mut self, id: &str, now: String) -> Option<Run> {
        self.update(id, now, |run| {
            run.status = RunStatus::Stopped;
        })
    }

    /// Drop the oldest terminal runs beyond the cap. Active runs are kept.
    fn prune(&mut self) {
        let is_terminal = |s: &RunStatus| {
            matches!(
                s,
                RunStatus::Completed | RunStatus::Failed | RunStatus::Stopped
            )
        };
        let mut terminal: Vec<(String, String)> = self
            .runs
            .iter()
            .filter(|(_, r)| is_terminal(&r.status))
            .map(|(id, r)| (id.clone(), r.created_at.clone()))
            .collect();
        if terminal.len() <= MAX_TERMINAL_RUNS {
            return;
        }
        // Oldest first, drop the overflow.
        terminal.sort_by(|a, b| a.1.cmp(&b.1));
        let drop_n = terminal.len() - MAX_TERMINAL_RUNS;
        for (id, _) in terminal.into_iter().take(drop_n) {
            self.runs.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("step {}", i)).collect()
    }

    #[test]
    fn prompt_attaches_and_tasks_project_from_steps() {
        let mut store = RunStore::default();
        store.create_workflow_run("r1".into(), "wf1".into(), labels(2), "t0".into());
        store.set_prompt(
            "r1",
            Prompt::new("summarize my inbox", "user", "t0".into()),
            "t1".into(),
        );
        let run = store.get("r1").unwrap();
        assert_eq!(run.prompt.as_ref().unwrap().text, "summarize my inbox");
        assert_eq!(run.prompt.as_ref().unwrap().source, "user");
        // Tasks project one-per-step with matching identity.
        let tasks = run.tasks();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].label, "step 0");
        assert_eq!(tasks[0].status, StepStatus::Pending);
    }

    #[test]
    fn chain_run_has_chain_kind() {
        let mut store = RunStore::default();
        let run = store.create_chain_run("c1".into(), "chainA".into(), vec!["agent1".into(), "agent2".into()], "t0".into());
        assert_eq!(run.kind, RunKind::Chain { chain_id: "chainA".into() });
        assert_eq!(run.tasks().len(), 2);
    }

    #[test]
    fn run_lifecycle_advances_to_completed() {
        let mut store = RunStore::default();
        store.create_workflow_run("r1".into(), "wf1".into(), labels(2), "t0".into());
        store.begin_step("r1", 0, "t1".into());
        store.complete_step("r1", 0, "out0".into(), "t2".into());
        let mid = store.get("r1").unwrap();
        assert_eq!(mid.status, RunStatus::Running);
        assert_eq!(mid.current_step, 1);

        store.begin_step("r1", 1, "t3".into());
        let done = store.complete_step("r1", 1, "out1".into(), "t4".into()).unwrap();
        assert_eq!(done.status, RunStatus::Completed);
        assert_eq!(done.outputs(), vec!["out0", "out1"]);
    }

    #[test]
    fn failed_step_fails_run() {
        let mut store = RunStore::default();
        store.create_workflow_run("r1".into(), "wf1".into(), labels(3), "t0".into());
        store.begin_step("r1", 0, "t1".into());
        let failed = store.fail_step("r1", 0, "boom".into(), "t2".into()).unwrap();
        assert_eq!(failed.status, RunStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("boom"));
        assert_eq!(failed.steps[0].status, StepStatus::Failed);
    }

    /// Concurrency: two runs advance independently, no shared slot.
    #[test]
    fn concurrent_runs_do_not_clobber() {
        let mut store = RunStore::default();
        store.create_workflow_run("a".into(), "wfa".into(), labels(2), "t0".into());
        store.create_workflow_run("b".into(), "wfb".into(), labels(2), "t0".into());
        store.begin_step("a", 0, "t1".into());
        store.complete_step("a", 0, "a0".into(), "t2".into());
        // b is untouched while a advanced.
        assert_eq!(store.get("a").unwrap().current_step, 1);
        assert_eq!(store.get("b").unwrap().current_step, 0);
        store.begin_step("b", 0, "t3".into());
        store.complete_step("b", 0, "b0".into(), "t4".into());
        assert_eq!(store.get("a").unwrap().current_step, 1);
        assert_eq!(store.get("b").unwrap().current_step, 1);
    }

    /// A run left Running (interrupted) is resumable; terminal runs are not.
    #[test]
    fn resumable_detects_interrupted_runs() {
        let mut store = RunStore::default();
        store.create_workflow_run("live".into(), "wf".into(), labels(3), "t0".into());
        store.begin_step("live", 0, "t1".into());
        store.complete_step("live", 0, "o".into(), "t2".into());
        store.create_workflow_run("dead".into(), "wf".into(), labels(1), "t0".into());
        store.begin_step("dead", 0, "t1".into());
        store.complete_step("dead", 0, "o".into(), "t2".into()); // -> Completed

        let resumable = store.resumable();
        assert_eq!(resumable.len(), 1);
        assert_eq!(resumable[0].id, "live");
        assert_eq!(resumable[0].current_step, 1); // resumes at step 1
    }

    #[test]
    fn prune_keeps_active_drops_oldest_terminal() {
        let mut store = RunStore::default();
        // one active run must always survive
        store.create_workflow_run("active".into(), "wf".into(), labels(5), "t0".into());
        for i in 0..(MAX_TERMINAL_RUNS + 10) {
            let id = format!("t{:05}", i);
            store.create_workflow_run(id.clone(), "wf".into(), labels(1), format!("ts{:05}", i));
            store.begin_step(&id, 0, "x".into());
            store.complete_step(&id, 0, "o".into(), "x".into());
        }
        assert!(store.runs.contains_key("active"));
        let terminal = store.runs.values().filter(|r| r.status == RunStatus::Completed).count();
        assert_eq!(terminal, MAX_TERMINAL_RUNS);
    }
}
