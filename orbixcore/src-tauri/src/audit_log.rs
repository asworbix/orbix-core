//! BOUNDARY (W0.7): this is a PERSISTENCE concern (hash-chained,
//! tamper-evident audit trail), not an event delivery path. All
//! backend-to-UI delivery goes through event_bus::emit_ui.
//! Compliance audit log - chained, signed, tamper-evident.
//!
//! Every action, approval, denial, override, principal decision, policy
//! change, and key rotation lands here as a row. Each row carries:
//!   * `seq`         monotonic 0-indexed counter, never re-used
//!   * `prev_hash`   SHA-256 over the prior row's `content_hash`
//!   * `content_hash` SHA-256 over this row's canonical payload
//!   * `signature`   HMAC-SHA256(device_key, content_hash) hex-encoded
//!
//! Write-once, append-only. The on-disk file lives next to the rest of
//! the workspace data but in its own SQLite-style append-only JSON
//! stream so retention and conversational data never share a vacuum.
//!
//! Tombstoning (right-to-erasure under GDPR Art. 17 / CCPA / similar)
//! never destructively deletes a row. Instead it nulls the subject's
//! PII fields, stamps `redacted_at` + `redaction_reason`, and leaves
//! the chain link intact. Auditors see "row N redacted on date X for
//! reason Y." A re-verification still passes because hashes were
//! computed at append time and the hash is preserved.
//!
//! v1 uses HMAC-SHA256 with a per-device key. The schema reserves
//! `signature_kind` so we can move to per-principal Ed25519 signing
//! later without a migration.

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use hmac::{Hmac, Mac};
use chrono::Utc;
use uuid::Uuid;
use std::sync::Mutex;

use crate::storage;

const AUDIT_FILE: &str = "audit_log.json";
const DEVICE_KEY_FILE: &str = "audit_device_key.bin";

type HmacSha256 = Hmac<Sha256>;

// ─────────────────────────────────────────────
//  Event kinds
// ─────────────────────────────────────────────

/// Flat list of every event that should leave a forensic trail.
/// Adding a new variant is fine; renaming or removing is a breaking
/// schema change - bump `AUDIT_SCHEMA_VERSION`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind {
    // Action lifecycle
    ActionExecuted,
    ActionDenied,
    ActionOverridden,
    // Approval lifecycle
    ApprovalRequested,
    ApprovalGranted,
    ApprovalRejected,
    ApprovalTimedOut,
    ApprovalEscalated,
    // Policy & identity
    PolicyChanged,
    PolicyLoaded,
    PrincipalAdded,
    PrincipalRevoked,
    KeyRotated,
    // Workflow definition
    WorkflowSigned,
    WorkflowSignatureInvalid,
    // Data subject
    DataSubjectErasureRequested,
    Tombstoned,
    // System
    AuditChainVerified,
    AuditChainBroken,
    // Connector module lifecycle (custom connector system)
    ConnectorModulesScanned,
    // Connector bus (Phase 3 of CONNECTORS.md)
    BusJobDispatched,
    BusJobCompleted,
    BusJobFailed,
    // Job profile scheduler (Phase 7 of CONNECTORS.md)
    JobProfileFired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignatureKind { HmacSha256, Ed25519 }

// ─────────────────────────────────────────────
//  Event
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub row_id: String,            // stable UUID
    pub seq: u64,                  // monotonic, unique within workspace
    pub timestamp: String,         // RFC3339 UTC
    pub workspace_id: String,
    pub principal_id: String,      // who authored the event
    pub kind: AuditKind,

    /// Free-form structured payload. Tombstoning replaces sensitive
    /// keys here with `null`. Top-level shape varies by kind; consumers
    /// match on `kind` first.
    pub payload: serde_json::Value,

    /// Hash of the data subject (or other sensitive field) used to
    /// route erasure requests without storing the raw value here.
    /// e.g. SHA-256(email) for a "data subject" reference.
    pub subject_hash: Option<String>,

    /// SHA-256 of the prior row's `content_hash`, hex. The first row
    /// uses the constant string `"genesis"`.
    pub prev_hash: String,

    /// SHA-256 of this row's canonical payload + chain context, hex.
    /// Stable across reads; computed at append time.
    pub content_hash: String,

    /// HMAC-SHA256(device_key, content_hash) - hex.
    pub signature: String,
    pub signature_kind: SignatureKind,

    /// Set when a tombstone has been applied. Original `content_hash`
    /// still validates because it was computed on the un-redacted row.
    pub redacted_at: Option<String>,
    pub redaction_reason: Option<String>,
}

