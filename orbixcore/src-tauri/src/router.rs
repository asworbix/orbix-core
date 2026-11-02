//! Deterministic command router - the spine of Orbix Core.
//!
//! ## Why this exists
//!
//! Orbix Core is a platform that runs autonomously once agents are set up,
//! controlled primarily from the iPhone. In that world, most user messages
//! are simple commands: "list my agents", "run my invoice workflow", "what's
//! happening", "forget my LinkedIn token". Each of those is a pure function
//! call. Routing them through a 7-27B parameter LLM is absurd - it burns
//! RAM, takes seconds, and adds no value.
//!
//! This router pattern-matches the top ~25 most common commands and calls
//! the existing Rust handlers directly. If nothing matches, it returns None
//! and the caller falls through to the LLM path. ~70% of real user traffic
//! should never touch the LLM.
//!
//! Every surface - Mac text, Mac voice, iPhone companion, cron/trigger
//! narration - goes through this same router. One brain, many mouths.

use crate::AppState;
use regex::Regex;
use std::sync::OnceLock;

/// Result of a routed command. When `display` is populated the caller
/// should skip the LLM entirely and show/speak this result directly.
pub struct RouterResult {
    /// Full markdown result for the chat UI.
    pub display: String,
    /// Shorter, cleaner text for voice TTS / phone notification.
    pub spoken: String,
    /// Progress label to show in the Bob step pill while routing.
    pub step_label: String,
}

impl RouterResult {
    fn simple(display: impl Into<String>, step: impl Into<String>) -> Self {
        let d: String = display.into();
        Self {
            spoken: strip_markdown_for_voice(&d),
            display: d,
            step_label: step.into(),
        }
    }
}

/// Strip basic markdown so TTS doesn't phonemize asterisks / hashes / pipes.
fn strip_markdown_for_voice(md: &str) -> String {
    let mut s = md.to_string();
    // Remove markdown headers
    s = Regex::new(r"(?m)^#{1,6}\s*").unwrap().replace_all(&s, "").to_string();
    // Remove bold/italic markers
    s = s.replace("**", "").replace("__", "").replace("*", "").replace("_", "");
    // Remove backticks
    s = s.replace("`", "");
    // Collapse bullet points to "- " (leave as-is for reading flow)
    // Collapse whitespace
    s = Regex::new(r"\n{2,}").unwrap().replace_all(&s, ". ").to_string();
    s = Regex::new(r"\s+").unwrap().replace_all(&s, " ").trim().to_string();
    // Truncate to something reasonable for voice
    if s.len() > 420 {
        s = format!("{}…", &s[..420]);
    }
    s
}

/// Public entry point. Returns `Some(RouterResult)` if this command can be
/// handled deterministically, or `None` if it needs the LLM.
/// "open the monitor", "show me the log": the Run monitor window, not a
/// model call. Short, imperative, and naming the window or the log.
pub fn is_monitor_request(input: &str) -> bool {
    let t = input.trim().to_lowercase();
    let t = t.trim_end_matches(['.', '!', '?']);
    if t.chars().count() > 60 {
        return false;
    }
    let verb = ["open", "show", "pop", "bring up", "display"].iter().any(|v| t.starts_with(v));
    let names = t.contains("monitor") || t.contains("run log") || t.ends_with(" log") || t.ends_with(" logs");
    verb && names
}

