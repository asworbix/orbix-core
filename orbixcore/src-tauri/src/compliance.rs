//! Compliance policy - the workspace-level contract that governs which
//! actions are approval-gated, who approves them, what residency applies,
//! and what to retain.
//!
//! The policy is a single YAML file (see
//! `templates/compliance.policy.example.yaml`) loaded at workspace open
//! and re-loaded on every save. The runtime consults it before executing
//! any side-effecting action; the audit log records every consultation
//! and every decision, even when the answer is "no gate needed."
//!
//! Single-tenant defaults are the strict baseline. Teams override or
//! relax via the same file - we never lower the floor, we only specify
//! who else can satisfy it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ─────────────────────────────────────────────
//  Top-level policy
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompliancePolicy {
    /// Schema version. Bump on breaking changes; we keep readers
    /// compatible across one major version.
    pub version: u32,

    /// Stable workspace UUID generated at first activation.
    pub workspace_id: String,

    /// Human-friendly workspace name (shown in audit reports).
    pub workspace_name: String,

    /// Frameworks the workspace claims compliance with. Used to tag
    /// generated evidence reports and to gate framework-specific rules
    /// (e.g. PHI handling kicks in only when HIPAA is claimed).
    #[serde(default)]
    pub frameworks: Vec<Framework>,

    /// Data residency rules. Hard-enforced at runtime - agents cannot
    /// reach a tool/model outside the allowed regions when the input
    /// touches a tagged entity.
    pub residency: Residency,

    /// Retention rules per data class. Audit log retention is
    /// tombstone-only; conversational data may rotate.
    pub retention: Retention,

    /// Action-type → approval-rule registry. Anything in this map that
    /// matches a step's tool/action key triggers the rule unless the
    /// step explicitly sets `auto_approve: true` AND the rule allows
    /// override.
    pub action_gates: BTreeMap<String, ApprovalRule>,

    /// Redaction posture for PII/PHI. `Strict` fails the run on a
    /// missed identifier; `Mask` substitutes a token; `Off` is only
    /// available outside compliance mode.
    pub redaction: RedactionMode,

    /// Principals registered in this workspace. A solo install has one
    /// principal holding every role.
    pub principals: Vec<Principal>,

    /// Role → principal mapping. Roles referenced by approval rules
    /// resolve through this table.
    pub role_assignments: BTreeMap<String, Vec<String>>,

    /// Default SLA + escalation when an approval rule doesn't override.
    pub approval_defaults: ApprovalDefaults,

    /// Workspace-level toggles.
    pub toggles: PolicyToggles,

    /// ISO-4217 currency code used to interpret tiered rule amounts.
    /// Audit reports stamp this on every monetary approval row so an
    /// auditor never has to guess whether "1000" meant USD or EUR.
    /// Defaults to USD.
    #[serde(default = "default_base_currency")]
    pub base_currency: String,
}

fn default_base_currency() -> String { "USD".to_string() }

// ─────────────────────────────────────────────
//  Frameworks
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    Soc2Type2,
    Iso27001,
    Gdpr,
    Hipaa,
    Nis2,
    Ccpa,
    Finra,
    Internal,
}

// ─────────────────────────────────────────────
//  Residency
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Residency {
    /// Default region for unflagged data.
    pub default_region: Region,
    /// Regions a workflow may reach when an input is tagged with the
    /// matching residency. EU-tagged data must stay in the EU set.
    pub region_routes: BTreeMap<Region, Vec<Region>>,
    /// When true, hitting a tool/model outside the allowed regions
    /// fails the step. When false, the relay attempts redaction first.
    /// Compliance mode forces `true`.
    pub hard_gate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Eu,
    Us,
    Uk,
    Apac,
    Global,
    OnDevice,
}