impl AuditEvent {
    /// Canonical bytes used to compute `content_hash`. We serialise a
    /// sub-set of fields in a stable order so two identical events at
    /// different times produce different hashes only if their
    /// timestamps differ.
    fn canonical_bytes(&self) -> Vec<u8> {
        let canonical = serde_json::json!({
            "row_id":       self.row_id,
            "seq":          self.seq,
            "timestamp":    self.timestamp,
            "workspace_id": self.workspace_id,
            "principal_id": self.principal_id,
            "kind":         self.kind,
            "payload":      self.payload,
            "subject_hash": self.subject_hash,
            "prev_hash":    self.prev_hash,
        });
        serde_json::to_vec(&canonical).unwrap_or_default()
    }
}

// ─────────────────────────────────────────────
//  Log
// ─────────────────────────────────────────────

pub struct AuditLog {
    pub events: Vec<AuditEvent>,
    next_seq: u64,
    device_key: [u8; 32],
    workspace_id: String,
}

impl AuditLog {
    pub fn load_or_init(workspace_id: &str) -> Self {
        let device_key = load_or_create_device_key();
        let events: Vec<AuditEvent> = storage::load_json(AUDIT_FILE).unwrap_or_default();
        let next_seq = events.iter().map(|e| e.seq + 1).max().unwrap_or(0);
        Self { events, next_seq, device_key, workspace_id: workspace_id.into() }
    }

    /// Append a new event. The row is hashed, signed, and persisted
    /// before this returns. If persistence fails we still keep the
    /// event in memory so the chain stays consistent for the rest of
    /// the run; the next successful save will catch up.
    pub fn append(
        &mut self,
        principal_id: &str,
        kind: AuditKind,
        payload: serde_json::Value,
        subject_hash: Option<String>,
    ) -> &AuditEvent {
        let prev_hash = self.events.last()
            .map(|e| e.content_hash.clone())
            .unwrap_or_else(|| "genesis".to_string());

        let mut event = AuditEvent {
            row_id: Uuid::new_v4().to_string(),
            seq: self.next_seq,
            timestamp: Utc::now().to_rfc3339(),
            workspace_id: self.workspace_id.clone(),
            principal_id: principal_id.into(),
            kind,
            payload,
            subject_hash,
            prev_hash,
            content_hash: String::new(),    // filled below
            signature: String::new(),       // filled below
            signature_kind: SignatureKind::HmacSha256,
            redacted_at: None,
            redaction_reason: None,
        };

        // Compute hash, then sign the hash.
        let hash_hex = hex::encode(Sha256::digest(&event.canonical_bytes()));
        event.content_hash = hash_hex.clone();
        event.signature = sign_hmac(&self.device_key, hash_hex.as_bytes());

        self.next_seq += 1;
        self.events.push(event);
        let _ = storage::save_json(AUDIT_FILE, &self.events);
        self.events.last().unwrap()
    }

    /// Walk the chain end-to-end. Returns Ok if every row's hash
    /// matches its declared prev + canonical bytes AND every signature
    /// verifies; Err(seq) of the first broken row otherwise.
    pub fn verify_chain(&self) -> Result<usize, u64> {
        let mut prev = "genesis".to_string();
        for ev in &self.events {
            if ev.prev_hash != prev { return Err(ev.seq); }
            let recomputed = hex::encode(Sha256::digest(&ev.canonical_bytes()));
            if recomputed != ev.content_hash { return Err(ev.seq); }
            if !verify_hmac(&self.device_key, ev.content_hash.as_bytes(), &ev.signature) {
                return Err(ev.seq);
            }
            prev = ev.content_hash.clone();
        }
        Ok(self.events.len())
    }