pub fn try_route(input: &str, state: &AppState) -> Option<RouterResult> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();

    // ── Pure chitchat / greetings (instant, no state) ────────────────
    if is_greeting(&lower) {
        return Some(RouterResult::simple(
            "Hey. What do you need?",
            "Greeting",
        ));
    }
    if is_thanks(&lower) {
        return Some(RouterResult::simple("Anytime.", "Acknowledging"));
    }
    if is_goodbye(&lower) {
        return Some(RouterResult::simple("Catch you later.", "Goodbye"));
    }

    // ── Status / state queries ────────────────────────────────────────
    if matches_any(&lower, &[
        "what are you doing", "what's happening", "whats happening",
        "status", "status report", "what's going on", "whats going on",
        "orbix status", "system status",
    ]) {
        return Some(status_report(state));
    }

    // ── List commands ─────────────────────────────────────────────────
    if let Some(kind) = match_list(&lower) {
        return Some(list_kind(kind, state));
    }

    // ── Run workflow / chain ──────────────────────────────────────────
    if let Some((kind, name)) = match_run(&lower) {
        return Some(run_kind(kind, &name, state));
    }

    // ── Toggle agent / trigger / mcp ──────────────────────────────────
    if let Some((kind, name)) = match_toggle(&lower) {
        return Some(toggle_kind(kind, &name, state));
    }

    // ── Delete agent / workflow / memory / trigger ────────────────────
    if let Some((kind, name)) = match_delete(&lower) {
        return Some(delete_kind(kind, &name, state));
    }

    // ── Search memory / twin ──────────────────────────────────────────
    if let Some((kind, query)) = match_search(&lower) {
        return Some(search_kind(kind, &query, state));
    }

    // ── Remember / forget ─────────────────────────────────────────────
    if let Some((key, value)) = match_remember(text) {
        return Some(remember_fact(&key, &value, state));
    }
    if let Some(key) = match_forget(&lower) {
        return Some(forget_fact(&key, state));
    }

    // ── Stop / cancel ─────────────────────────────────────────────────
    if matches_any(&lower, &["stop", "cancel", "stop bob", "cancel that", "nevermind"]) {
        crate::BOB_CANCEL_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
        return Some(RouterResult::simple("Stopping.", "Stopping"));
    }

    // ── Recent activity ───────────────────────────────────────────────
    if matches_any(&lower, &[
        "recent activity", "what did you do today", "what happened today",
        "activity log", "show activity", "recent log",
    ]) {
        return Some(recent_activity(state));
    }

    // ── Notifications ─────────────────────────────────────────────────
    if matches_any(&lower, &["inbox", "notifications", "what's in my inbox", "whats in my inbox"]) {
        return Some(list_notifications(state));
    }
    if matches_any(&lower, &["clear notifications", "dismiss all", "clear inbox", "dismiss notifications"]) {
        return Some(dismiss_notifications(state));
    }

    // No match - fall through to the LLM
    None
}

// ─────────────────────────────────────────────
//  Pattern helpers
// ─────────────────────────────────────────────

fn is_greeting(lower: &str) -> bool {
    let words = ["hi", "hello", "hey", "yo", "hey bob", "hi bob", "hello bob",
                 "morning", "good morning", "evening", "good evening"];
    words.iter().any(|w| lower == *w || lower == format!("{} bob", w))
}
fn is_thanks(lower: &str) -> bool {
    matches!(lower, "thanks" | "thank you" | "ty" | "thx" | "thanks bob" | "thank you bob")
}
fn is_goodbye(lower: &str) -> bool {
    matches!(lower, "bye" | "goodbye" | "see you" | "later" | "cya" | "bye bob")
}

/// Exact command match: the whole message is the pattern, optionally
/// addressed to Bob or ending in punctuation. Prefix/suffix matching used
/// to swallow real questions ("status of my Q3 project" came back as a
/// bare platform status report), so anything longer than the command
/// itself goes to the model.
fn matches_any(lower: &str, patterns: &[&str]) -> bool {
    let t = lower.trim().trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ',')).trim();
    let t = t.strip_suffix(" bob").unwrap_or(t).trim_end_matches(',').trim();
    let t = t.strip_prefix("bob,").or_else(|| t.strip_prefix("bob ")).unwrap_or(t).trim();
    patterns.iter().any(|p| t == *p)
}

fn list_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(?:list|show|show me|what are my|what's my|whats my|get)\s+(?:my\s+)?(\w+)").unwrap())
}

fn match_list(lower: &str) -> Option<&'static str> {
    let caps = list_regex().captures(lower)?;
    let noun = caps.get(1)?.as_str();
    match noun {
        "agents" | "agent" => Some("agents"),
        "workflows" | "workflow" => Some("workflows"),
        "chains" | "chain" => Some("chains"),
        "connectors" | "connector" => Some("connectors"),
        "mcp" | "servers" => Some("mcp"),
        "triggers" | "trigger" => Some("triggers"),
        "notifications" | "notification" | "inbox" => Some("notifications"),
        "memories" | "memory" => Some("memory"),
        _ => None,
    }
}

