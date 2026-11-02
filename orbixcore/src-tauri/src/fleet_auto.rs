use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Automation Rules
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub priority: u32,                  // lower = higher priority
    pub condition: AutoCondition,
    pub action: AutoAction,
    pub cooldown_secs: u32,             // min seconds between triggers
    pub last_triggered: Option<String>,
    pub trigger_count: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoCondition {
    DeviceOffline,
    DeviceError,
    SlaBreached,
    FailureRateAbove(f32),              // percentage 0-100
    WorkloadAbove(u32),                 // threshold
    QueueOverflow(u32),                 // max items
    DeviceIdleMinutes(u32),
    ComplianceViolation,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoAction {
    ReassignWorkItems,
    EscalateToRole(String),
    PauseDevice,
    NotifyAdmin(String),                // message
    RebalanceWorkload,
    RunPlaybook(String),                // playbook_id
    CreateAlert(String),                // alert message
    RestartDeviceRole,
    Chain(Vec<AutoAction>),             // multiple actions in sequence
    Custom(String),
}

// ─────────────────────────────────────────────
//  Execution Log
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoExecution {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub condition_met: String,          // human-readable condition description
    pub action_taken: String,           // human-readable action description
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub success: bool,
    pub details: String,
    pub executed_at: String,
}

// ─────────────────────────────────────────────
//  Templates
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,               // "reliability", "performance", "security", "workload"
    pub condition: AutoCondition,
    pub action: AutoAction,
    pub cooldown_secs: u32,
    pub is_builtin: bool,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetAutoStore {
    pub rules: Vec<AutomationRule>,
    pub executions: Vec<AutoExecution>,
    pub templates: Vec<AutoTemplate>,
}

impl Default for FleetAutoStore {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            executions: Vec::new(),
            templates: Self::builtin_templates(),
        }
    }
}

impl FleetAutoStore {
    fn builtin_templates() -> Vec<AutoTemplate> {
        vec![
            AutoTemplate {
                id: "tpl-offline-reassign".into(),
                name: "Offline Device Reassignment".into(),
                description: "When a device goes offline, reassign its work items to available devices".into(),
                category: "reliability".into(),
                condition: AutoCondition::DeviceOffline,
                action: AutoAction::ReassignWorkItems,
                cooldown_secs: 60,
                is_builtin: true,
            },
            AutoTemplate {
                id: "tpl-sla-escalate".into(),
                name: "SLA Breach Escalation".into(),
                description: "Escalate work items to a supervisor role when SLA is breached".into(),
                category: "performance".into(),
                condition: AutoCondition::SlaBreached,
                action: AutoAction::Chain(vec![
                    AutoAction::EscalateToRole("Supervisor".into()),
                    AutoAction::CreateAlert("SLA breach detected - items escalated".into()),
                ]),
                cooldown_secs: 300,
                is_builtin: true,
            },
            AutoTemplate {
                id: "tpl-failure-pause".into(),
                name: "High Failure Rate Pause".into(),
                description: "Pause a device and notify admin when failure rate exceeds 30%".into(),
                category: "reliability".into(),
                condition: AutoCondition::FailureRateAbove(30.0),
                action: AutoAction::Chain(vec![
                    AutoAction::PauseDevice,
                    AutoAction::NotifyAdmin("Device paused due to high failure rate (>30%)".into()),
                ]),
                cooldown_secs: 600,
                is_builtin: true,
            },
            AutoTemplate {
                id: "tpl-overload-rebalance".into(),
                name: "Workload Rebalancer".into(),
                description: "Rebalance work when a device's workload exceeds its capacity".into(),
                category: "workload".into(),
                condition: AutoCondition::WorkloadAbove(90),
                action: AutoAction::RebalanceWorkload,
                cooldown_secs: 120,
                is_builtin: true,
            },
            AutoTemplate {
                id: "tpl-queue-alert".into(),
                name: "Queue Overflow Alert".into(),
                description: "Alert admin when queued items exceed threshold".into(),
                category: "workload".into(),
                condition: AutoCondition::QueueOverflow(50),
                action: AutoAction::Chain(vec![
                    AutoAction::CreateAlert("Queue overflow - too many items waiting".into()),
                    AutoAction::NotifyAdmin("Queue overflow detected, manual review needed".into()),
                ]),
                cooldown_secs: 300,
                is_builtin: true,
            },
            AutoTemplate {
                id: "tpl-compliance-alert".into(),
                name: "Compliance Violation Response".into(),
                description: "Create alert and notify admin when compliance violations are detected".into(),
                category: "security".into(),
                condition: AutoCondition::ComplianceViolation,
                action: AutoAction::Chain(vec![
                    AutoAction::CreateAlert("Compliance violation detected".into()),
                    AutoAction::NotifyAdmin("Compliance violation requires review".into()),
                ]),
                cooldown_secs: 600,
                is_builtin: true,
            },
        ]
    }

