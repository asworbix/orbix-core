//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Conversation History (Epic D)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_conversations() -> Result<Vec<storage::ConversationSummary>, String> {
    storage::list_conversations()
}

#[tauri::command]
pub(crate) async fn load_conversation(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Message>, String> {
    let messages = storage::load_conversation(&id)?;
    let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
    *msgs = messages.clone();
    let mut conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner());
    *conv_id = id;
    Ok(messages)
}

#[tauri::command]
pub(crate) async fn delete_conversation(id: String) -> Result<(), String> {
    storage::delete_conversation(&id)
}

#[tauri::command]
pub(crate) async fn rename_conversation(
    id: String,
    title: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Title cannot be empty".to_string());
    }
    storage::rename_conversation(&id, trimmed)?;
    crate::event_bus::emit_ui(&app, "data-changed", "conversation-renamed").ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn save_current_conversation(state: State<'_, AppState>) -> Result<(), String> {
    let msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
    if msgs.is_empty() {
        return Ok(());
    }
    let conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let title = msgs
        .first()
        .map(|m| {
            let t = m.content.chars().take(50).collect::<String>();
            if m.content.len() > 50 { format!("{}...", t) } else { t }
        })
        .unwrap_or_else(|| "Untitled".to_string());
    storage::save_conversation(&conv_id, &title, &msgs)
}


// ─────────────────────────────────────────────
//  Commands - Export Conversation to PDF
// ─────────────────────────────────────────────

/// Mask sensitive data: API keys, tokens, UUIDs, emails, IPs, secrets
pub(crate) fn mask_sensitive(text: &str) -> String {
    use regex::Regex;
    let mut out = text.to_string();

    // API keys (sk-..., key-..., bearer tokens)
    let api_key = Regex::new(r"(?i)(sk-|key-|api[_-]?key[=:\s]+)[a-zA-Z0-9\-_]{8,}").unwrap();
    out = api_key.replace_all(&out, "${1}[MASKED]").to_string();

    // Bearer tokens
    let bearer = Regex::new(r"(?i)(bearer\s+)[a-zA-Z0-9\-_.]{8,}").unwrap();
    out = bearer.replace_all(&out, "${1}[MASKED]").to_string();

    // UUIDs
    let uuid = Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}").unwrap();
    out = uuid.replace_all(&out, "[MASKED-ID]").to_string();

    // Email addresses
    let email = Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap();
    out = email.replace_all(&out, "[MASKED-EMAIL]").to_string();

    // IP addresses (v4)
    let ip = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
    out = ip.replace_all(&out, "[MASKED-IP]").to_string();

    // Client IDs / secrets (hex strings 20+ chars)
    let hex_secret = Regex::new(r"(?i)(client[_-]?(id|secret)[=:\s]+)[a-zA-Z0-9]{20,}").unwrap();
    out = hex_secret.replace_all(&out, "${1}[MASKED]").to_string();

    // Passwords
    let pw = Regex::new(r#"(?i)(password|passwd|secret)[=:\s"]+[^\s"]{4,}"#).unwrap();
    out = pw.replace_all(&out, "${1}=[MASKED]").to_string();

    out
}

