use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Runbook Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookStep {
    pub id: String,
    pub order: u32,
    pub title: String,
    pub instructions: String,
    pub expected_outcome: String,
    pub rollback: Option<String>,
    pub estimated_mins: u32,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runbook {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub steps: Vec<RunbookStep>,
    pub estimated_mins: u32,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_id: String,
    pub status: StepStatus,
    pub notes: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookExecution {
    pub id: String,
    pub runbook_id: String,
    pub runbook_name: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub status: ExecutionStatus,
    pub current_step: u32,
    pub total_steps: u32,
    pub step_statuses: Vec<ExecutionStep>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub executed_by: String,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetRunbookStore {
    pub runbooks: Vec<Runbook>,
    pub executions: Vec<RunbookExecution>,
}

impl Default for FleetRunbookStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            runbooks: vec![
                Runbook {
                    id: "rb-deploy".into(),
                    name: "Production Deployment".into(),
                    description: "Standard deployment procedure for production devices.".into(),
                    category: "Deployment".into(),
                    steps: vec![
                        RunbookStep { id: "s1".into(), order: 1, title: "Pre-flight checks".into(), instructions: "Verify all health checks pass and no active incidents.".into(), expected_outcome: "All checks green".into(), rollback: None, estimated_mins: 5, requires_confirmation: true },
                        RunbookStep { id: "s2".into(), order: 2, title: "Enable maintenance mode".into(), instructions: "Put device into maintenance window to suppress alerts.".into(), expected_outcome: "Maintenance mode active".into(), rollback: Some("Disable maintenance mode".into()), estimated_mins: 2, requires_confirmation: false },
                        RunbookStep { id: "s3".into(), order: 3, title: "Deploy update".into(), instructions: "Pull latest release and apply configuration changes.".into(), expected_outcome: "New version running".into(), rollback: Some("Rollback to previous version".into()), estimated_mins: 10, requires_confirmation: true },
                        RunbookStep { id: "s4".into(), order: 4, title: "Verify deployment".into(), instructions: "Run smoke tests and verify core functionality.".into(), expected_outcome: "All tests pass".into(), rollback: None, estimated_mins: 5, requires_confirmation: false },
                        RunbookStep { id: "s5".into(), order: 5, title: "Exit maintenance mode".into(), instructions: "Remove maintenance window and resume normal monitoring.".into(), expected_outcome: "Normal operations resumed".into(), rollback: None, estimated_mins: 2, requires_confirmation: false },
                    ],
                    estimated_mins: 24,
                    tags: vec!["deployment".into(), "production".into()],
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
                Runbook {
                    id: "rb-incident".into(),
                    name: "Incident Response".into(),
                    description: "Standard incident response procedure for fleet issues.".into(),
                    category: "Operations".into(),
                    steps: vec![
                        RunbookStep { id: "s1".into(), order: 1, title: "Assess severity".into(), instructions: "Determine incident severity and affected scope.".into(), expected_outcome: "Severity classified".into(), rollback: None, estimated_mins: 3, requires_confirmation: false },
                        RunbookStep { id: "s2".into(), order: 2, title: "Isolate affected devices".into(), instructions: "Route traffic away from affected devices if needed.".into(), expected_outcome: "Devices isolated".into(), rollback: Some("Restore traffic routing".into()), estimated_mins: 5, requires_confirmation: true },
                        RunbookStep { id: "s3".into(), order: 3, title: "Diagnose root cause".into(), instructions: "Collect logs, check metrics, identify root cause.".into(), expected_outcome: "Root cause identified".into(), rollback: None, estimated_mins: 15, requires_confirmation: false },
                        RunbookStep { id: "s4".into(), order: 4, title: "Apply fix".into(), instructions: "Implement remediation based on diagnosis.".into(), expected_outcome: "Fix applied".into(), rollback: Some("Revert changes".into()), estimated_mins: 10, requires_confirmation: true },
                        RunbookStep { id: "s5".into(), order: 5, title: "Verify resolution".into(), instructions: "Confirm issue is resolved and service restored.".into(), expected_outcome: "Service restored".into(), rollback: None, estimated_mins: 5, requires_confirmation: false },
                    ],
                    estimated_mins: 38,
                    tags: vec!["incident".into(), "response".into()],
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
                Runbook {
                    id: "rb-onboard".into(),
                    name: "Device Onboarding".into(),
                    description: "Set up a new device in the fleet.".into(),
                    category: "Setup".into(),
                    steps: vec![
                        RunbookStep { id: "s1".into(), order: 1, title: "Register device".into(), instructions: "Add device to fleet registry with role and metadata.".into(), expected_outcome: "Device registered".into(), rollback: Some("Remove device registration".into()), estimated_mins: 3, requires_confirmation: false },
                        RunbookStep { id: "s2".into(), order: 2, title: "Assign role & playbook".into(), instructions: "Set the device role and assign appropriate playbook.".into(), expected_outcome: "Role configured".into(), rollback: None, estimated_mins: 5, requires_confirmation: false },
                        RunbookStep { id: "s3".into(), order: 3, title: "Configure monitoring".into(), instructions: "Set up capacity thresholds and alert rules.".into(), expected_outcome: "Monitoring active".into(), rollback: None, estimated_mins: 5, requires_confirmation: false },
                        RunbookStep { id: "s4".into(), order: 4, title: "Run validation".into(), instructions: "Execute health check and verify connectivity.".into(), expected_outcome: "Device healthy".into(), rollback: None, estimated_mins: 3, requires_confirmation: true },
                    ],
                    estimated_mins: 16,
                    tags: vec!["setup".into(), "onboarding".into()],
                    created_at: now.clone(),
                    updated_at: now,
                },
            ],
            executions: vec![],
        }
    }
}

impl FleetRunbookStore {
    // ── Runbooks ─────────────────────────────

    pub fn list_runbooks(&self, category: Option<&str>) -> Vec<Runbook> {
        self.runbooks.iter()
            .filter(|r| category.map(|c| r.category == c).unwrap_or(true))
            .cloned()
            .collect()
    }

    pub fn get_runbook(&self, id: &str) -> Option<Runbook> {
        self.runbooks.iter().find(|r| r.id == id).cloned()
    }

    pub fn create_runbook(&mut self, mut rb: Runbook) -> Runbook {
        if rb.id.is_empty() { rb.id = format!("rb-{}", Uuid::new_v4()); }
        let now = Utc::now().to_rfc3339();
        if rb.created_at.is_empty() { rb.created_at = now.clone(); }
        rb.updated_at = now;
        // Auto-calculate estimated time
        rb.estimated_mins = rb.steps.iter().map(|s| s.estimated_mins).sum();
        self.runbooks.push(rb.clone());
        rb
    }

    pub fn delete_runbook(&mut self, id: &str) -> bool {
        let len = self.runbooks.len();
        self.runbooks.retain(|r| r.id != id);
        self.runbooks.len() < len
    }

    pub fn list_categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self.runbooks.iter().map(|r| r.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    // ── Executions ───────────────────────────

    pub fn start_execution(&mut self, runbook_id: &str, device_id: Option<String>, device_name: Option<String>, executed_by: String) -> Option<RunbookExecution> {
        let rb = self.runbooks.iter().find(|r| r.id == runbook_id)?.clone();
        let step_statuses: Vec<ExecutionStep> = rb.steps.iter().map(|s| {
            ExecutionStep {
                step_id: s.id.clone(),
                status: StepStatus::Pending,
                notes: None,
                started_at: None,
                completed_at: None,
            }
        }).collect();

        let exec = RunbookExecution {
            id: format!("exec-{}", Uuid::new_v4()),
            runbook_id: rb.id,
            runbook_name: rb.name,
            device_id,
            device_name,
            status: ExecutionStatus::Running,
            current_step: 0,
            total_steps: rb.steps.len() as u32,
            step_statuses,
            started_at: Utc::now().to_rfc3339(),
            completed_at: None,
            executed_by,
        };
        self.executions.push(exec.clone());
        Some(exec)
    }

    pub fn advance_step(&mut self, execution_id: &str, notes: Option<String>) -> Option<RunbookExecution> {
        if let Some(exec) = self.executions.iter_mut().find(|e| e.id == execution_id) {
            let idx = exec.current_step as usize;
            if idx < exec.step_statuses.len() {
                let now = Utc::now().to_rfc3339();
                exec.step_statuses[idx].status = StepStatus::Completed;
                exec.step_statuses[idx].completed_at = Some(now.clone());
                exec.step_statuses[idx].notes = notes;

                if idx + 1 < exec.step_statuses.len() {
                    exec.current_step += 1;
                    exec.step_statuses[idx + 1].status = StepStatus::InProgress;
                    exec.step_statuses[idx + 1].started_at = Some(now);
                } else {
                    exec.status = ExecutionStatus::Completed;
                    exec.completed_at = Some(now);
                }
            }
            Some(exec.clone())
        } else { None }
    }

    pub fn fail_step(&mut self, execution_id: &str, notes: Option<String>) -> Option<RunbookExecution> {
        if let Some(exec) = self.executions.iter_mut().find(|e| e.id == execution_id) {
            let idx = exec.current_step as usize;
            if idx < exec.step_statuses.len() {
                let now = Utc::now().to_rfc3339();
                exec.step_statuses[idx].status = StepStatus::Failed;
                exec.step_statuses[idx].completed_at = Some(now.clone());
                exec.step_statuses[idx].notes = notes;
                exec.status = ExecutionStatus::Failed;
                exec.completed_at = Some(now);
            }
            Some(exec.clone())
        } else { None }
    }

    pub fn skip_step(&mut self, execution_id: &str) -> Option<RunbookExecution> {
        if let Some(exec) = self.executions.iter_mut().find(|e| e.id == execution_id) {
            let idx = exec.current_step as usize;
            if idx < exec.step_statuses.len() {
                let now = Utc::now().to_rfc3339();
                exec.step_statuses[idx].status = StepStatus::Skipped;
                exec.step_statuses[idx].completed_at = Some(now.clone());

                if idx + 1 < exec.step_statuses.len() {
                    exec.current_step += 1;
                    exec.step_statuses[idx + 1].status = StepStatus::InProgress;
                    exec.step_statuses[idx + 1].started_at = Some(now);
                } else {
                    exec.status = ExecutionStatus::Completed;
                    exec.completed_at = Some(now);
                }
            }
            Some(exec.clone())
        } else { None }
    }

    pub fn cancel_execution(&mut self, execution_id: &str) -> Option<RunbookExecution> {
        if let Some(exec) = self.executions.iter_mut().find(|e| e.id == execution_id) {
            exec.status = ExecutionStatus::Cancelled;
            exec.completed_at = Some(Utc::now().to_rfc3339());
            Some(exec.clone())
        } else { None }
    }

    pub fn list_active_executions(&self) -> Vec<RunbookExecution> {
        self.executions.iter()
            .filter(|e| e.status == ExecutionStatus::Running)
            .cloned()
            .collect()
    }

    pub fn list_executions(&self, limit: usize) -> Vec<RunbookExecution> {
        let mut execs = self.executions.clone();
        execs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        execs.truncate(limit);
        execs
    }

    pub fn get_execution(&self, id: &str) -> Option<RunbookExecution> {
        self.executions.iter().find(|e| e.id == id).cloned()
    }
}
