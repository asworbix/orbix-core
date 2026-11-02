use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Tool Definitions
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub parameters: Vec<ToolParam>,
    pub requires_approval: bool,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    FileSystem,
    Shell,
    Network,
    Utility,
    Custom,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub param_type: String,
    pub required: bool,
}

// ─────────────────────────────────────────────
//  Tool Calls & Results
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ToolCall {
    pub id: String,
    pub tool_name: String,
    pub arguments: HashMap<String, serde_json::Value>,
    pub status: ToolCallStatus,
    pub result: Option<ToolResult>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    Approved,
    Denied,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────
//  Tool Registry
// ─────────────────────────────────────────────

pub struct ToolRegistry {
    pub tools: Vec<ToolDef>,
    pub pending_calls: Vec<ToolCall>,
    pub auto_approve: HashMap<String, bool>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            tools: builtin_tools(),
            pending_calls: Vec::new(),
            auto_approve: HashMap::new(),
        }
    }
}

fn builtin_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            id: "file_read".into(),
            name: "file_read".into(),
            description: "Read the contents of a file at a given path".into(),
            category: ToolCategory::FileSystem,
            parameters: vec![ToolParam {
                name: "path".into(),
                description: "Absolute or relative file path to read".into(),
                param_type: "string".into(),
                required: true,
            }],
            requires_approval: true,
            enabled: true,
        },
        ToolDef {
            id: "file_write".into(),
            name: "file_write".into(),
            description: "Write content to a file, creating it if it doesn't exist".into(),
            category: ToolCategory::FileSystem,
            parameters: vec![
                ToolParam {
                    name: "path".into(),
                    description: "Absolute or relative file path to write".into(),
                    param_type: "string".into(),
                    required: true,
                },
                ToolParam {
                    name: "content".into(),
                    description: "Content to write to the file".into(),
                    param_type: "string".into(),
                    required: true,
                },
            ],
            requires_approval: true,
            enabled: true,
        },
        ToolDef {
            id: "file_edit".into(),
            name: "file_edit".into(),
            description: "Make a surgical edit: replace an exact unique snippet in a file (old_string -> new_string) instead of rewriting the whole file".into(),
            category: ToolCategory::FileSystem,
            parameters: vec![
                ToolParam { name: "path".into(), description: "File path to edit".into(), param_type: "string".into(), required: true },
                ToolParam { name: "old_string".into(), description: "Exact text to replace (must appear once)".into(), param_type: "string".into(), required: true },
                ToolParam { name: "new_string".into(), description: "Replacement text".into(), param_type: "string".into(), required: true },
            ],
            requires_approval: true,
            enabled: true,
        },
        ToolDef {
            id: "file_list".into(),
            name: "file_list".into(),
            description: "List files and directories at a given path".into(),
            category: ToolCategory::FileSystem,
            parameters: vec![ToolParam {
                name: "path".into(),
                description: "Directory path to list".into(),
                param_type: "string".into(),
                required: true,
            }],
            requires_approval: false,
            enabled: true,
        },
        ToolDef {
            id: "shell_exec".into(),
            name: "shell_exec".into(),
            description: "Execute a shell command and return its output".into(),
            category: ToolCategory::Shell,
            parameters: vec![ToolParam {
                name: "command".into(),
                description: "Shell command to execute".into(),
                param_type: "string".into(),
                required: true,
            }],
            requires_approval: true,
            enabled: true,
        },
        ToolDef {
            id: "web_fetch".into(),
            name: "web_fetch".into(),
            description: "Fetch content from a URL (GET request). Use for reading web pages, APIs, and online data sources.".into(),
            category: ToolCategory::Network,
            parameters: vec![
                ToolParam {
                    name: "url".into(),
                    description: "URL to fetch".into(),
                    param_type: "string".into(),
                    required: true,
                },
                ToolParam {
                    name: "method".into(),
                    description: "HTTP method (GET, POST). Defaults to GET".into(),
                    param_type: "string".into(),
                    required: false,
                },
            ],
            requires_approval: true,
            enabled: true,
        },
        ToolDef {
            id: "web_search".into(),
            name: "web_search".into(),
            description: "Search the web for information. Returns top results with titles, URLs, and snippets. Use for lead research, company intelligence, market data, and competitive analysis.".into(),
            category: ToolCategory::Network,
            parameters: vec![
                ToolParam {
                    name: "query".into(),
                    description: "Search query string".into(),
                    param_type: "string".into(),
                    required: true,
                },
                ToolParam {
                    name: "region".into(),
                    description: "Region code for localised results (e.g. 'dk-da' for Denmark, 'us-en' for US). Defaults to 'dk-da'.".into(),
                    param_type: "string".into(),
                    required: false,
                },
                ToolParam {
                    name: "max_results".into(),
                    description: "Maximum number of results to return (default 10, max 20)".into(),
                    param_type: "integer".into(),
                    required: false,
                },
            ],
            requires_approval: true,
            enabled: true,
        },
    ]
}