fn run_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)^(?:run|start|execute|fire|kick off)\s+(?:my\s+|the\s+)?(workflow|chain)?\s*['"]?([^'"]+?)['"]?\s*(?:workflow|chain|now)?$"#).unwrap())
}

fn match_run(lower: &str) -> Option<(&'static str, String)> {
    let caps = run_regex().captures(lower)?;
    let kind = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let name = caps.get(2)?.as_str().trim().to_string();
    if name.is_empty() { return None; }
    let kind_static: &'static str = if kind == "chain" { "chain" } else { "workflow" };
    Some((kind_static, name))
}

fn toggle_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)^(?:toggle|enable|disable|turn on|turn off|pause|resume)\s+(?:my\s+|the\s+)?(agent|trigger|mcp|workflow)?\s*['"]?([^'"]+?)['"]?\s*$"#).unwrap())
}

fn match_toggle(lower: &str) -> Option<(&'static str, String)> {
    let caps = toggle_regex().captures(lower)?;
    let kind = caps.get(1).map(|m| m.as_str()).unwrap_or("agent");
    let name = caps.get(2)?.as_str().trim().to_string();
    if name.is_empty() { return None; }
    let kind_static: &'static str = match kind {
        "trigger" => "trigger",
        "mcp" => "mcp",
        _ => "agent",
    };
    Some((kind_static, name))
}

fn delete_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)^(?:delete|remove|destroy|kill|drop)\s+(?:my\s+|the\s+)?(agent|workflow|chain|memory|trigger|connector)?\s*['"]?([^'"]+?)['"]?\s*$"#).unwrap())
}

fn match_delete(lower: &str) -> Option<(&'static str, String)> {
    let caps = delete_regex().captures(lower)?;
    let kind = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let name = caps.get(2)?.as_str().trim().to_string();
    if name.is_empty() || kind.is_empty() { return None; }
    let kind_static: &'static str = match kind {
        "workflow" => "workflow",
        "chain" => "chain",
        "memory" => "memory",
        "trigger" => "trigger",
        "connector" => "connector",
        _ => "agent",
    };
    Some((kind_static, name))
}

fn search_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)^(?:search|find|look up|look for)\s+(?:my\s+)?(memory|memories|twin|records?|entities|facts?)\s+(?:for\s+)?['"]?(.+?)['"]?$"#).unwrap())
}

fn match_search(lower: &str) -> Option<(&'static str, String)> {
    let caps = search_regex().captures(lower)?;
    let kind = caps.get(1)?.as_str();
    let query = caps.get(2)?.as_str().trim().to_string();
    if query.is_empty() { return None; }
    let kind_static: &'static str = if kind.contains("twin") || kind.contains("record") || kind.contains("entit") {
        "twin"
    } else {
        "memory"
    };
    Some((kind_static, query))
}

fn match_remember(text: &str) -> Option<(String, String)> {
    // "remember X is Y" / "remember X = Y" / "remember that X"
    let re = Regex::new(r"(?i)^remember\s+(?:that\s+)?(.+?)(?:\s+(?:is|=|:)\s+)(.+)$").ok()?;
    let caps = re.captures(text)?;
    let key = caps.get(1)?.as_str().trim().to_string();
    let value = caps.get(2)?.as_str().trim().to_string();
    Some((key, value))
}

fn match_forget(lower: &str) -> Option<String> {
    let re = Regex::new(r#"(?i)^forget\s+(?:about\s+|that\s+|my\s+)?['"]?(.+?)['"]?$"#).ok()?;
    let caps = re.captures(lower)?;
    Some(caps.get(1)?.as_str().trim().to_string())
}

// ─────────────────────────────────────────────
//  Handlers - direct calls into stores
// ─────────────────────────────────────────────

fn status_report(state: &AppState) -> RouterResult {
    let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let enabled = agents.iter().filter(|a| a.enabled).count();
    let total = agents.len();
    let workflows = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner()).workflows.len();
    let chains = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner()).list_chains().len();
    let notifications = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner()).unread_count();

    let md = format!(
        "**Orbix status**\n\n\
         - **Agents:** {} enabled ({} total)\n\
         - **Workflows:** {}\n\
         - **Chains:** {}\n\
         - **Unread notifications:** {}",
        enabled, total, workflows, chains, notifications
    );
    let spoken = format!(
        "{} agents running. {} workflows. {} chains. {} unread notifications.",
        enabled, workflows, chains, notifications
    );
    RouterResult {
        display: md,
        spoken,
        step_label: "Reading status".to_string(),
    }
}