/// Generate HTML for a conversation with masked sensitive data
pub(crate) fn conversation_to_html(messages: &[Message], title: &str) -> String {
    let mut html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>{title} - OrbixCore</title>
<style>
  @page {{ size: A4; margin: 18mm; }}
  body {{ font-family: -apple-system, 'Helvetica Neue', sans-serif; font-size: 12px; line-height: 1.6; color: #1a1a2e; max-width: 720px; margin: 0 auto; padding: 24px; }}
  .header {{ border-bottom: 2px solid #c864b4; padding-bottom: 12px; margin-bottom: 24px; }}
  .header h1 {{ font-size: 20px; margin: 0 0 4px 0; color: #0f0f23; }}
  .header .meta {{ font-size: 11px; color: #888; }}
  .msg {{ margin-bottom: 16px; page-break-inside: avoid; }}
  .msg-user {{ background: #f0ecf8; border-radius: 10px; padding: 10px 14px; margin-left: 40px; }}
  .msg-assistant {{ background: #f5f5f5; border-radius: 10px; padding: 10px 14px; margin-right: 40px; border-left: 3px solid #c864b4; }}
  .msg-role {{ font-size: 10px; font-weight: 700; text-transform: uppercase; color: #888; margin-bottom: 4px; letter-spacing: 0.5px; }}
  .msg-role.user {{ color: #6a4fb6; }}
  .msg-role.assistant {{ color: #c864b4; }}
  .msg-time {{ font-size: 9px; color: #aaa; margin-top: 4px; text-align: right; }}
  .msg-content {{ white-space: pre-wrap; word-wrap: break-word; }}
  .msg-content code {{ background: #e8e0f0; padding: 1px 4px; border-radius: 3px; font-size: 11px; }}
  .msg-content pre {{ background: #1a1a2e; color: #e0d0f0; padding: 10px; border-radius: 6px; font-size: 10px; overflow-x: auto; }}
  .masked {{ background: #ffe8e8; color: #a00; padding: 1px 4px; border-radius: 3px; font-size: 10px; }}
  .footer {{ margin-top: 32px; border-top: 1px solid #e0e0e0; padding-top: 12px; font-size: 10px; color: #aaa; text-align: center; }}
</style>
</head>
<body>
<div class="header">
  <h1>{title}</h1>
  <div class="meta">{count} messages &middot; Exported {now} &middot; Sensitive data masked automatically</div>
</div>
"#,
        title = mask_sensitive(title),
        count = messages.len(),
        now = Utc::now().format("%Y-%m-%d %H:%M UTC"),
    );

    for msg in messages {
        let role_class = if msg.role == "user" { "user" } else { "assistant" };
        let msg_class = if msg.role == "user" { "msg-user" } else { "msg-assistant" };
        let role_label = if msg.role == "user" { "You" } else { "Bob" };
        let content = mask_sensitive(&msg.content);
        // Basic markdown: bold, code blocks, inline code
        let content = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");

        let time = if msg.timestamp.len() > 16 { &msg.timestamp[..16] } else { &msg.timestamp };

        html.push_str(&format!(
            r#"<div class="msg">
  <div class="{msg_class}">
    <div class="msg-role {role_class}">{role_label}</div>
    <div class="msg-content">{content}</div>
    <div class="msg-time">{time}</div>
  </div>
</div>
"#
        ));
    }

    html.push_str(r#"<div class="footer">Exported from OrbixCore &middot; Sensitive data masked automatically</div>
</body></html>"#);

    html
}

#[tauri::command]
pub(crate) async fn export_conversation(
    id: Option<String>,
    state: State<'_, AppState>,
    _app: tauri::AppHandle,
) -> Result<String, String> {
    // Get messages: either from a saved conversation or the current one
    let (messages, title) = if let Some(ref conv_id) = id {
        let msgs = storage::load_conversation(conv_id)?;
        let title = msgs.first()
            .map(|m| { let t = m.content.chars().take(50).collect::<String>(); if m.content.len() > 50 { format!("{}...", t) } else { t } })
            .unwrap_or_else(|| "Conversation".to_string());
        (msgs, title)
    } else {
        let msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let title = msgs.first()
            .map(|m| { let t = m.content.chars().take(50).collect::<String>(); if m.content.len() > 50 { format!("{}...", t) } else { t } })
            .unwrap_or_else(|| "Conversation".to_string());
        (msgs, title)
    };

    if messages.is_empty() {
        return Err("No messages to export".to_string());
    }

    let html = conversation_to_html(&messages, &title);

    // Save to temp file and open save dialog
    let safe_title: String = title.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .take(40)
        .collect();
    let filename = format!("OrbixCore_{}.html", safe_title.trim().replace(' ', "_"));

    // Write to Desktop by default
    let desktop = dirs::desktop_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = desktop.join(&filename);
    std::fs::write(&path, &html).map_err(|e| format!("Failed to write: {}", e))?;

    // Try to convert to PDF via Chrome headless (if available)
    let pdf_path = path.with_extension("pdf");
    let chrome_path = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    if std::path::Path::new(chrome_path).exists() {
        let output = std::process::Command::new(chrome_path)
            .arg("--headless")
            .arg("--disable-gpu")
            .arg(format!("--print-to-pdf={}", pdf_path.display()))
            .arg("--no-margins")
            .arg(format!("file://{}", path.display()))
            .output();

        if let Ok(o) = output {
            if o.status.success() && pdf_path.exists() {
                // Remove the HTML intermediate
                std::fs::remove_file(&path).ok();
                // Open the PDF
                std::process::Command::new("open").arg(&pdf_path).spawn().ok();
                return Ok(pdf_path.display().to_string());
            }
        }
    }

    // Fallback: open HTML in browser (user can Cmd+P to print as PDF)
    std::process::Command::new("open").arg(&path).spawn().ok();
    Ok(path.display().to_string())
}