// ─────────────────────────────────────────────
//  Tool Execution
// ─────────────────────────────────────────────

/// Execution context threaded into every tool call (W4): the app state
/// carries the compliance policy and confidential-folder ACLs the
/// gateway and file tools enforce; agent_id identifies the caller in
/// access logs ("bob", "user", "workflow", or a real agent id).
pub struct ToolCtx<'a> {
    pub state: &'a crate::AppState,
    pub agent_id: &'a str,
}

/// True when `name` is a built-in registry tool executable via
/// `execute_tool`. Workflow Action steps use this to route generic
/// tool steps without shadowing legacy endpoint hints.
pub fn is_builtin_tool(name: &str) -> bool {
    matches!(name,
        "file_read" | "file_write" | "file_edit" | "file_list"
        | "shell_exec" | "web_fetch" | "web_search")
}

pub async fn execute_tool(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolCtx<'_>,
) -> ToolResult {
    match tool_name {
        "file_read" => exec_file_read(args, ctx).await,
        "file_write" => exec_file_write(args, ctx).await,
        "file_edit" => exec_file_edit(args, ctx).await,
        "file_list" => exec_file_list(args, ctx).await,
        "shell_exec" => exec_shell(args).await,
        "web_fetch" => exec_web_fetch(args, ctx).await,
        "web_search" => exec_web_search(args, ctx).await,
        _ => ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Unknown tool: {}", tool_name)),
        },
    }
}



/// W4: enforce confidential-folder ACLs (data_control) on a file path.
/// Returns Err(denial ToolResult) when access is refused; on success
/// returns the folder id when the path is confidential so the caller can
/// log the access after the operation.
fn guard_path(ctx: &ToolCtx<'_>, path: &str, write: bool) -> Result<Option<String>, ToolResult> {
    let dc = ctx.state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(reason) = dc.authorize(ctx.agent_id, path, write) {
        return Err(ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Access denied by data control: {}", reason)),
        });
    }
    Ok(dc.check_access(ctx.agent_id, path).map(|(f, _)| f.id.clone()))
}

fn log_confidential(ctx: &ToolCtx<'_>, folder_id: &Option<String>, action: &str, path: &str) {
    if let Some(fid) = folder_id {
        let mut dc = ctx.state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner());
        dc.log_access(fid, ctx.agent_id, ctx.agent_id, action, path);
    }
}

fn get_str_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Result<String, ToolResult> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Missing required argument: {}", key)),
        })
}

