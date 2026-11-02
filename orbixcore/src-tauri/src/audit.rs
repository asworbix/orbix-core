use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Audit Entry
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub action: String,            // "device.registered", "work_item.assigned", "playbook.updated", etc.
    pub category: AuditCategory,
    pub actor: String,             // device name, "system", or user
    pub actor_id: Option<String>,
    pub target_type: String,       // "device", "work_item", "playbook", "routing_rule", etc.
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub details: String,
    pub before_state: Option<String>,  // JSON snapshot of old state
    pub after_state: Option<String>,   // JSON snapshot of new state
    pub severity: AuditSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditCategory {
    Device,
    WorkItem,
    Routing,
    Playbook,
    Performance,
    Security,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditSeverity {
    Info,
    Warning,
    Critical,
}

// ─────────────────────────────────────────────
//  Compliance
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,          // "approval", "data_handling", "sla", "security"
    pub enabled: bool,
    pub check_type: ComplianceCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceCheck {
    RequiresApprovalAbovePriority(String),  // priority level
    MaxProcessingMinutes(u32),
    RequiresPlaybook,
    MaxFailureRate(f32),                    // percentage 0-100
    MinUptimePercent(f32),
    NoUnacknowledgedAlerts,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceViolation {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub severity: String,          // "low", "medium", "high", "critical"
    pub details: String,
    pub detected_at: String,
    pub resolved: bool,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCompliance {
    pub device_id: String,
    pub device_name: String,
    pub role: String,
    pub compliance_score: f32,     // 0-100
    pub rules_passed: u32,
    pub rules_failed: u32,
    pub violations: Vec<ComplianceViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceSummary {
    pub fleet_compliance_score: f32,
    pub total_rules: u32,
    pub rules_passing: u32,
    pub active_violations: u32,
    pub critical_violations: u32,
    pub devices: Vec<DeviceCompliance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub max_entries: usize,
    pub max_age_days: u32,
    pub auto_export: bool,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct AuditStore {
    pub entries: Vec<AuditEntry>,
    pub compliance_rules: Vec<ComplianceRule>,
    pub violations: Vec<ComplianceViolation>,
    pub retention: RetentionPolicy,
}

impl Default for AuditStore {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            compliance_rules: Self::default_rules(),
            violations: Vec::new(),
            retention: RetentionPolicy {
                max_entries: 5000,
                max_age_days: 90,
                auto_export: false,
            },
        }
    }
}

impl AuditStore {
    fn default_rules() -> Vec<ComplianceRule> {
        vec![
            ComplianceRule {
                id: "cr-playbook".into(),
                name: "Playbook Required".into(),
                description: "All fleet devices must have an assigned playbook".into(),
                category: "security".into(),
                enabled: true,
                check_type: ComplianceCheck::RequiresPlaybook,
            },
            ComplianceRule {
                id: "cr-sla".into(),
                name: "SLA Adherence".into(),
                description: "Work items must be processed within 30 minutes".into(),
                category: "sla".into(),
                enabled: true,
                check_type: ComplianceCheck::MaxProcessingMinutes(30),
            },
            ComplianceRule {
                id: "cr-failure-rate".into(),
                name: "Failure Rate Limit".into(),
                description: "Device failure rate must not exceed 20%".into(),
                category: "data_handling".into(),
                enabled: true,
                check_type: ComplianceCheck::MaxFailureRate(20.0),
            },
            ComplianceRule {
                id: "cr-uptime".into(),
                name: "Minimum Uptime".into(),
                description: "Devices must maintain at least 95% uptime".into(),
                category: "sla".into(),
                enabled: true,
                check_type: ComplianceCheck::MinUptimePercent(95.0),
            },
            ComplianceRule {
                id: "cr-approval".into(),
                name: "High Priority Approval".into(),
                description: "Urgent work items require approval before processing".into(),
                category: "approval".into(),
                enabled: false,
                check_type: ComplianceCheck::RequiresApprovalAbovePriority("Urgent".into()),
            },
        ]
    }

    // ── Audit Log ───────────────────────────

    pub fn log(&mut self, mut entry: AuditEntry) -> AuditEntry {
        if entry.id.is_empty() {
            entry.id = Uuid::new_v4().to_string();
        }
        if entry.timestamp.is_empty() {
            entry.timestamp = Utc::now().to_rfc3339();
        }
        self.entries.push(entry.clone());
        self.enforce_retention();
        entry
    }

    fn enforce_retention(&mut self) {
        if self.entries.len() > self.retention.max_entries {
            self.entries.drain(0..self.entries.len() - self.retention.max_entries);
        }
    }

    pub fn search(
        &self,
        query: Option<&str>,
        category: Option<&str>,
        severity: Option<&str>,
        device_id: Option<&str>,
        action: Option<&str>,
        limit: usize,
    ) -> Vec<AuditEntry> {
        self.entries.iter().rev()
            .filter(|e| {
                if let Some(q) = query {
                    let q_lower = q.to_lowercase();
                    if !e.details.to_lowercase().contains(&q_lower)
                        && !e.action.to_lowercase().contains(&q_lower)
                        && !e.actor.to_lowercase().contains(&q_lower)
                        && !e.target_name.as_ref().map(|n| n.to_lowercase().contains(&q_lower)).unwrap_or(false)
                    { return false; }
                }
                if let Some(cat) = category {
                    let cat_str = format!("{:?}", e.category);
                    if !cat_str.eq_ignore_ascii_case(cat) { return false; }
                }
                if let Some(sev) = severity {
                    let sev_str = format!("{:?}", e.severity);
                    if !sev_str.eq_ignore_ascii_case(sev) { return false; }
                }
                if let Some(did) = device_id {
                    if e.actor_id.as_deref() != Some(did) && e.target_id.as_deref() != Some(did) { return false; }
                }
                if let Some(act) = action {
                    if !e.action.contains(act) { return false; }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get_retention(&self) -> RetentionPolicy {
        self.retention.clone()
    }

    pub fn update_retention(&mut self, policy: RetentionPolicy) -> RetentionPolicy {
        self.retention = policy.clone();
        self.enforce_retention();
        policy
    }

    // ── Compliance Rules ────────────────────

    pub fn list_compliance_rules(&self) -> Vec<ComplianceRule> {
        self.compliance_rules.clone()
    }

    pub fn toggle_compliance_rule(&mut self, id: &str) -> Option<ComplianceRule> {
        if let Some(rule) = self.compliance_rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
            Some(rule.clone())
        } else {
            None
        }
    }

    pub fn create_compliance_rule(&mut self, mut rule: ComplianceRule) -> ComplianceRule {
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }
        self.compliance_rules.push(rule.clone());
        rule
    }

    pub fn delete_compliance_rule(&mut self, id: &str) -> bool {
        let len = self.compliance_rules.len();
        self.compliance_rules.retain(|r| r.id != id);
        self.compliance_rules.len() < len
    }

    // ── Violations ──────────────────────────

    pub fn add_violation(&mut self, mut v: ComplianceViolation) -> ComplianceViolation {
        if v.id.is_empty() {
            v.id = Uuid::new_v4().to_string();
        }
        if v.detected_at.is_empty() {
            v.detected_at = Utc::now().to_rfc3339();
        }
        self.violations.push(v.clone());
        v
    }

    pub fn list_violations(&self, active_only: bool) -> Vec<ComplianceViolation> {
        if active_only {
            self.violations.iter().filter(|v| !v.resolved).cloned().collect()
        } else {
            self.violations.clone()
        }
    }

    pub fn resolve_violation(&mut self, id: &str) -> bool {
        if let Some(v) = self.violations.iter_mut().find(|v| v.id == id) {
            v.resolved = true;
            v.resolved_at = Some(Utc::now().to_rfc3339());
            true
        } else {
            false
        }
    }

    pub fn compute_compliance(
        &self,
        devices: &[(String, String, String, bool, f32, f32)], // (id, name, role, has_playbook, failure_rate, uptime)
    ) -> ComplianceSummary {
        let enabled_rules: Vec<&ComplianceRule> = self.compliance_rules.iter().filter(|r| r.enabled).collect();
        let total_rules = enabled_rules.len() as u32;

        let mut device_comps = Vec::new();
        let mut fleet_passed = 0u32;
        let mut fleet_total = 0u32;

        for (id, name, role, has_pb, failure_rate, uptime) in devices {
            let mut passed = 0u32;
            let mut failed = 0u32;
            let mut dev_violations = Vec::new();

            for rule in &enabled_rules {
                let ok = match &rule.check_type {
                    ComplianceCheck::RequiresPlaybook => *has_pb,
                    ComplianceCheck::MaxFailureRate(max) => *failure_rate <= *max,
                    ComplianceCheck::MinUptimePercent(min) => *uptime >= *min,
                    ComplianceCheck::MaxProcessingMinutes(_) => true, // checked per-item
                    ComplianceCheck::RequiresApprovalAbovePriority(_) => true,
                    ComplianceCheck::NoUnacknowledgedAlerts => true,
                    ComplianceCheck::Custom(_) => true,
                };
                if ok { passed += 1; } else {
                    failed += 1;
                    let existing = self.violations.iter().find(|v| v.rule_id == rule.id && v.device_id.as_deref() == Some(id) && !v.resolved);
                    if let Some(v) = existing {
                        dev_violations.push(v.clone());
                    } else {
                        dev_violations.push(ComplianceViolation {
                            id: Uuid::new_v4().to_string(),
                            rule_id: rule.id.clone(),
                            rule_name: rule.name.clone(),
                            device_id: Some(id.clone()),
                            device_name: Some(name.clone()),
                            severity: if matches!(rule.check_type, ComplianceCheck::RequiresPlaybook) { "high".into() } else { "medium".into() },
                            details: format!("{}: failed for {} ({})", rule.name, name, role),
                            detected_at: Utc::now().to_rfc3339(),
                            resolved: false,
                            resolved_at: None,
                        });
                    }
                }
                fleet_total += 1;
                if ok { fleet_passed += 1; }
            }

            let score = if total_rules > 0 { passed as f32 / total_rules as f32 * 100.0 } else { 100.0 };
            device_comps.push(DeviceCompliance {
                device_id: id.clone(),
                device_name: name.clone(),
                role: role.clone(),
                compliance_score: score,
                rules_passed: passed,
                rules_failed: failed,
                violations: dev_violations,
            });
        }

        let fleet_score = if fleet_total > 0 { fleet_passed as f32 / fleet_total as f32 * 100.0 } else { 100.0 };
        let active_violations = self.violations.iter().filter(|v| !v.resolved).count() as u32;
        let critical = self.violations.iter().filter(|v| !v.resolved && (v.severity == "high" || v.severity == "critical")).count() as u32;

        ComplianceSummary {
            fleet_compliance_score: fleet_score,
            total_rules,
            rules_passing: fleet_passed,
            active_violations,
            critical_violations: critical,
            devices: device_comps,
        }
    }
}
