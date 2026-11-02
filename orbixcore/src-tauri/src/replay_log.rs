//! Local replay log: the full rendered request and raw model output of
//! every completed turn, one JSONL record per turn, under the profile
//! data dir. This is the full-prompt tap the adaptive-models research
//! gates on (`research/replay-eval/`): the conversation store keeps
//! message text, but replaying what the model really saw needs the
//! system prompt, live state, and schemas too. Records never leave the
//! machine. Files rotate daily; old days are pruned. Set
//! ORBIX_REPLAY_LOG=0 to disable.

use std::io::Write;
use std::path::{Path, PathBuf};

const KEEP_DAYS: i64 = 30;
// A record above this is replaced by a small marker so a runaway tool
// result can never grow the log without bound.
const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;

static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn log_dir() -> PathBuf {
    crate::storage::data_dir().join("replay_log")
}

/// Replace inline base64 data URIs (vision turns embed whole screenshots)
/// with a short placeholder: the log needs the shape of the turn, not
/// megabytes of pixels.
fn elide_data_uris(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Array(items) => items.iter_mut().for_each(elide_data_uris),
        serde_json::Value::Object(map) => {
            for val in map.values_mut() {
                if let Some(s) = val.as_str() {
                    if s.starts_with("data:") && s.len() > 256 {
                        *val = serde_json::Value::String(format!("[data uri elided: {} bytes]", s.len()));
                        continue;
                    }
                }
                elide_data_uris(val);
            }
        }
        _ => {}
    }
}

/// Append one completed model turn. Best-effort and silent on failure:
/// logging must never break a turn. `request` is the full request body
/// as sent (credentials travel in headers, never in bodies, so it is
/// safe to persist); `tool_calls` are native calls already reconstructed
/// as action objects.
pub fn log_turn(
    kind: &str,
    model: &str,
    request: &serde_json::Value,
    output: &str,
    tool_calls: &[serde_json::Value],
) {
    if std::env::var("ORBIX_REPLAY_LOG").ok().as_deref() == Some("0") {
        return;
    }
    let mut request = request.clone();
    elide_data_uris(&mut request);
    let ts = chrono::Utc::now();
    let record = serde_json::json!({
        "ts": ts.to_rfc3339(),
        "kind": kind,
        "model": model,
        "request": request,
        "output": output,
        "tool_calls": tool_calls,
    });
    let mut line = match serde_json::to_string(&record) {
        Ok(l) => l,
        Err(_) => return,
    };
    if line.len() > MAX_RECORD_BYTES {
        line = serde_json::json!({
            "ts": ts.to_rfc3339(),
            "kind": kind,
            "model": model,
            "oversize_bytes": line.len(),
        })
        .to_string();
    }

    let dir = log_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jsonl", ts.format("%Y-%m-%d")));
    let first_of_day = !path.exists();
    {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", line);
        }
    }
    if first_of_day {
        prune(&dir);
    }
}

/// Drop day files older than KEEP_DAYS. Lexicographic comparison works
/// because the names are YYYY-MM-DD.jsonl.
fn prune(dir: &Path) {
    let cutoff = format!(
        "{}.jsonl",
        (chrono::Utc::now() - chrono::Duration::days(KEEP_DAYS)).format("%Y-%m-%d")
    );
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".jsonl") && name < cutoff {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_uris_are_elided_everywhere() {
        let big = format!("data:image/png;base64,{}", "A".repeat(1000));
        let mut v = serde_json::json!({
            "messages": [
                { "role": "user", "content": [
                    { "type": "image_url", "image_url": { "url": big } },
                    { "type": "text", "text": "keep me" }
                ]}
            ]
        });
        elide_data_uris(&mut v);
        let url = v["messages"][0]["content"][0]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("[data uri elided:"));
        assert_eq!(v["messages"][0]["content"][1]["text"], "keep me");
    }

    #[test]
    fn short_data_strings_survive() {
        let mut v = serde_json::json!({ "note": "data:text/plain,hi" });
        elide_data_uris(&mut v);
        assert_eq!(v["note"], "data:text/plain,hi");
    }
}