async fn exec_file_read(
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolCtx<'_>,
) -> ToolResult {
    let path = match get_str_arg(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let path = crate::turn_scope::resolve_path(&path);
    let folder = match guard_path(ctx, &path.to_string_lossy(), false) {
        Ok(f) => f,
        Err(denied) => return denied,
    };
    if !path.exists() {
        return ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("File not found: {}", path.display())),
        };
    }
    log_confidential(ctx, &folder, "read", &path.to_string_lossy());

    let cap = |content: String| -> String {
        if content.len() > 50_000 {
            // Cap on a char boundary so a multibyte glyph is never split.
            let mut end = 50_000;
            while end > 0 && !content.is_char_boundary(end) { end -= 1; }
            format!("{}...\n[truncated at 50KB]", &content[..end])
        } else {
            content
        }
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // PDFs are binary; extract their text so a dropped/attached PDF is
    // readable. Runs on a blocking thread - pdf_extract is synchronous.
    if ext == "pdf" {
        let p = path.clone();
        return match tokio::task::spawn_blocking(move || pdf_extract::extract_text(&p)).await {
            Ok(Ok(text)) if !text.trim().is_empty() => ToolResult { success: true, output: cap(text), error: None },
            Ok(Ok(_)) => ToolResult {
                success: false, output: String::new(),
                error: Some("The PDF has no extractable text (it may be a scanned image; ask Bob to look at it as an image instead).".to_string()),
            },
            Ok(Err(e)) => ToolResult { success: false, output: String::new(), error: Some(format!("Could not read the PDF: {}", e)) },
            Err(e) => ToolResult { success: false, output: String::new(), error: Some(format!("PDF read task failed: {}", e)) },
        };
    }

    // Spreadsheets are zip archives, unreadable as text; extract every
    // sheet as tab-separated rows so a dropped .xlsx is answerable.
    // Runs on a blocking thread - calamine is synchronous.
    if matches!(ext.as_str(), "xlsx" | "xlsm" | "xlsb" | "xls" | "ods") {
        let p = path.clone();
        return match tokio::task::spawn_blocking(move || spreadsheet_to_text(&p)).await {
            Ok(Ok(text)) if !text.trim().is_empty() => ToolResult { success: true, output: cap(text), error: None },
            Ok(Ok(_)) => ToolResult {
                success: false, output: String::new(),
                error: Some("The spreadsheet has no data in any sheet.".to_string()),
            },
            Ok(Err(e)) => ToolResult { success: false, output: String::new(), error: Some(format!("Could not read the spreadsheet: {}", e)) },
            Err(e) => ToolResult { success: false, output: String::new(), error: Some(format!("Spreadsheet read task failed: {}", e)) },
        };
    }

    // Images can't be read as text; the chat's vision path handles them.
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "heif" | "tiff") {
        return ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!(
                "\"{}\" is an image. Bob reads images with a vision model in chat, not with file_read. Ask about the image directly.",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
            )),
        };
    }

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => ToolResult { success: true, output: cap(content), error: None },
        Err(e) => ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!(
                "Failed to read file as text: {}. Readable formats: text/code, csv, pdf, and spreadsheets (xlsx, xls, xlsb, ods). Other binary formats are not supported yet.",
                e
            )),
        },
    }
}

/// Render every sheet of a workbook as tab-separated rows under a
/// `## Sheet: <name>` header. calamine handles xlsx/xlsm/xlsb/xls/ods;
/// the caller caps the output at 50KB like any other file_read.
fn spreadsheet_to_text(path: &std::path::Path) -> Result<String, String> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names = wb.sheet_names().to_vec();
    let mut out = String::new();
    for name in names {
        let range = match wb.worksheet_range(&name) {
            Ok(r) => r,
            Err(e) => {
                out.push_str(&format!("## Sheet: {} (unreadable: {})\n\n", name, e));
                continue;
            }
        };
        if range.is_empty() { continue; }
        out.push_str(&format!("## Sheet: {}\n", name));
        for row in range.rows() {
            let mut cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
            // Drop trailing empty cells so wide sparse sheets stay readable.
            while cells.last().map(|c| c.is_empty()).unwrap_or(false) { cells.pop(); }
            out.push_str(&cells.join("\t"));
            out.push('\n');
        }
        out.push('\n');
    }
    Ok(out)
}

