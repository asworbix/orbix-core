//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Compliance + Audit log
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn compliance_get_policy(state: State<'_, AppState>) -> Result<compliance::CompliancePolicy, String> {
    let policy = state.core.compliance.lock().map_err(|e| e.to_string())?.clone();
    Ok(policy)
}

#[tauri::command]
pub(crate) fn compliance_save_policy(
    policy: compliance::CompliancePolicy,
    state: State<'_, AppState>,
) -> Result<(), String> {
    policy.save()?;
    *state.core.compliance.lock().map_err(|e| e.to_string())? = policy.clone();
    let mut log = state.core.audit_log.lock().map_err(|e| e.to_string())?;
    log.append(
        "system",
        audit_log::AuditKind::PolicyChanged,
        serde_json::json!({
            "version":          policy.version,
            "compliance_mode":  policy.toggles.compliance_mode,
            "frameworks":       policy.frameworks,
        }),
        None,
    );
    Ok(())
}


#[tauri::command]
pub(crate) fn audit_log_recent(limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<audit_log::AuditEvent>, String> {
    let log = state.core.audit_log.lock().map_err(|e| e.to_string())?;
    Ok(log.recent(limit.unwrap_or(200)).to_vec())
}

#[tauri::command]
pub(crate) fn audit_log_count(state: State<'_, AppState>) -> Result<AuditSummary, String> {
    let log = state.core.audit_log.lock().map_err(|e| e.to_string())?;
    let chain_ok = log.verify_chain().is_ok();
    Ok(AuditSummary {
        count: log.count(),
        chain_ok,
        last_seq: log.events.last().map(|e| e.seq),
        first_timestamp: log.events.first().map(|e| e.timestamp.clone()),
        last_timestamp: log.events.last().map(|e| e.timestamp.clone()),
    })
}

#[tauri::command]
pub(crate) fn audit_log_verify(state: State<'_, AppState>) -> Result<AuditSummary, String> {
    let log = state.core.audit_log.lock().map_err(|e| e.to_string())?;
    let chain_ok = log.verify_chain().is_ok();
    Ok(AuditSummary {
        count: log.count(),
        chain_ok,
        last_seq: log.events.last().map(|e| e.seq),
        first_timestamp: log.events.first().map(|e| e.timestamp.clone()),
        last_timestamp: log.events.last().map(|e| e.timestamp.clone()),
    })
}

#[tauri::command]
pub(crate) fn audit_log_export_signed(state: State<'_, AppState>) -> Result<audit_log::ExportBundle, String> {
    let log = state.core.audit_log.lock().map_err(|e| e.to_string())?;
    Ok(log.export_signed())
}

#[tauri::command]
pub(crate) fn audit_log_tombstone_subject(
    subject_email_or_hash: String,
    reason: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let mut log = state.core.audit_log.lock().map_err(|e| e.to_string())?;
    // Accept either a raw email (which we hash) or an existing
    // subject_hash hex value.
    let hash = if subject_email_or_hash.contains('@') {
        audit_log::hash_subject(&subject_email_or_hash)
    } else {
        subject_email_or_hash.clone()
    };
    Ok(log.tombstone_subject(&hash, &reason))
}