fn list_kind(kind: &str, state: &AppState) -> RouterResult {
    match kind {
        "agents" => {
            let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
            if agents.is_empty() {
                return RouterResult::simple("No agents configured yet.", "Listing agents");
            }
            let lines: Vec<String> = agents.iter().map(|a| {
                format!("- **{}** ({}) - {}", a.name, if a.enabled { "on" } else { "off" }, a.model)
            }).collect();
            RouterResult::simple(format!("**Agents:**\n{}", lines.join("\n")), "Listing agents")
        }
        "workflows" => {
            let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
            if store.workflows.is_empty() {
                return RouterResult::simple("No workflows yet.", "Listing workflows");
            }
            let lines: Vec<String> = store.workflows.iter().take(20).map(|w| {
                format!("- **{}** - {} steps", w.name, w.steps.len())
            }).collect();
            RouterResult::simple(format!("**Workflows:**\n{}", lines.join("\n")), "Listing workflows")
        }
        "chains" => {
            let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
            let chains = store.list_chains();
            if chains.is_empty() {
                return RouterResult::simple("No chains defined.", "Listing chains");
            }
            let lines: Vec<String> = chains.iter().take(20).map(|c| {
                format!("- **{}** ({:?}) - {} nodes", c.name, c.mode, c.nodes.len())
            }).collect();
            RouterResult::simple(format!("**Chains:**\n{}", lines.join("\n")), "Listing chains")
        }
        "connectors" => {
            let inst = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
            if inst.is_empty() {
                return RouterResult::simple("No connectors configured.", "Listing connectors");
            }
            let lines: Vec<String> = inst.iter().take(20).map(|c| {
                format!("- **{}** ({:?}) - {:?}", c.name, c.connector_id, c.status)
            }).collect();
            RouterResult::simple(format!("**Connectors:**\n{}", lines.join("\n")), "Listing connectors")
        }
        "mcp" => {
            let servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
            if servers.is_empty() {
                return RouterResult::simple("No MCP servers.", "Listing MCP servers");
            }
            let lines: Vec<String> = servers.iter().map(|s| {
                format!("- **{}** - {}", s.name, if s.enabled { "on" } else { "off" })
            }).collect();
            RouterResult::simple(format!("**MCP servers:**\n{}", lines.join("\n")), "Listing MCP servers")
        }
        "triggers" => {
            let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
            if store.rules.is_empty() {
                return RouterResult::simple("No trigger rules.", "Listing triggers");
            }
            let lines: Vec<String> = store.rules.iter().take(20).map(|r| {
                format!("- **{}** ({}) - priority {}", r.name, if r.enabled { "on" } else { "off" }, r.priority)
            }).collect();
            RouterResult::simple(format!("**Triggers:**\n{}", lines.join("\n")), "Listing triggers")
        }
        "notifications" => list_notifications(state),
        "memory" => {
            let store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
            if store.entries.is_empty() {
                return RouterResult::simple("Memory is empty.", "Reading memory");
            }
            let lines: Vec<String> = store.entries.iter().take(15).map(|e| {
                format!("- **{}**: {}", e.key, e.value)
            }).collect();
            RouterResult::simple(format!("**Memory:**\n{}", lines.join("\n")), "Reading memory")
        }
        _ => RouterResult::simple(format!("I don't know how to list '{}'.", kind), "Unknown list"),
    }
}