async fn exec_file_write(
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolCtx<'_>,
) -> ToolResult {
    let path = match get_str_arg(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };
    let content = match get_str_arg(args, "content") {
        Ok(c) => c,
        Err(e) => return e,
    };

    let path = crate::turn_scope::resolve_path(&path);
    // W4 gap fix: writes to confidential folders must be authorized too
    // (only reads were guarded). write=true checks write permission.
    let folder = match guard_path(ctx, &path.to_string_lossy(), true) {
        Ok(f) => f,
        Err(denied) => return denied,
    };
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to create directory: {}", e)),
                };
            }
        }
    }

    // Atomic write so a crash mid-write never leaves a truncated file.
    match crate::storage::atomic_write(&path, content.as_bytes()) {
        Ok(()) => {
            log_confidential(ctx, &folder, "write", &path.to_string_lossy());
            ToolResult {
                success: true,
                output: format!("Written {} bytes to {}", content.len(), path.display()),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Failed to write file: {}", e)),
        },
    }
}

/// W6: surgical edit. Replaces the first exact occurrence of `old_string`
/// with `new_string`, refusing when the match is missing or ambiguous
/// (appears more than once) so an edit never lands in the wrong place.
/// Returns a small diff preview. Guarded + atomic like file_write.
async fn exec_file_edit(
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolCtx<'_>,
) -> ToolResult {
    let path = match get_str_arg(args, "path") { Ok(p) => p, Err(e) => return e };
    let old_string = match get_str_arg(args, "old_string") { Ok(s) => s, Err(e) => return e };
    let new_string = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let path = crate::turn_scope::resolve_path(&path);
    let folder = match guard_path(ctx, &path.to_string_lossy(), true) {
        Ok(f) => f,
        Err(denied) => return denied,
    };
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) => return ToolResult { success: false, output: String::new(), error: Some(format!("Failed to read file: {}", e)) },
    };
    let count = content.matches(&old_string).count();
    if count == 0 {
        return ToolResult { success: false, output: String::new(), error: Some("old_string not found in file".to_string()) };
    }
    if count > 1 {
        return ToolResult { success: false, output: String::new(), error: Some(format!("old_string appears {} times; include more context to make it unique", count)) };
    }
    let updated = content.replacen(&old_string, &new_string, 1);
    match crate::storage::atomic_write(&path, updated.as_bytes()) {
        Ok(()) => {
            log_confidential(ctx, &folder, "edit", &path.to_string_lossy());
            ToolResult {
                success: true,
                output: format!("Edited {}\n{}", path.display(), mini_diff(&old_string, &new_string)),
                error: None,
            }
        }
        Err(e) => ToolResult { success: false, output: String::new(), error: Some(format!("Failed to write file: {}", e)) },
    }
}

/// A compact -/+ preview of a replacement (first few lines each side).
fn mini_diff(old: &str, new: &str) -> String {
    let take = |s: &str| s.lines().take(6).map(|l| l.to_string()).collect::<Vec<_>>();
    let mut out = String::new();
    for l in take(old) { out.push_str(&format!("- {}\n", l)); }
    for l in take(new) { out.push_str(&format!("+ {}\n", l)); }
    out
}

async fn exec_file_list(
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolCtx<'_>,
) -> ToolResult {
    let path = match get_str_arg(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    let path = crate::turn_scope::resolve_path(&path);
    // W4 gap fix: listing a confidential folder is a read; authorize it.
    if let Err(denied) = guard_path(ctx, &path.to_string_lossy(), false) {
        return denied;
    }
    if !path.is_dir() {
        return ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Not a directory: {}", path.display())),
        };
    }

    match tokio::fs::read_dir(&path).await {
        Ok(mut entries) => {
            let mut items = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false);
                items.push(if is_dir {
                    format!("{}/", name)
                } else {
                    name
                });
            }
            items.sort();
            ToolResult {
                success: true,
                output: items.join("\n"),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Failed to list directory: {}", e)),
        },
    }
}

/// The directory a shell command runs in: an explicit, existing `cwd`
/// wins; then the workspace setting; then home. Never a missing folder.
pub(crate) fn shell_cwd(requested: Option<&str>) -> std::path::PathBuf {
    if let Some(dir) = requested.map(str::trim).filter(|d| !d.is_empty()) {
        let expanded = if let Some(rest) = dir.strip_prefix("~/") {
            dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| std::path::PathBuf::from(dir))
        } else if dir == "~" {
            dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(dir))
        } else {
            std::path::PathBuf::from(dir)
        };
        if expanded.is_dir() {
            return expanded;
        }
    }
    // A turn started from a terminal runs where that terminal is.
    if let Some(dir) = crate::turn_scope::workdir() {
        return dir;
    }
    if let Some(ws) = crate::workspace::WorkspaceSettings::load().usable_path() {
        return ws;
    }
    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
}

