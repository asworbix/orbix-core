use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Decision Rules
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRule {
    pub id: String,
    pub name: String,
    pub condition: RuleCondition,
    pub action: RuleAction,
    pub priority: u32,         // lower = higher priority
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    Always,
    PriorityEquals(String),
    PriorityAtLeast(String),
    TagContains(String),
    SourceEquals(String),
    TitleContains(String),
    KeywordMatch(Vec<String>),
    Custom(String),                // free-form description
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleAction {
    Process,                       // handle normally
    Escalate(String),              // escalate to role
    Reject(String),                // reject with reason
    Defer(u32),                    // defer for N minutes
    ApplyTemplate(String),         // use response template id
    RunWorkflow(String),           // trigger workflow id
    Custom(String),                // free-form action
}

// ─────────────────────────────────────────────
//  Response Templates
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTemplate {
    pub id: String,
    pub name: String,
    pub category: String,          // "greeting", "resolution", "escalation", "follow_up"
    pub content: String,
    pub variables: Vec<String>,    // placeholders like {{customer_name}}
}

// ─────────────────────────────────────────────
//  Quality Gates
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    pub id: String,
    pub name: String,
    pub gate_type: QualityGateType,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityGateType {
    MinResponseLength(u32),
    RequiresApproval,
    MustContainKeyword(String),
    MaxProcessingMinutes(u32),
    CustomCheck(String),
}

// ─────────────────────────────────────────────
//  KPIs
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiTarget {
    pub id: String,
    pub name: String,
    pub metric: KpiMetric,
    pub target_value: f32,
    pub unit: String,              // "minutes", "items/hour", "%", "count"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KpiMetric {
    AvgResponseTime,
    ItemsPerHour,
    CompletionRate,
    EscalationRate,
    QualityScore,
    UptimePercent,
    Custom(String),
}

// ─────────────────────────────────────────────
//  Idle Behavior
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IdleBehavior {
    Wait,                          // do nothing
    PollForWork,                   // actively check for queued items
    RunMaintenance(String),        // run a maintenance workflow
    SelfImprove(String),           // review past work and optimize
    NotifyAdmin,                   // alert that device is idle
}

// ─────────────────────────────────────────────
//  Playbook
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookVersion {
    pub version: u32,
    pub changed_at: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playbook {
    pub id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub instructions: String,      // system prompt / personality
    pub decision_rules: Vec<DecisionRule>,
    pub response_templates: Vec<ResponseTemplate>,
    pub quality_gates: Vec<QualityGate>,
    pub kpi_targets: Vec<KpiTarget>,
    pub allowed_tools: Vec<String>,      // tool IDs this role can use
    pub allowed_connectors: Vec<String>, // connector IDs
    pub idle_behavior: IdleBehavior,
    pub auto_process: bool,        // auto-start processing when item arrives
    pub max_concurrent: u32,       // max items to process simultaneously
    pub is_builtin: bool,
    pub version: u32,
    pub version_history: Vec<PlaybookVersion>,
    pub created_at: String,
    pub updated_at: String,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct PlaybookStore {
    pub playbooks: Vec<Playbook>,
    pub assignments: Vec<(String, String)>, // (device_id, playbook_id)
}

impl Default for PlaybookStore {
    fn default() -> Self {
        Self {
            playbooks: Self::builtin_playbooks(),
            assignments: Vec::new(),
        }
    }
}

impl PlaybookStore {
    fn builtin_playbooks() -> Vec<Playbook> {
        vec![
            Self::make_playbook(
                "pb-lead-gen", "Lead Generator", "Lead Generator",
                "Finds and captures inbound leads from various sources.",
                "You are a Lead Generator agent. Your job is to identify, qualify, and capture inbound leads. Monitor email, web forms, and social channels for potential prospects. Extract key information: name, company, role, interest, budget signals. Score each lead 1-10 based on fit. Route high-scoring leads immediately.",
                vec![
                    DecisionRule { id: "r1".into(), name: "High-score leads".into(), condition: RuleCondition::Custom("Lead score >= 8".into()), action: RuleAction::Process, priority: 1, enabled: true },
                    DecisionRule { id: "r2".into(), name: "Low-score defer".into(), condition: RuleCondition::Custom("Lead score < 4".into()), action: RuleAction::Defer(60), priority: 2, enabled: true },
                ],
                vec![
                    ResponseTemplate { id: "t1".into(), name: "Lead captured".into(), category: "resolution".into(), content: "New lead captured: {{name}} from {{company}}. Score: {{score}}/10. Interest: {{interest}}.".into(), variables: vec!["name".into(), "company".into(), "score".into(), "interest".into()] },
                ],
                vec![
                    KpiTarget { id: "k1".into(), name: "Leads per hour".into(), metric: KpiMetric::ItemsPerHour, target_value: 5.0, unit: "leads/hr".into() },
                    KpiTarget { id: "k2".into(), name: "Avg response time".into(), metric: KpiMetric::AvgResponseTime, target_value: 2.0, unit: "minutes".into() },
                ],
                IdleBehavior::PollForWork,
                10,
            ),
            Self::make_playbook(
                "pb-triage", "Triage Bot", "Triage Bot",
                "Classifies and prioritizes incoming support tickets.",
                "You are a Triage Bot. Your job is to classify incoming support tickets by category (billing, technical, account, feature request) and priority (low, normal, high, urgent). Extract the core issue in one sentence. Route to the appropriate support tier based on complexity.",
                vec![
                    DecisionRule { id: "r1".into(), name: "Urgent tickets".into(), condition: RuleCondition::KeywordMatch(vec!["outage".into(), "down".into(), "broken".into(), "critical".into()]), action: RuleAction::Escalate("L2 Specialist".into()), priority: 1, enabled: true },
                    DecisionRule { id: "r2".into(), name: "Billing tickets".into(), condition: RuleCondition::KeywordMatch(vec!["billing".into(), "invoice".into(), "charge".into(), "refund".into()]), action: RuleAction::ApplyTemplate("billing-ack".into()), priority: 2, enabled: true },
                    DecisionRule { id: "r3".into(), name: "Default route".into(), condition: RuleCondition::Always, action: RuleAction::Process, priority: 10, enabled: true },
                ],
                vec![
                    ResponseTemplate { id: "t1".into(), name: "Ticket classified".into(), category: "resolution".into(), content: "Ticket classified: [{{category}}] Priority: {{priority}}. Summary: {{summary}}".into(), variables: vec!["category".into(), "priority".into(), "summary".into()] },
                    ResponseTemplate { id: "billing-ack".into(), name: "Billing ack".into(), category: "greeting".into(), content: "Billing inquiry received. Account: {{account}}. Forwarding to billing team.".into(), variables: vec!["account".into()] },
                ],
                vec![
                    KpiTarget { id: "k1".into(), name: "Classification time".into(), metric: KpiMetric::AvgResponseTime, target_value: 1.0, unit: "minutes".into() },
                    KpiTarget { id: "k2".into(), name: "Throughput".into(), metric: KpiMetric::ItemsPerHour, target_value: 30.0, unit: "tickets/hr".into() },
                ],
                IdleBehavior::PollForWork,
                25,
            ),
            Self::make_playbook(
                "pb-code-review", "Code Reviewer", "Code Reviewer",
                "Reviews pull requests and provides structured feedback.",
                "You are a Code Reviewer agent. Review pull requests for: correctness, security vulnerabilities, performance issues, code style, test coverage. Provide structured feedback with severity levels (critical, warning, suggestion). Block merges with critical issues. Approve clean PRs automatically.",
                vec![
                    DecisionRule { id: "r1".into(), name: "Security issues".into(), condition: RuleCondition::KeywordMatch(vec!["sql injection".into(), "xss".into(), "auth bypass".into()]), action: RuleAction::Escalate("L2 Specialist".into()), priority: 1, enabled: true },
                    DecisionRule { id: "r2".into(), name: "Standard review".into(), condition: RuleCondition::Always, action: RuleAction::Process, priority: 5, enabled: true },
                ],
                vec![
                    ResponseTemplate { id: "t1".into(), name: "Review complete".into(), category: "resolution".into(), content: "Review complete: {{file_count}} files, {{issue_count}} issues found ({{critical}} critical, {{warnings}} warnings). Verdict: {{verdict}}.".into(), variables: vec!["file_count".into(), "issue_count".into(), "critical".into(), "warnings".into(), "verdict".into()] },
                ],
                vec![
                    KpiTarget { id: "k1".into(), name: "Review time".into(), metric: KpiMetric::AvgResponseTime, target_value: 15.0, unit: "minutes".into() },
                    KpiTarget { id: "k2".into(), name: "Quality score".into(), metric: KpiMetric::QualityScore, target_value: 90.0, unit: "%".into() },
                ],
                IdleBehavior::Wait,
                5,
            ),
            Self::make_playbook(
                "pb-writer", "Content Writer", "Writer",
                "Creates content drafts from research briefs.",
                "You are a Content Writer agent. Create well-structured content from research briefs. Follow brand voice guidelines. Include SEO keywords naturally. Target word count based on content type: blog post (800-1200), social post (50-280), email (200-400). Always include a compelling headline and call-to-action.",
                vec![
                    DecisionRule { id: "r1".into(), name: "Blog posts".into(), condition: RuleCondition::TagContains("blog".into()), action: RuleAction::Process, priority: 1, enabled: true },
                    DecisionRule { id: "r2".into(), name: "Urgent content".into(), condition: RuleCondition::PriorityAtLeast("High".into()), action: RuleAction::Process, priority: 2, enabled: true },
                    DecisionRule { id: "r3".into(), name: "Default".into(), condition: RuleCondition::Always, action: RuleAction::Process, priority: 5, enabled: true },
                ],
                vec![
                    ResponseTemplate { id: "t1".into(), name: "Draft ready".into(), category: "resolution".into(), content: "Draft ready: \"{{title}}\" ({{word_count}} words, {{content_type}}). Keywords: {{keywords}}.".into(), variables: vec!["title".into(), "word_count".into(), "content_type".into(), "keywords".into()] },
                ],
                vec![
                    KpiTarget { id: "k1".into(), name: "Drafts per day".into(), metric: KpiMetric::ItemsPerHour, target_value: 1.0, unit: "drafts/hr".into() },
                    KpiTarget { id: "k2".into(), name: "Completion rate".into(), metric: KpiMetric::CompletionRate, target_value: 95.0, unit: "%".into() },
                ],
                IdleBehavior::SelfImprove("Review past drafts for style consistency".into()),
                3,
            ),
        ]
    }

    fn make_playbook(
        id: &str, name: &str, role: &str, desc: &str, instructions: &str,
        rules: Vec<DecisionRule>, templates: Vec<ResponseTemplate>,
        kpis: Vec<KpiTarget>, idle: IdleBehavior, max_concurrent: u32,
    ) -> Playbook {
        Playbook {
            id: id.to_string(),
            name: name.to_string(),
            role: role.to_string(),
            description: desc.to_string(),
            instructions: instructions.to_string(),
            decision_rules: rules,
            response_templates: templates,
            quality_gates: vec![
                QualityGate { id: "qg1".into(), name: "Min response length".into(), gate_type: QualityGateType::MinResponseLength(20), required: true },
            ],
            kpi_targets: kpis,
            allowed_tools: Vec::new(),
            allowed_connectors: Vec::new(),
            idle_behavior: idle,
            auto_process: true,
            max_concurrent,
            is_builtin: true,
            version: 1,
            version_history: vec![PlaybookVersion { version: 1, changed_at: "2025-01-01T00:00:00Z".into(), summary: "Initial version".into() }],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    // ── CRUD ────────────────────────────────

    pub fn list_playbooks(&self) -> Vec<Playbook> {
        self.playbooks.clone()
    }

    pub fn get_playbook(&self, id: &str) -> Option<Playbook> {
        self.playbooks.iter().find(|p| p.id == id).cloned()
    }

    pub fn create_playbook(&mut self, mut pb: Playbook) -> Playbook {
        if pb.id.is_empty() {
            pb.id = Uuid::new_v4().to_string();
        }
        let now = Utc::now().to_rfc3339();
        if pb.created_at.is_empty() { pb.created_at = now.clone(); }
        pb.updated_at = now;
        pb.is_builtin = false;
        pb.version = 1;
        pb.version_history = vec![PlaybookVersion { version: 1, changed_at: pb.created_at.clone(), summary: "Created".into() }];
        self.playbooks.push(pb.clone());
        pb
    }

    pub fn update_playbook(&mut self, mut pb: Playbook) -> Option<Playbook> {
        if let Some(pos) = self.playbooks.iter().position(|p| p.id == pb.id) {
            pb.version = self.playbooks[pos].version + 1;
            pb.updated_at = Utc::now().to_rfc3339();
            pb.version_history = self.playbooks[pos].version_history.clone();
            pb.version_history.push(PlaybookVersion {
                version: pb.version,
                changed_at: pb.updated_at.clone(),
                summary: format!("Updated to v{}", pb.version),
            });
            self.playbooks[pos] = pb.clone();
            Some(pb)
        } else {
            None
        }
    }

    pub fn delete_playbook(&mut self, id: &str) -> bool {
        let len = self.playbooks.len();
        self.playbooks.retain(|p| p.id != id || p.is_builtin);
        self.assignments.retain(|(_, pid)| pid != id);
        self.playbooks.len() < len
    }

    pub fn duplicate_playbook(&mut self, id: &str) -> Option<Playbook> {
        let source = self.playbooks.iter().find(|p| p.id == id)?.clone();
        let mut copy = source;
        copy.id = Uuid::new_v4().to_string();
        copy.name = format!("{} (copy)", copy.name);
        copy.is_builtin = false;
        copy.version = 1;
        let now = Utc::now().to_rfc3339();
        copy.created_at = now.clone();
        copy.updated_at = now.clone();
        copy.version_history = vec![PlaybookVersion { version: 1, changed_at: now, summary: "Duplicated".into() }];
        self.playbooks.push(copy.clone());
        Some(copy)
    }

    // ── Assignments ─────────────────────────

    pub fn assign_playbook(&mut self, device_id: &str, playbook_id: &str) {
        self.assignments.retain(|(did, _)| did != device_id);
        self.assignments.push((device_id.to_string(), playbook_id.to_string()));
    }

    pub fn unassign_playbook(&mut self, device_id: &str) {
        self.assignments.retain(|(did, _)| did != device_id);
    }

    pub fn get_device_playbook(&self, device_id: &str) -> Option<Playbook> {
        let pid = self.assignments.iter().find(|(did, _)| did == device_id)?.1.clone();
        self.get_playbook(&pid)
    }

    pub fn list_assignments(&self) -> Vec<(String, String)> {
        self.assignments.clone()
    }
}