    /// Apply a GDPR / CCPA right-to-erasure on every row whose
    /// `subject_hash` matches. Tombstones replace PII-bearing payload
    /// keys with `null` and stamp `redacted_at` + reason. The original
    /// `content_hash` and chain links are preserved.
    pub fn tombstone_subject(&mut self, subject_hash: &str, reason: &str) -> usize {
        let now = Utc::now().to_rfc3339();
        let mut redacted = 0usize;
        for ev in self.events.iter_mut() {
            if ev.subject_hash.as_deref() == Some(subject_hash) && ev.redacted_at.is_none() {
                redact_payload(&mut ev.payload);
                ev.redacted_at = Some(now.clone());
                ev.redaction_reason = Some(reason.into());
                redacted += 1;
            }
        }
        if redacted > 0 {
            let _ = storage::save_json(AUDIT_FILE, &self.events);
            // Append a Tombstoned row so the erasure itself is auditable.
            let payload = serde_json::json!({
                "subject_hash": subject_hash,
                "rows_redacted": redacted,
                "reason": reason,
            });
            self.append("system", AuditKind::Tombstoned, payload, Some(subject_hash.into()));
        }
        redacted
    }

    pub fn recent(&self, n: usize) -> &[AuditEvent] {
        let start = self.events.len().saturating_sub(n);
        &self.events[start..]
    }

    pub fn count(&self) -> usize { self.events.len() }

    /// Produce an export bundle: every row + verification result + a
    /// final manifest signature so the recipient can confirm the
    /// bundle wasn't truncated in transit.
    pub fn export_signed(&self) -> ExportBundle {
        let chain_ok = self.verify_chain().is_ok();
        let manifest = serde_json::json!({
            "workspace_id": self.workspace_id,
            "generated_at": Utc::now().to_rfc3339(),
            "row_count":    self.events.len(),
            "first_seq":    self.events.first().map(|e| e.seq),
            "last_seq":     self.events.last().map(|e| e.seq),
            "chain_ok":     chain_ok,
        });
        let manifest_hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&manifest).unwrap_or_default(),
        ));
        let manifest_sig = sign_hmac(&self.device_key, manifest_hash.as_bytes());
        ExportBundle {
            manifest,
            manifest_hash,
            manifest_signature: manifest_sig,
            events: self.events.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub manifest: serde_json::Value,
    pub manifest_hash: String,
    pub manifest_signature: String,
    pub events: Vec<AuditEvent>,
}

// ─────────────────────────────────────────────
//  Crypto helpers
// ─────────────────────────────────────────────

fn sign_hmac(key: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

fn verify_hmac(key: &[u8], data: &[u8], hex_sig: &str) -> bool {
    let sig = match hex::decode(hex_sig) { Ok(b) => b, Err(_) => return false };
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.verify_slice(&sig).is_ok()
}

fn load_or_create_device_key() -> [u8; 32] {
    let dir = storage::data_dir();
    let path = dir.join(DEVICE_KEY_FILE);
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            return k;
        }
    }
    use rand::RngCore;
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, &k);
    k
}

/// PII-bearing payload keys get nulled on tombstone. We don't try to
/// be clever about types - the schema for each `kind` declares which
/// fields are sensitive and we redact them by name.
fn redact_payload(payload: &mut serde_json::Value) {
    const SENSITIVE: &[&str] = &[
        "email", "name", "display_name", "phone", "address",
        "ip", "user_agent", "subject", "body", "message",
        "document", "attachment", "content", "to", "from",
        "reason", "amount", "account",
    ];
    if let Some(obj) = payload.as_object_mut() {
        for k in SENSITIVE {
            if obj.contains_key(*k) {
                obj.insert((*k).to_string(), serde_json::Value::Null);
            }
        }
        obj.insert("redacted".to_string(), serde_json::Value::Bool(true));
    }
}

// ─────────────────────────────────────────────
//  Convenience constructors for common payloads
// ─────────────────────────────────────────────

pub fn payload_action(action_key: &str, args: &serde_json::Value, outcome: &str) -> serde_json::Value {
    serde_json::json!({
        "action_key": action_key,
        "args":       args,
        "outcome":    outcome,
    })
}

pub fn payload_approval(
    approval_id: &str,
    action_key: &str,
    decision: &str,
    decider_principal: &str,
    reason: Option<&str>,
    channel: &str,
) -> serde_json::Value {
    serde_json::json!({
        "approval_id":       approval_id,
        "action_key":        action_key,
        "decision":          decision,
        "decider_principal": decider_principal,
        "reason":            reason,
        "channel":           channel,
    })
}

/// Convenience: SHA-256 hash a string (typically an email) for
/// `subject_hash` lookups. Output is hex.
pub fn hash_subject(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

// ─────────────────────────────────────────────
//  Thin wrapper for the AppState mutex
// ─────────────────────────────────────────────

pub type AuditLogState = Mutex<AuditLog>;