pub(crate) async fn exec_shell(args: &HashMap<String, serde_json::Value>) -> ToolResult {
    let command = match get_str_arg(args, "command") {
        Ok(c) => c,
        Err(e) => return e,
    };

    // Where the command runs: the call's own `cwd` when it names an
    // existing folder, else the user's workspace setting, else home. It
    // used to inherit the app process's directory (usually "/"), so a
    // bound repo's `git status` ran nowhere useful (field failure
    // 2026-09-06) and the model had to remember to `cd` every time.
    let cwd = shell_cwd(args.get("cwd").and_then(|v| v.as_str()));

    // /bin/sh on unix; cmd.exe on Windows (with the no-window helper so
    // console children never flash). The unix branch injects a full PATH
    // because macOS apps launched from Finder have a minimal one.
    #[cfg(windows)]
    let result = {
        let mut cmd = crate::process_ext::tokio_command("cmd.exe");
        cmd.arg("/C").arg(&command).current_dir(&cwd);
        cmd.output().await
    };
    #[cfg(not(windows))]
    let result = {
        let path_env = format!(
            "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:{}/.cargo/bin",
            dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_default()
        );
        tokio::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .current_dir(&cwd)
            .env("PATH", &path_env)
            .env("HOME", dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_default())
            .output()
            .await
    };
    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                stderr.clone()
            } else {
                format!("{}\n--- stderr ---\n{}", stdout, stderr)
            };
            let truncated = if combined.len() > 50_000 {
                format!("{}...\n[truncated at 50KB]", &combined[..50_000])
            } else {
                combined
            };
            ToolResult {
                success: output.status.success(),
                output: truncated,
                error: if output.status.success() {
                    None
                } else {
                    Some(format!("Exit code: {}", output.status.code().unwrap_or(-1)))
                },
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Failed to execute command: {}", e)),
        },
    }
}

async fn exec_web_fetch(
    args: &HashMap<String, serde_json::Value>,
    _ctx: &ToolCtx<'_>,
) -> ToolResult {
    let url = match get_str_arg(args, "url") {
        Ok(u) => u,
        Err(e) => return e,
    };

    // Validate URL scheme : only allow http/https
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return ToolResult {
            success: false,
            output: String::new(),
            error: Some("URL must start with http:// or https://".to_string()),
        };
    }

    // Use reqwest instead of shell curl to prevent command injection
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_default();

    match client.get(&url)
        .header("User-Agent", "OrbixCore/1.0")
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => {
                    let truncated = if body.len() > 1_048_576 {
                        format!("{}...\n[truncated at 1MB]", &body[..50_000])
                    } else {
                        body
                    };
                    ToolResult {
                        success: status.is_success(),
                        output: truncated,
                        error: if status.is_success() { None } else { Some(format!("HTTP {}", status)) },
                    }
                }
                Err(e) => ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read response: {}", e)),
                },
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!("Request failed: {}", e)),
        },
    }
}

/// Crude but effective readable-text extraction: drop scripts, styles
/// and tags, collapse whitespace, cap the length. Good enough for a
/// model to actually LEARN something from a page.
pub(crate) fn html_readable_text(html: &str, cap: usize) -> String {
    let no_scripts = regex::Regex::new(r"(?is)<(script|style|noscript|svg|nav|footer|header)[^>]*>.*?</\1>")
        .map(|re| re.replace_all(html, " ").to_string())
        .unwrap_or_else(|_| html.to_string());
    let no_tags = regex::Regex::new(r"<[^>]+>")
        .map(|re| re.replace_all(&no_scripts, " ").to_string())
        .unwrap_or(no_scripts);
    let decoded = no_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    let collapsed = regex::Regex::new(r"\s+")
        .map(|re| re.replace_all(&decoded, " ").trim().to_string())
        .unwrap_or(decoded);
    if collapsed.len() > cap {
        let mut end = cap;
        while !collapsed.is_char_boundary(end) { end -= 1; }
        format!("{}…", &collapsed[..end])
    } else {
        collapsed
    }
}