    // ── Rules ──────────────────────────────────

    pub fn list_rules(&self) -> Vec<AutomationRule> {
        let mut rules = self.rules.clone();
        rules.sort_by_key(|r| r.priority);
        rules
    }

    pub fn create_rule(&mut self, mut rule: AutomationRule) -> AutomationRule {
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }
        if rule.created_at.is_empty() {
            rule.created_at = Utc::now().to_rfc3339();
        }
        self.rules.push(rule.clone());
        rule
    }

    pub fn update_rule(&mut self, rule: AutomationRule) -> Option<AutomationRule> {
        if let Some(existing) = self.rules.iter_mut().find(|r| r.id == rule.id) {
            existing.name = rule.name;
            existing.description = rule.description;
            existing.enabled = rule.enabled;
            existing.priority = rule.priority;
            existing.condition = rule.condition;
            existing.action = rule.action;
            existing.cooldown_secs = rule.cooldown_secs;
            Some(existing.clone())
        } else {
            None
        }
    }

    pub fn delete_rule(&mut self, id: &str) -> bool {
        let len = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < len
    }

    pub fn toggle_rule(&mut self, id: &str) -> Option<AutomationRule> {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
            Some(rule.clone())
        } else {
            None
        }
    }

    pub fn create_rule_from_template(&mut self, template_id: &str, name: Option<String>) -> Option<AutomationRule> {
        let tpl = self.templates.iter().find(|t| t.id == template_id)?.clone();
        let rule = AutomationRule {
            id: Uuid::new_v4().to_string(),
            name: name.unwrap_or(tpl.name),
            description: tpl.description,
            enabled: true,
            priority: (self.rules.len() as u32) + 1,
            condition: tpl.condition,
            action: tpl.action,
            cooldown_secs: tpl.cooldown_secs,
            last_triggered: None,
            trigger_count: 0,
            created_at: Utc::now().to_rfc3339(),
        };
        self.rules.push(rule.clone());
        Some(rule)
    }

    // ── Execution Log ──────────────────────────

    pub fn log_execution(&mut self, mut exec: AutoExecution) -> AutoExecution {
        if exec.id.is_empty() {
            exec.id = Uuid::new_v4().to_string();
        }
        if exec.executed_at.is_empty() {
            exec.executed_at = Utc::now().to_rfc3339();
        }
        // Update the rule's trigger info
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == exec.rule_id) {
            rule.trigger_count += 1;
            rule.last_triggered = Some(exec.executed_at.clone());
        }
        self.executions.push(exec.clone());
        // Keep last 1000 executions
        if self.executions.len() > 1000 {
            self.executions.drain(0..self.executions.len() - 1000);
        }
        exec
    }

    pub fn list_executions(&self, rule_id: Option<&str>, success_only: Option<bool>, limit: usize) -> Vec<AutoExecution> {
        self.executions.iter().rev()
            .filter(|e| {
                if let Some(rid) = rule_id {
                    if e.rule_id != rid { return false; }
                }
                if let Some(ok) = success_only {
                    if e.success != ok { return false; }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn clear_executions(&mut self) {
        self.executions.clear();
    }

    pub fn execution_stats(&self) -> AutoStats {
        let total = self.executions.len() as u32;
        let successful = self.executions.iter().filter(|e| e.success).count() as u32;
        let failed = total - successful;
        let active_rules = self.rules.iter().filter(|r| r.enabled).count() as u32;
        let total_rules = self.rules.len() as u32;
        let last_execution = self.executions.last().map(|e| e.executed_at.clone());
        AutoStats { total_executions: total, successful, failed, active_rules, total_rules, last_execution }
    }

    // ── Templates ──────────────────────────────

    pub fn list_templates(&self) -> Vec<AutoTemplate> {
        self.templates.clone()
    }

    pub fn get_template(&self, id: &str) -> Option<AutoTemplate> {
        self.templates.iter().find(|t| t.id == id).cloned()
    }

    pub fn create_template(&mut self, mut tpl: AutoTemplate) -> AutoTemplate {
        if tpl.id.is_empty() {
            tpl.id = Uuid::new_v4().to_string();
        }
        tpl.is_builtin = false;
        self.templates.push(tpl.clone());
        tpl
    }

    pub fn delete_template(&mut self, id: &str) -> bool {
        let len = self.templates.len();
        self.templates.retain(|t| t.id != id || t.is_builtin);
        self.templates.len() < len
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoStats {
    pub total_executions: u32,
    pub successful: u32,
    pub failed: u32,
    pub active_rules: u32,
    pub total_rules: u32,
    pub last_execution: Option<String>,
}