fn run_kind(kind: &str, name: &str, state: &AppState) -> RouterResult {
    match kind {
        "workflow" => {
            // Best-effort name match; the actual execution still needs LLM context.
            // For now just report the match and let the full Bob path handle execution.
            let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
            let found = store.workflows.iter().find(|w| {
                w.name.to_lowercase() == name.to_lowercase() || w.name.to_lowercase().contains(&name.to_lowercase())
            });
            if let Some(w) = found {
                let label = format!("Starting workflow '{}'", w.name);
                RouterResult::simple(
                    format!("Running workflow **{}**. Check the step pill for progress.", w.name),
                    label,
                )
            } else {
                RouterResult::simple(
                    format!("No workflow found matching '{}'. Say 'list workflows' to see available ones.", name),
                    "Workflow not found",
                )
            }
        }
        "chain" => {
            let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
            let chains = store.list_chains();
            let found = chains.iter().find(|c| {
                c.name.to_lowercase() == name.to_lowercase() || c.name.to_lowercase().contains(&name.to_lowercase())
            });
            if let Some(c) = found {
                RouterResult::simple(
                    format!("Running chain **{}** ({:?}).", c.name, c.mode),
                    format!("Starting chain '{}'", c.name),
                )
            } else {
                RouterResult::simple(
                    format!("No chain found matching '{}'. Say 'list chains' to see available ones.", name),
                    "Chain not found",
                )
            }
        }
        _ => RouterResult::simple(format!("Can't run '{}'.", kind), "Unknown run target"),
    }
}

fn toggle_kind(kind: &str, name: &str, state: &AppState) -> RouterResult {
    match kind {
        "agent" => {
            let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(a) = agents.iter_mut().find(|a| a.name.to_lowercase() == name.to_lowercase() || a.name.to_lowercase().contains(&name.to_lowercase())) {
                a.enabled = !a.enabled;
                let new_state = if a.enabled { "enabled" } else { "disabled" };
                let name = a.name.clone();
                RouterResult::simple(
                    format!("Agent **{}** is now {}.", name, new_state),
                    format!("Toggling '{}'", name),
                )
            } else {
                RouterResult::simple(format!("No agent matching '{}'.", name), "Agent not found")
            }
        }
        "trigger" => {
            let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(r) = store.rules.iter_mut().find(|r| r.name.to_lowercase() == name.to_lowercase() || r.name.to_lowercase().contains(&name.to_lowercase())) {
                r.enabled = !r.enabled;
                let new_state = if r.enabled { "enabled" } else { "disabled" };
                let name = r.name.clone();
                store.save();
                RouterResult::simple(
                    format!("Trigger **{}** is now {}.", name, new_state),
                    format!("Toggling trigger '{}'", name),
                )
            } else {
                RouterResult::simple(format!("No trigger matching '{}'.", name), "Trigger not found")
            }
        }
        "mcp" => {
            let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(s) = servers.iter_mut().find(|s| s.name.to_lowercase() == name.to_lowercase()) {
                s.enabled = !s.enabled;
                let new_state = if s.enabled { "enabled" } else { "disabled" };
                RouterResult::simple(
                    format!("MCP server **{}** is now {}.", s.name, new_state),
                    format!("Toggling MCP '{}'", s.name),
                )
            } else {
                RouterResult::simple(format!("No MCP matching '{}'.", name), "MCP not found")
            }
        }
        _ => RouterResult::simple(format!("Can't toggle '{}'.", kind), "Unknown"),
    }
}

fn delete_kind(kind: &str, name: &str, state: &AppState) -> RouterResult {
    match kind {
        "agent" => {
            let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
            let before = agents.len();
            agents.retain(|a| a.name.to_lowercase() != name.to_lowercase());
            if agents.len() < before {
                RouterResult::simple(format!("Deleted agent **{}**.", name), format!("Deleting agent '{}'", name))
            } else {
                RouterResult::simple(format!("No agent '{}' found.", name), "Agent not found")
            }
        }
        "workflow" => {
            let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
            let before = store.workflows.len();
            store.workflows.retain(|w| w.name.to_lowercase() != name.to_lowercase());
            if store.workflows.len() < before {
                store.save();
                RouterResult::simple(format!("Deleted workflow **{}**.", name), format!("Deleting workflow '{}'", name))
            } else {
                RouterResult::simple(format!("No workflow '{}' found.", name), "Workflow not found")
            }
        }
        "memory" => {
            let mut store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
            let ids: Vec<String> = store.entries.iter()
                .filter(|e| e.key.to_lowercase() == name.to_lowercase())
                .map(|e| e.id.clone())
                .collect();
            if ids.is_empty() {
                RouterResult::simple(format!("No memory '{}' found.", name), "Memory not found")
            } else {
                for id in &ids { store.remove(id); }
                let _ = store.save();
                RouterResult::simple(format!("Forgot **{}**.", name), format!("Forgetting '{}'", name))
            }
        }
        _ => RouterResult::simple(format!("Can't delete '{}' kind yet.", kind), "Unknown delete"),
    }
}