async fn exec_web_search(
    args: &HashMap<String, serde_json::Value>,
    ctx: &ToolCtx<'_>,
) -> ToolResult {
    let query = match get_str_arg(args, "query") {
        Ok(q) => q,
        Err(e) => return e,
    };
    let max_results: usize = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .min(20) as usize;

    // W4: all search egress goes through the gateway (compliance gate +
    // redaction + audit). The old path piped curl through a shell, which
    // was also a command-injection surface; this is plain HTTP.
    match crate::egress::web_search(ctx.state, &query, max_results).await {
        Ok(results) if results.is_empty() => ToolResult {
            success: false,
            output: String::new(),
            error: Some("No search results".to_string()),
        },
        Ok(results) => {
            let mut out = format!("Web search results for \"{}\":\n\n", query);
            for r in &results {
                out.push_str(&format!(
                    "- {} \n  {}\n  {}\n",
                    r["title"].as_str().unwrap_or(""),
                    r["url"].as_str().unwrap_or(""),
                    r["snippet"].as_str().unwrap_or(""),
                ));
            }
            // Depth, not just links: read top pages so the model works
            // from real content instead of 300-char snippets. Thin pages
            // (JS shells, listing/tag pages that render to nav junk) are
            // skipped and the next result is tried instead - field failure
            // 2026-08-01: two league-of-legends LISTING pages used both
            // read slots and yielded nothing, so the model hallucinated.
            //
            // Each read is capped at PAGE_READ_SECS: the fetch's own 30s
            // timeout let one slow site hold the whole answer. Reads stay
            // one by one on purpose: the first three usually succeed fast,
            // and fetching all six at once measured slower (3.4s vs 2.9s
            // per search, 2026-10-21) because it waits for the slowest.
            const PAGE_READ_SECS: u64 = 5;
            let mut pages_read = 0;
            for r in results.iter().take(6) {
                if pages_read >= 3 { break; }
                let url = r["url"].as_str().unwrap_or("");
                if url.is_empty() { continue; }
                let read = tokio::time::timeout(
                    std::time::Duration::from_secs(PAGE_READ_SECS),
                    crate::egress::web_fetch(ctx.state, url),
                )
                .await;
                if let Ok(Ok((status, body))) = read {
                    if status < 400 {
                        let text = html_readable_text(&body, 2000);
                        if text.len() > 400 {
                            pages_read += 1;
                            out.push_str(&format!(
                                "\n## Read from {} ({})\n{}\n",
                                r["title"].as_str().unwrap_or("page"), url, text
                            ));
                        }
                    }
                }
            }
            ToolResult { success: true, output: out, error: None }
        }
        Err(e) => ToolResult {
            success: false,
            output: String::new(),
            // Steer the degraded path: search rate limits don't affect
            // direct fetches, and the model usually knows the authoritative
            // domain for a topic (official news page, docs, wiki). One
            // fetch of the right page beats waiting out the cooldown.
            error: Some(format!(
                "{}\nweb_search is temporarily limited, but web_fetch still works: \
                 if you know a likely authoritative page for this topic (the official \
                 site's news or updates page), call web_fetch on that URL directly and \
                 answer from what it returns.",
                e
            )),
        },
    }
}

// ─────────────────────────────────────────────
//  Registry Helpers
// ─────────────────────────────────────────────

