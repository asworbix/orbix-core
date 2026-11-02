//! Steps only the user can take, kept visible until they are done.
//!
//! A pairing can leave one human step behind: restart Claude so it sees
//! Bob's tools, allow Accessibility, open a window so Bob can look. A
//! toast is the wrong home for that (it is gone in four seconds), so
//! every pending step is one `UserAction` here. The chat surface shows
//! them as a persistent bar with a button, Bob sees them in his prompt
//! and can run one when the user says "go ahead", and the list empties
//! itself as sources resolve. Peers are the first source; permissions
//! and connector re-auth can join without touching the UI.

use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct UserAction {
    /// Stable id: "<source>:<subject id>".
    pub id: String,
    pub source: String,
    /// What it is about, in the user's words: "Claude".
    pub subject: String,
    /// Button text: "Restart Claude".
    pub label: String,
    /// Why, in one sentence.
    pub reason: String,
    /// Machine action the source understands.
    pub action: String,
}

pub fn list() -> Vec<UserAction> {
    let mut out = Vec::new();
    for p in crate::peer::list() {
        let Some(fu) = p.follow_up.clone() else { continue };
        let reason = match fu.action.as_str() {
            a if a.starts_with("connect_service:") => {
                let slug = a.trim_start_matches("connect_service:");
                let name = crate::services::info(slug).map(|i| i.name).unwrap_or(slug);
                format!("{} lets Bob in through its {} API once you sign in. Then he works with the real data, not the window.", p.app, name)
            }
            "restart_app" => format!("{} needs a restart before it can call Bob.", p.app),
            "open_accessibility_settings" => "macOS must allow Orbix Core to see other apps' windows (Accessibility) before Bob can type into or read them.".to_string(),
            "refresh" => format!("{} had no window open when it was paired. Open one, then check again.", p.app),
            "devtools_relaunch" => format!("{} is a Chromium app. Relaunched with DevTools, Bob reads it and types into it reliably instead of guessing from the window. It quits and reopens the app.", p.app),
            _ => p.bob_line.clone(),
        };
        out.push(UserAction {
            id: format!("peer:{}", p.id),
            source: "peer".to_string(),
            subject: p.app.clone(),
            label: fu.label,
            reason,
            action: fu.action,
        });
    }
    out
}

/// Find by id, or by subject name when Bob resolves "restart it" from
/// context. Returns the resolved action's id and subject.
pub fn find(id_or_subject: &str) -> Option<UserAction> {
    let needle = id_or_subject.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let all = list();
    all.iter()
        .find(|a| a.id.to_lowercase() == needle)
        .or_else(|| all.iter().find(|a| a.subject.to_lowercase() == needle))
        .or_else(|| all.iter().find(|a| a.subject.to_lowercase().contains(&needle)))
        .or_else(|| if all.len() == 1 { all.first() } else { None })
        .cloned()
}

/// Run one pending step. Sources do the work; this only routes.
pub async fn run(id_or_subject: &str, app: &tauri::AppHandle) -> Result<String, String> {
    let action = find(id_or_subject).ok_or_else(|| {
        let pending = list();
        if pending.is_empty() {
            "nothing is waiting on the user right now".to_string()
        } else {
            format!(
                "no pending step matches \"{}\". Pending: {}",
                id_or_subject,
                pending.iter().map(|a| format!("{} ({})", a.subject, a.label)).collect::<Vec<_>>().join(", ")
            )
        }
    })?;
    let result = match action.source.as_str() {
        "peer" => {
            let peer = if action.action == "refresh" {
                crate::peer::refresh(&action.subject, Some(app)).await?
            } else {
                crate::peer::follow_up(&action.subject, Some(app)).await?
            };
            let done = peer.follow_up.is_none();
            crate::peer::emit_event(app, "updated", &format!("{} is ready.", peer.app), Some(&peer));
            if done {
                format!("Done: {}. {}", action.label, peer.bob_line)
            } else {
                format!("{} still needs: {}. {}", peer.app, peer.follow_up.as_ref().map(|f| f.label.clone()).unwrap_or_default(), peer.bob_line)
            }
        }
        other => return Err(format!("unknown action source \"{}\"", other)),
    };
    emit_changed(app);
    Ok(result)
}

pub fn emit_changed(app: &tauri::AppHandle) {
    let _ = crate::event_bus::emit_ui(app, "data-changed", "user-actions");
    let _ = crate::event_bus::emit_ui(app, "user-actions", json!({ "pending": list() }));
}

/// One block for Bob's prompt so "restart it" resolves without a lookup.
pub fn prompt_digest() -> String {
    let pending = list();
    if pending.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nWAITING ON THE USER (run_user_action does the step when the user agrees; offer it, never assume):\n");
    for a in pending {
        out.push_str(&format!("- {}: {} Button: \"{}\" (id {}).\n", a.subject, a.reason, a.label, a.id));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_gives_empty_digest_and_no_match() {
        // Test data dir is empty: no peers, so nothing is pending.
        assert!(list().is_empty());
        assert!(prompt_digest().is_empty());
        assert!(find("Claude").is_none());
    }
}