// ─────────────────────────────────────────────
//  Retention
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Retention {
    /// Audit log is tombstone-only. Right-to-erasure writes a tombstone
    /// row pointing at the redacted subject without breaking the chain.
    pub audit_log: AuditRetention,

    /// Conversational and generated artifacts may rotate after N days.
    pub conversations_days: Option<u32>,
    pub generated_artifacts_days: Option<u32>,

    /// Workflow run logs: keep N most recent runs per workflow, plus
    /// any run referenced by an approval (kept indefinitely).
    pub workflow_runs_per_workflow: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRetention {
    /// Always `Tombstone` in compliance mode. Constant kept here so a
    /// future "never tombstone" mode can be added without schema change.
    pub mode: AuditRetentionMode,
    /// Optional minimum hold (days) before any tombstoning is even
    /// allowed. Some frameworks require a 6 or 7 year minimum.
    pub minimum_hold_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditRetentionMode {
    Tombstone,
    NeverDelete,
}

// ─────────────────────────────────────────────
//  Approval rules
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    /// Hand-off shape: informational, single, dual-control, tiered,
    /// quorum.
    pub kind: ApprovalKind,

    /// Who must approve. Resolved against `role_assignments` and the
    /// workspace's principals.
    pub approvers: Vec<Approver>,

    /// SLA - how long Bob waits before escalating per `escalation`.
    /// Falls back to `approval_defaults.deadline_seconds`.
    pub deadline_seconds: Option<u64>,

    /// What happens at deadline: hold (default, safe), escalate, or
    /// auto_reject. `auto_approve` is intentionally not a deadline
    /// outcome - never silently approve on timeout.
    pub on_timeout: OnTimeout,

    /// Free-form reason text the approver must supply. Required for
    /// dual-control and tiered rules in compliance mode.
    pub require_reason: bool,

    /// Optional file attachment requirement (e.g. invoice PDF).
    pub require_attachment: bool,

    /// Whether the workflow author can opt out via `auto_approve: true`
    /// on the step. Always false for high-risk action types.
    pub overridable: bool,

    /// Tier rules for amount-banded approvals (only used when
    /// kind == Tiered).
    pub tiers: Option<Vec<ApprovalTier>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// FYI only - does not block the run, but is logged.
    Informational,
    /// One named approver or one principal in a role.
    SingleApprover,
    /// Two distinct principals must approve. Initiator excluded.
    DualControl,
    /// Routes to different approver sets based on amount/scope band.
    Tiered,
    /// m-of-n quorum (e.g. 2 of 3 board members).
    Quorum { m: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Approver {
    /// Direct principal (email or principal_id).
    Principal { id: String },
    /// Role lookup - first available principal holding the role.
    Role { role: String },
    /// Relationship - e.g. `reports_to:initiator.manager`.
    Relationship { rel: String },
    /// Anyone holding the role (used for quorum).
    AnyInRole { role: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalTier {
    /// Inclusive lower bound on the gated amount (in workspace base
    /// currency for finance, or step count for ops actions, etc).
    pub min: f64,
    /// Inclusive upper bound, or null for "no upper bound".
    pub max: Option<f64>,
    /// Approvers required for this tier.
    pub approvers: Vec<Approver>,
    /// Override the parent rule's kind for this band (e.g. tier 3 might
    /// require dual-control even when lower tiers are single-approver).
    pub kind: Option<ApprovalKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OnTimeout {
    /// Workflow stays paused indefinitely. Safe default.
    Hold,
    /// Notify the next role up and reset the deadline.
    Escalate { to_role: String },
    /// Write a "timed out" decision into the audit log, terminate run.
    AutoReject,
}

// ─────────────────────────────────────────────
//  Approval defaults
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDefaults {
    /// 1 hour default. Override per rule.
    pub deadline_seconds: u64,
    /// `Hold` is the only sane default - never silently auto-approve
    /// or auto-reject.
    pub on_timeout: OnTimeout,
    /// All channels notified concurrently when a gate triggers.
    pub channels: Vec<NotifyChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotifyChannel {
    Apns,         // iPhone push
    Email,        // Resend via Vercel function
    MacosLocal,   // macOS notification center
    LanWebsocket, // existing paired-LAN session
}

// ─────────────────────────────────────────────
//  Redaction
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RedactionMode {
    /// Fail the step if any registered identifier escapes the relay
    /// without a token substitution. Strictest - required by HIPAA.
    Strict,
    /// Substitute a token and continue. Default outside high-risk
    /// frameworks.
    Mask,
    /// No redaction. Disallowed in compliance mode.
    Off,
}

// ─────────────────────────────────────────────
//  Principals + toggles
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub id: String,           // stable UUID
    pub email: String,
    pub display_name: String,
    pub created_at: String,   // RFC3339
    /// Public key fingerprint for the principal's signing key.
    /// Audit-log entries authored by this principal are signed with it.
    pub key_fingerprint: String,
    /// The principal's macOS / iPhone device IDs that hold the
    /// workspace key. Multi-device per principal is supported.
    pub devices: Vec<String>,
    /// Active = false disables approval routing without deleting the
    /// audit history.
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyToggles {
    /// Master switch. When false, the workspace is "draft mode" - no
    /// gates fire, but actions still log to the audit ledger.
    pub compliance_mode: bool,

    /// Workflow definitions must be signed by the editor's principal
    /// key. Loading an unsigned workflow shows a security warning and
    /// refuses to run when this is true.
    pub signed_workflows: bool,

    /// Right-to-erasure tombstones are required (never destructive
    /// delete). Always true in compliance mode.
    pub tombstone_only: bool,

    /// EU boundary mode: any EU-tagged entity in a workflow forbids
    /// non-EU egress, period. Composes with `residency.hard_gate`.
    pub eu_boundary: bool,

    /// PHI tagging: HIPAA-flagged entities require encryption-at-rest
    /// and minimum-necessary access logging.
    pub phi_tagging: bool,
}

// ─────────────────────────────────────────────
//  Loading + defaults
// ─────────────────────────────────────────────

const POLICY_FILE: &str = "compliance.policy.yaml";
const POLICY_VERSION: u32 = 1;

impl CompliancePolicy {
    /// Load the policy from the workspace's data dir, or write a
    /// strict default on first run. The default has compliance_mode
    /// ON - users opt OUT, not in.
    pub fn load_or_default(workspace_id: &str, owner_email: &str) -> Self {
        let path = crate::storage::data_dir().join(POLICY_FILE);
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(parsed) = serde_yaml::from_slice::<CompliancePolicy>(&bytes) {
                return parsed;
            }
            log::warn!("compliance.policy could not be parsed; falling back to default");
        }
        let default = Self::strict_default(workspace_id, owner_email);
        if let Err(e) = default.save() {
            log::warn!("failed to write initial compliance.policy: {}", e);
        }
        default
    }

    pub fn save(&self) -> Result<(), String> {
        let path = crate::storage::data_dir().join(POLICY_FILE);
        let bytes = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())
    }

    /// Strict baseline - what every workspace gets on day 1. Single
    /// principal holding every role, EU boundary on, signed workflows,
    /// tombstone-only audit retention.
    pub fn strict_default(workspace_id: &str, owner_email: &str) -> Self {
        let owner_id = uuid::Uuid::new_v4().to_string();
        let mut role_assignments: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for role in ["owner", "admin", "approver", "operator", "auditor"] {
            role_assignments.insert(role.into(), vec![owner_id.clone()]);
        }

        let mut action_gates: BTreeMap<String, ApprovalRule> = BTreeMap::new();
        for action in [
            // Outbound communication
            "gmail.send", "outlook.send", "linkedin.publish",
            "slack.post", "twitter.post", "intercom.reply",
            // System-mutating
            "shell_exec", "file_write_outside_workdir",
            // Twin / state-mutating
            "create_workflow", "delete_workflow",
            "create_agent", "delete_agent",
            // Money / finance
            "stripe.charge", "stripe.refund", "wise.transfer",
            // External APIs that move data off-device
            "api_call.write",
        ] {
            action_gates.insert(action.into(), ApprovalRule {
                kind: ApprovalKind::SingleApprover,
                approvers: vec![Approver::Role { role: "approver".into() }],
                deadline_seconds: Some(3600),
                on_timeout: OnTimeout::Hold,
                require_reason: false,
                require_attachment: false,
                overridable: false, // initiator can't disable on these
                tiers: None,
            });
        }
        // Money actions get dual-control by default.
        for action in ["stripe.charge", "stripe.refund", "wise.transfer"] {
            if let Some(rule) = action_gates.get_mut(action) {
                rule.kind = ApprovalKind::DualControl;
                rule.require_reason = true;
                rule.require_attachment = true;
            }
        }

        let mut region_routes: BTreeMap<Region, Vec<Region>> = BTreeMap::new();
        region_routes.insert(Region::Eu, vec![Region::Eu, Region::OnDevice]);
        region_routes.insert(Region::Us, vec![Region::Us, Region::OnDevice]);
        region_routes.insert(Region::Uk, vec![Region::Uk, Region::Eu, Region::OnDevice]);
        region_routes.insert(Region::Global, vec![
            Region::Global, Region::Eu, Region::Us, Region::Uk, Region::Apac, Region::OnDevice,
        ]);
        region_routes.insert(Region::OnDevice, vec![Region::OnDevice]);

        Self {
            version: POLICY_VERSION,
            workspace_id: workspace_id.into(),
            workspace_name: "Workspace".into(),
            frameworks: vec![
                Framework::Soc2Type2, Framework::Iso27001, Framework::Gdpr,
                Framework::Hipaa,     Framework::Nis2,     Framework::Ccpa,
                Framework::Finra,
            ],
            residency: Residency {
                default_region: Region::OnDevice,
                region_routes,
                hard_gate: true,
            },
            retention: Retention {
                audit_log: AuditRetention {
                    mode: AuditRetentionMode::Tombstone,
                    minimum_hold_days: Some(7 * 365), // 7 years - FINRA floor
                },
                conversations_days: Some(365),
                generated_artifacts_days: Some(365),
                workflow_runs_per_workflow: 1000,
            },
            action_gates,
            redaction: RedactionMode::Strict,
            principals: vec![Principal {
                id: owner_id.clone(),
                email: owner_email.into(),
                display_name: owner_email.into(),
                created_at: chrono::Utc::now().to_rfc3339(),
                key_fingerprint: String::new(), // filled at first key generation
                devices: Vec::new(),
                active: true,
            }],
            role_assignments,
            approval_defaults: ApprovalDefaults {
                deadline_seconds: 3600,
                on_timeout: OnTimeout::Hold,
                channels: vec![
                    NotifyChannel::Apns,
                    NotifyChannel::Email,
                    NotifyChannel::MacosLocal,
                    NotifyChannel::LanWebsocket,
                ],
            },
            toggles: PolicyToggles {
                compliance_mode: true,
                signed_workflows: true,
                tombstone_only: true,
                eu_boundary: true,
                phi_tagging: true,
            },
            base_currency: "USD".into(),
        }
    }

    /// Resolve the approval rule that applies to a given action key.
    /// Returns `None` when the action is not gated.
    pub fn rule_for(&self, action_key: &str) -> Option<&ApprovalRule> {
        self.action_gates.get(action_key)
    }

    /// Look up the principals satisfying a role.
    pub fn principals_for_role(&self, role: &str) -> Vec<&Principal> {
        match self.role_assignments.get(role) {
            Some(ids) => self.principals.iter()
                .filter(|p| p.active && ids.contains(&p.id))
                .collect(),
            None => Vec::new(),
        }
    }
}