impl ToolRegistry {
    pub fn create_tool_call(
        &mut self,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<ToolCall, String> {
        let tool = self.tools.iter().find(|t| t.name == tool_name);
        let tool = match tool {
            Some(t) => t,
            None => return Err(format!("Tool '{}' not found", tool_name)),
        };

        if !tool.enabled {
            return Err(format!("Tool '{}' is disabled", tool_name));
        }

        let auto = self.auto_approve.get(tool_name).copied().unwrap_or(false);
        let status = if !tool.requires_approval || auto {
            ToolCallStatus::Approved
        } else {
            ToolCallStatus::Pending
        };

        let call = ToolCall {
            id: Uuid::new_v4().to_string(),
            tool_name: tool_name.to_string(),
            arguments,
            status,
            result: None,
            created_at: Utc::now().to_rfc3339(),
            resolved_at: None,
        };

        if call.status == ToolCallStatus::Pending {
            self.pending_calls.push(call.clone());
        }

        Ok(call)
    }

    pub fn approve_call(&mut self, call_id: &str) -> Option<ToolCall> {
        if let Some(call) = self.pending_calls.iter_mut().find(|c| c.id == call_id) {
            call.status = ToolCallStatus::Approved;
            call.resolved_at = Some(Utc::now().to_rfc3339());
            let result = call.clone();
            self.pending_calls.retain(|c| c.id != call_id);
            Some(result)
        } else {
            None
        }
    }

    pub fn deny_call(&mut self, call_id: &str) -> Option<ToolCall> {
        if let Some(call) = self.pending_calls.iter_mut().find(|c| c.id == call_id) {
            call.status = ToolCallStatus::Denied;
            call.resolved_at = Some(Utc::now().to_rfc3339());
            call.result = Some(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Tool call denied by user".into()),
            });
            let result = call.clone();
            self.pending_calls.retain(|c| c.id != call_id);
            Some(result)
        } else {
            None
        }
    }

    pub fn set_auto_approve(&mut self, tool_name: &str, auto: bool) {
        self.auto_approve.insert(tool_name.to_string(), auto);
    }

    pub fn toggle_tool(&mut self, tool_id: &str) -> Option<ToolDef> {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.id == tool_id) {
            tool.enabled = !tool.enabled;
            Some(tool.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod edit_tests {
    // exec_file_edit needs a ToolCtx/AppState; the substantive logic (match
    // count, replacement, atomicity) is exercised here at the string layer
    // plus a real filesystem roundtrip using the same primitives.
    use std::collections::HashMap;

    #[test]
    fn replacen_replaces_only_first_and_diff_shape() {
        let content = "line one\ntarget\nline three\n";
        assert_eq!(content.matches("target").count(), 1);
        let updated = content.replacen("target", "REPLACED", 1);
        assert!(updated.contains("REPLACED"));
        assert!(!updated.contains("target"));
        let d = super::mini_diff("target", "REPLACED");
        assert!(d.contains("- target") && d.contains("+ REPLACED"));
    }

    #[test]
    fn ambiguous_match_is_detected() {
        let content = "x\nx\n";
        assert_eq!(content.matches("x").count(), 2, "ambiguous edits must be refused");
    }

    #[tokio::test]
    async fn file_edit_roundtrip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "alpha\nbeta\ngamma\n").unwrap();
        // Apply the same replacen + atomic_write the tool uses.
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches("beta").count(), 1);
        let updated = content.replacen("beta", "BETA", 1);
        crate::storage::atomic_write(&path, updated.as_bytes()).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "alpha\nBETA\ngamma\n");
        let _ = HashMap::<String, serde_json::Value>::new();
    }
}

#[cfg(test)]
mod shell_cwd_tests {
    use super::shell_cwd;

    #[test]
    fn explicit_existing_dir_wins_and_tilde_expands() {
        // A directory guaranteed to exist on the platform running the tests.
        let existing = std::env::temp_dir();
        assert_eq!(shell_cwd(Some(&existing.to_string_lossy())), existing);
        let home = dirs::home_dir().unwrap();
        assert_eq!(shell_cwd(Some("~")), home);
        assert!(shell_cwd(Some("~/")).starts_with(&home));
    }

    #[test]
    fn missing_dir_falls_back_to_something_that_exists() {
        let d = shell_cwd(Some("/definitely/not/here"));
        assert!(d.is_dir(), "{}", d.display());
        assert!(shell_cwd(None).is_dir());
    }
}