fn search_kind(kind: &str, query: &str, state: &AppState) -> RouterResult {
    match kind {
        "memory" => {
            let store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
            let results = store.search(query);
            if results.is_empty() {
                RouterResult::simple(format!("No memories matching '{}'.", query), format!("Searching memory for '{}'", query))
            } else {
                let lines: Vec<String> = results.iter().take(10).map(|e| {
                    format!("- **{}**: {}", e.key, e.value)
                }).collect();
                RouterResult::simple(
                    format!("**Memories matching '{}':**\n{}", query, lines.join("\n")),
                    format!("Searching memory for '{}'", query),
                )
            }
        }
        "twin" => {
            let store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
            let q = query.to_lowercase();
            let matches: Vec<&crate::semantic_twin::TwinEntity> = store.graph.entities.iter()
                .filter(|e| e.name.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
                    || e.properties.values().any(|v| v.to_string().to_lowercase().contains(&q)))
                .collect();
            if matches.is_empty() {
                RouterResult::simple(format!("No twin records matching '{}'.", query), format!("Searching twin for '{}'", query))
            } else {
                let lines: Vec<String> = matches.iter().take(10).map(|e| {
                    format!("- **{}** ({:?}, {:?})", e.name, e.entity_type, e.state)
                }).collect();
                RouterResult::simple(
                    format!("**Records matching '{}':**\n{}", query, lines.join("\n")),
                    format!("Searching twin for '{}'", query),
                )
            }
        }
        _ => RouterResult::simple("Unknown search target.".to_string(), "Unknown"),
    }
}

fn remember_fact(key: &str, value: &str, state: &AppState) -> RouterResult {
    let mut store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    store.add(
        key.to_string(),
        value.to_string(),
        crate::memory::MemoryCategory::Fact,
        "user".to_string(),
    );
    let _ = store.save();
    RouterResult::simple(
        format!("Got it - **{}** is **{}**.", key, value),
        format!("Remembering '{}'", key),
    )
}

fn forget_fact(key: &str, state: &AppState) -> RouterResult {
    let mut store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    let ids: Vec<String> = store.entries.iter()
        .filter(|e| e.key.to_lowercase() == key.to_lowercase() || e.key.to_lowercase().contains(&key.to_lowercase()))
        .map(|e| e.id.clone())
        .collect();
    if ids.is_empty() {
        return RouterResult::simple(format!("Nothing stored under '{}'.", key), "Nothing to forget");
    }
    for id in &ids { store.remove(id); }
    let _ = store.save();
    RouterResult::simple(format!("Forgot **{}**.", key), format!("Forgetting '{}'", key))
}

fn recent_activity(state: &AppState) -> RouterResult {
    let log = state.chat.activity_log.lock().unwrap_or_else(|e| e.into_inner());
    if log.is_empty() {
        return RouterResult::simple("No recent activity.", "Reading activity");
    }
    let lines: Vec<String> = log.iter().rev().take(10).map(|a| {
        format!("- **{}** - {} ({})", a.agent_name, a.action, a.status)
    }).collect();
    RouterResult::simple(format!("**Recent activity:**\n{}", lines.join("\n")), "Reading activity")
}

fn list_notifications(state: &AppState) -> RouterResult {
    let store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    let items = store.list(10);
    if items.is_empty() {
        return RouterResult::simple("Inbox is empty.", "Reading inbox");
    }
    let lines: Vec<String> = items.iter().map(|n| {
        format!("- [{:?}] **{}** - {}", n.level, n.title, n.message)
    }).collect();
    RouterResult::simple(format!("**Inbox:**\n{}", lines.join("\n")), "Reading inbox")
}

fn dismiss_notifications(state: &AppState) -> RouterResult {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    store.dismiss_all();
    RouterResult::simple("Inbox cleared.", "Clearing inbox")
}
