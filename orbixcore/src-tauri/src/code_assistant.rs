use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ─────────────────────────────────────────────
//  Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodeProject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub language: String,
    pub files: usize,
    pub lines: usize,
    pub scanned_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodeFile {
    pub path: String,
    pub relative_path: String,
    pub language: String,
    pub lines: usize,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodeAgent {
    pub id: String,
    pub name: String,
    pub role: String, // "reviewer", "optimizer", "architect", "debugger", "documenter"
    pub status: String, // "idle", "working", "done", "error"
    pub focus: Option<String>, // file or area being analyzed
    pub findings: Vec<CodeFinding>,
    pub created_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodeFinding {
    pub id: String,
    pub agent_id: String,
    pub severity: String, // "info", "suggestion", "warning", "critical"
    pub category: String, // "quality", "performance", "security", "architecture", "style"
    pub title: String,
    pub description: String,
    pub file_path: Option<String>,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub suggestion: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodeChat {
    pub id: String,
    pub role: String,
    pub content: String,
    pub agent_id: Option<String>,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AnalysisRequest {
    pub project_path: String,
    pub focus_files: Option<Vec<String>>,
    pub analysis_type: String, // "review", "optimize", "security", "architecture", "full"
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AnalysisResult {
    pub id: String,
    pub project_path: String,
    pub analysis_type: String,
    pub agents_spawned: Vec<String>,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct CodeAssistantStore {
    pub projects: Vec<CodeProject>,
    pub agents: Vec<CodeAgent>,
    pub findings: Vec<CodeFinding>,
    pub chats: Vec<CodeChat>,
    pub analyses: Vec<AnalysisResult>,
}

impl Default for CodeAssistantStore {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            agents: Vec::new(),
            findings: Vec::new(),
            chats: Vec::new(),
            analyses: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────
//  File scanning utilities
// ─────────────────────────────────────────────

fn detect_language(ext: &str) -> &str {
    match ext {
        "rs" => "Rust",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" => "JavaScript",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kt" => "Kotlin",
        "swift" => "Swift",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" => "CSS",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "md" => "Markdown",
        "sql" => "SQL",
        "sh" | "bash" | "zsh" => "Shell",
        "dockerfile" => "Dockerfile",
        _ => "Other",
    }
}

fn is_code_file(ext: &str) -> bool {
    matches!(ext,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "kt" | "swift" |
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "cs" | "rb" | "php" |
        "html" | "htm" | "css" | "scss" | "sass" |
        "json" | "yaml" | "yml" | "toml" | "md" | "sql" |
        "sh" | "bash" | "zsh" | "dockerfile"
    )
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name,
        "node_modules" | ".git" | "target" | "dist" | "build" | ".next" |
        ".cache" | "__pycache__" | ".venv" | "venv" | ".idea" | ".vscode" |
        ".DS_Store" | "vendor" | "Pods" | ".gradle" | "bin" | "obj"
    )
}

pub fn scan_directory(path: &str) -> Result<(Vec<CodeFile>, String), String> {
    let root = PathBuf::from(path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    let mut files = Vec::new();
    let mut lang_counts: HashMap<String, usize> = HashMap::new();
    scan_recursive(&root, &root, &mut files, &mut lang_counts)?;

    // Determine primary language
    let primary_lang = lang_counts.iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    Ok((files, primary_lang))
}

fn scan_recursive(
    root: &PathBuf,
    dir: &PathBuf,
    files: &mut Vec<CodeFile>,
    lang_counts: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if !should_skip_dir(&name) {
                scan_recursive(root, &path, files, lang_counts)?;
            }
            continue;
        }

        let ext = path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if !is_code_file(&ext) {
            continue;
        }

        let lang = detect_language(&ext).to_string();
        *lang_counts.entry(lang.clone()).or_insert(0) += 1;

        let metadata = std::fs::metadata(&path).ok();
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

        let lines = std::fs::read_to_string(&path)
            .map(|c| c.lines().count())
            .unwrap_or(0);

        let relative = path.strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());

        files.push(CodeFile {
            path: path.to_string_lossy().to_string(),
            relative_path: relative,
            language: lang,
            lines,
            size_bytes: size,
        });
    }

    Ok(())
}

// ─────────────────────────────────────────────
//  Agent role definitions (for auto-spawning)
// ─────────────────────────────────────────────

pub fn default_agents_for_analysis(analysis_type: &str) -> Vec<(&str, &str)> {
    match analysis_type {
        "review" => vec![
            ("Code Reviewer", "reviewer"),
            ("Style Checker", "style"),
        ],
        "optimize" => vec![
            ("Performance Analyst", "optimizer"),
            ("Memory Profiler", "optimizer"),
        ],
        "security" => vec![
            ("Security Scanner", "security"),
            ("Dependency Auditor", "security"),
        ],
        "architecture" => vec![
            ("Architecture Analyst", "architect"),
            ("Dependency Mapper", "architect"),
        ],
        "full" | _ => vec![
            ("Code Reviewer", "reviewer"),
            ("Security Scanner", "security"),
        ],
    }
}

pub fn build_analysis_prompt(role: &str, file_content: &str, file_path: &str) -> String {
    let role_instruction = match role {
        "reviewer" => "You are a senior code reviewer. Analyze this code for:\n\
            - Logic errors or bugs\n\
            - Code quality issues\n\
            - Missing error handling\n\
            - Dead code or unused variables\n\
            Provide specific, actionable findings.",
        "optimizer" => "You are a performance optimization expert. Analyze this code for:\n\
            - Unnecessary allocations or copies\n\
            - Algorithm complexity improvements\n\
            - Caching opportunities\n\
            - Hot path optimizations\n\
            Provide specific optimization suggestions with expected impact.",
        "security" => "You are a security auditor. Analyze this code for:\n\
            - Injection vulnerabilities (SQL, XSS, command injection)\n\
            - Authentication/authorization issues\n\
            - Sensitive data exposure\n\
            - Insecure defaults or configurations\n\
            Rate each finding by severity: critical, warning, info.",
        "architect" => "You are a software architect. Analyze this code for:\n\
            - SOLID principle violations\n\
            - Coupling and cohesion issues\n\
            - Missing abstractions or over-engineering\n\
            - Scalability concerns\n\
            Provide architectural improvement suggestions.",
        "style" => "You are a code style expert. Analyze this code for:\n\
            - Naming convention issues\n\
            - Formatting inconsistencies\n\
            - Missing documentation for public APIs\n\
            - Idiomatic patterns for this language\n\
            Suggest improvements that make the code more readable.",
        _ => "Analyze this code and provide findings.",
    };

    format!(
        "{}\n\nFile: {}\n\n```\n{}\n```\n\n\
        Respond in this JSON format:\n\
        {{\"findings\": [\n\
          {{\"severity\": \"info|suggestion|warning|critical\",\n\
           \"category\": \"quality|performance|security|architecture|style\",\n\
           \"title\": \"Short title\",\n\
           \"description\": \"Explanation\",\n\
           \"line_start\": null,\n\
           \"line_end\": null,\n\
           \"suggestion\": \"Code fix or approach\"}}\n\
        ]}}\n\
        Return ONLY the JSON - no markdown, no commentary.",
        role_instruction, file_path,
        &crate::truncate_str(file_content, 24_000)
    )
}

pub fn build_chat_prompt(question: &str, context_files: &[(String, String)], history: &[CodeChat]) -> String {
    let mut prompt = String::new();

    // Include only the first context file, heavily truncated
    if let Some((path, content)) = context_files.first() {
        // W6: full(er)-context analysis. 16k chars fits comfortably in the
        // local model's context window; char-safe truncation avoids a panic
        // on a multi-byte boundary.
        let truncated = crate::truncate_str(content, 16_000);
        prompt.push_str(&format!("File: {}\n```\n{}\n```\n\n", path, truncated));
    }

    // Only last 4 history messages for context
    if history.len() > 1 {
        for msg in history.iter().rev().take(4).rev() {
            let content_trunc = if msg.content.len() > 300 {
                format!("{}...", &msg.content[..300])
            } else {
                msg.content.clone()
            };
            prompt.push_str(&format!("{}: {}\n", msg.role, content_trunc));
        }
        prompt.push('\n');
    }

    prompt.push_str(question);
    prompt
}
