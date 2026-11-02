//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Suggestion Commands (Epic Z)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_suggestions(state: State<'_, AppState>) -> Result<Vec<suggestions::Suggestion>, String> {
    let store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_active())
}

#[tauri::command]
pub(crate) async fn get_suggestion_history(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<suggestions::Suggestion>, String> {
    let store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_history(limit.unwrap_or(20)))
}

#[tauri::command]
pub(crate) async fn accept_suggestion(state: State<'_, AppState>, id: String) -> Result<suggestions::Suggestion, String> {
    let mut store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    store.accept(&id).ok_or("Suggestion not found".into())
}

#[tauri::command]
pub(crate) async fn dismiss_suggestion(state: State<'_, AppState>, id: String) -> Result<suggestions::Suggestion, String> {
    let mut store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    store.dismiss(&id).ok_or("Suggestion not found".into())
}

#[tauri::command]
pub(crate) async fn dismiss_all_suggestions(state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    store.dismiss_all();
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_suggestion_preferences(state: State<'_, AppState>) -> Result<suggestions::SuggestionPreferences, String> {
    let store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_preferences())
}

#[tauri::command]
pub(crate) async fn update_suggestion_preferences(
    state: State<'_, AppState>,
    prefs: suggestions::SuggestionPreferences,
) -> Result<suggestions::SuggestionPreferences, String> {
    let mut store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update_preferences(prefs.clone());
    Ok(prefs)
}

#[tauri::command]
pub(crate) async fn suggestion_count(state: State<'_, AppState>) -> Result<usize, String> {
    let store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.active_count())
}

// ── Dashboard & Analytics (Epic AA) ──

#[tauri::command]
pub(crate) async fn get_dashboard_metrics(range: String, state: State<'_, AppState>) -> Result<analytics::DashboardMetrics, String> {
    let time_range: analytics::TimeRange = serde_json::from_str(&format!("\"{}\"", range))
        .unwrap_or(analytics::TimeRange::Week);
    let pipeline = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    let triggers = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    let workflows = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    let scheduler = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(analytics::compute_dashboard(&pipeline, &triggers, &workflows, &scheduler, &time_range))
}

// ── Data Export & Reporting (Epic AB) ──

#[tauri::command]
pub(crate) async fn export_report(options: export::ExportOptions, state: State<'_, AppState>) -> Result<export::ExportResult, String> {
    let pipeline = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    let triggers = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    let scheduler = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    export::run_export(&pipeline, &triggers, &scheduler, &options)
}

#[tauri::command]
pub(crate) async fn list_reports() -> Result<Vec<export::ExportResult>, String> {
    Ok(export::list_reports())
}

#[tauri::command]
pub(crate) async fn delete_report(filename: String) -> Result<(), String> {
    export::delete_report(&filename)
}

#[tauri::command]
pub(crate) async fn open_reports_folder() -> Result<(), String> {
    let dir = export::reports_dir();
    std::process::Command::new("open")
        .arg(dir.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| format!("Failed to open folder: {}", e))?;
    Ok(())
}

// ── Knowledge Base (Epic AC) ──

#[tauri::command]
pub(crate) async fn list_kb_articles(
    category: Option<String>, tag: Option<String>, state: State<'_, AppState>,
) -> Result<Vec<knowledge::KBArticle>, String> {
    let store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list(category.as_deref(), tag.as_deref()))
}

#[tauri::command]
pub(crate) async fn get_kb_article(id: String, state: State<'_, AppState>) -> Result<knowledge::KBArticle, String> {
    let store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get(&id).cloned().ok_or_else(|| "Article not found".into())
}

#[tauri::command]
pub(crate) async fn create_kb_article(
    title: String, content: String, category: String,
    tags: Vec<String>, source: knowledge::KBSource,
    state: State<'_, AppState>,
) -> Result<knowledge::KBArticle, String> {
    let mut store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.create(title, content, category, tags, source))
}

#[tauri::command]
pub(crate) async fn update_kb_article(
    id: String, title: Option<String>, content: Option<String>,
    category: Option<String>, tags: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<knowledge::KBArticle, String> {
    let mut store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update(&id, title, content, category, tags)
        .ok_or_else(|| "Article not found".into())
}

#[tauri::command]
pub(crate) async fn delete_kb_article(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.delete(&id))
}

#[tauri::command]
pub(crate) async fn search_kb(query: String, limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<knowledge::SearchResult>, String> {
    let store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.search(&query, limit.unwrap_or(10)))
}

#[tauri::command]
pub(crate) async fn get_kb_context(query: String, max_chars: Option<usize>, state: State<'_, AppState>) -> Result<Vec<knowledge::KBContext>, String> {
    let store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_context(&query, max_chars.unwrap_or(3000)))
}

#[tauri::command]
pub(crate) async fn list_kb_categories(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_categories())
}

#[tauri::command]
pub(crate) async fn list_kb_tags(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let store = state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_tags())
}

// ── Notifications Center (Epic AD) ──

#[tauri::command]
pub(crate) async fn list_notifications(limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<notifications::Notification>, String> {
    let store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list(limit.unwrap_or(50)))
}

#[tauri::command]
pub(crate) async fn notification_unread_count(state: State<'_, AppState>) -> Result<usize, String> {
    let store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.unread_count())
}

#[tauri::command]
pub(crate) async fn mark_notification_read(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.mark_read(&id))
}

#[tauri::command]
pub(crate) async fn mark_all_notifications_read(state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    store.mark_all_read();
    Ok(())
}

#[tauri::command]
pub(crate) async fn dismiss_notification(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.dismiss(&id))
}

#[tauri::command]
pub(crate) async fn dismiss_all_notifications(state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    store.dismiss_all();
    Ok(())
}

#[tauri::command]
pub(crate) async fn add_notification(
    source: notifications::NotificationSource,
    level: notifications::NotificationLevel,
    title: String, message: String,
    action_panel: Option<String>, action_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    let mut notif = notifications::Notification::new(source, level, title, message);
    if let Some(panel) = action_panel {
        notif = notif.with_action(&panel, action_id.as_deref());
    }
    Ok(store.add(notif))
}

#[tauri::command]
pub(crate) async fn get_notification_preferences(state: State<'_, AppState>) -> Result<notifications::NotificationPreferences, String> {
    let store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_preferences())
}

#[tauri::command]
pub(crate) async fn update_notification_preferences(prefs: notifications::NotificationPreferences, state: State<'_, AppState>) -> Result<notifications::NotificationPreferences, String> {
    let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update_preferences(prefs);
    Ok(store.get_preferences())
}

// ── Conversation Search (Epic AE) ──

#[tauri::command]
pub(crate) async fn search_conversations(
    query: String,
    role: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    conversation_id: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<search::ConversationSearchResult>, String> {
    let index = state.connect.search_index.lock().unwrap_or_else(|e| e.into_inner());
    let filters = search::SearchFilters {
        role, date_from, date_to, conversation_id,
    };
    Ok(index.search(&query, &filters, limit.unwrap_or(20)))
}

#[tauri::command]
pub(crate) async fn index_conversation(
    conversation_id: String,
    title: String,
    messages: Vec<(String, String, String, String)>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut index = state.connect.search_index.lock().unwrap_or_else(|e| e.into_inner());
    index.index_conversation(&conversation_id, &title, &messages);
    Ok(())
}

#[tauri::command]
pub(crate) async fn remove_conversation_index(conversation_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut index = state.connect.search_index.lock().unwrap_or_else(|e| e.into_inner());
    index.remove_conversation(&conversation_id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_search_stats(state: State<'_, AppState>) -> Result<(usize, usize), String> {
    let index = state.connect.search_index.lock().unwrap_or_else(|e| e.into_inner());
    Ok((index.indexed_count(), index.term_count()))
}

// ── Keyboard Shortcuts (Epic AF) ──

#[tauri::command]
pub(crate) async fn get_shortcut_preferences(state: State<'_, AppState>) -> Result<shortcuts::ShortcutPreferences, String> {
    let store = state.automation.shortcuts_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_preferences())
}

#[tauri::command]
pub(crate) async fn update_shortcut_preferences(prefs: shortcuts::ShortcutPreferences, state: State<'_, AppState>) -> Result<shortcuts::ShortcutPreferences, String> {
    let mut store = state.automation.shortcuts_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update_preferences(prefs);
    Ok(store.get_preferences())
}

#[tauri::command]
pub(crate) async fn update_shortcut_binding(
    id: String, keys: Option<String>, enabled: Option<bool>,
    state: State<'_, AppState>,
) -> Result<shortcuts::ShortcutBinding, String> {
    let mut store = state.automation.shortcuts_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update_binding(&id, keys, enabled)
        .ok_or_else(|| "Binding not found".into())
}

#[tauri::command]
pub(crate) async fn add_recent_command(command: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.automation.shortcuts_store.lock().unwrap_or_else(|e| e.into_inner());
    store.add_recent_command(command);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_recent_commands(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let store = state.automation.shortcuts_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_recent_commands())
}

#[tauri::command]
pub(crate) async fn reset_shortcuts(state: State<'_, AppState>) -> Result<shortcuts::ShortcutPreferences, String> {
    let mut store = state.automation.shortcuts_store.lock().unwrap_or_else(|e| e.into_inner());
    store.reset_to_defaults();
    Ok(store.get_preferences())
}

// ── Plugin System (Epic AG) ──

#[tauri::command]
pub(crate) async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<plugins::Plugin>, String> {
    let store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list())
}

#[tauri::command]
pub(crate) async fn get_plugin(id: String, state: State<'_, AppState>) -> Result<plugins::Plugin, String> {
    let store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get(&id).cloned().ok_or_else(|| "Plugin not found".into())
}

#[tauri::command]
pub(crate) async fn install_plugin(manifest: plugins::PluginManifest, state: State<'_, AppState>) -> Result<plugins::Plugin, String> {
    let mut store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    store.install(manifest)
}

#[tauri::command]
pub(crate) async fn uninstall_plugin(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.uninstall(&id))
}

#[tauri::command]
pub(crate) async fn enable_plugin(id: String, state: State<'_, AppState>) -> Result<plugins::Plugin, String> {
    let mut store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    store.enable(&id).ok_or_else(|| "Plugin not found".into())
}

#[tauri::command]
pub(crate) async fn disable_plugin(id: String, state: State<'_, AppState>) -> Result<plugins::Plugin, String> {
    let mut store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    store.disable(&id).ok_or_else(|| "Plugin not found".into())
}

#[tauri::command]
pub(crate) async fn grant_plugin_permission(plugin_id: String, permission_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.grant_permission(&plugin_id, &permission_id))
}

#[tauri::command]
pub(crate) async fn update_plugin_settings(id: String, settings: serde_json::Value, state: State<'_, AppState>) -> Result<plugins::Plugin, String> {
    let mut store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update_settings(&id, settings).ok_or_else(|| "Plugin not found".into())
}

// ── Team Collaboration (Epic AH) ──

#[tauri::command]
pub(crate) async fn create_share_bundle(
    name: String, description: String, author: String,
    items: Vec<sharing::SharedItem>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut store = state.connect.share_store.lock().unwrap_or_else(|e| e.into_inner());
    let bundle = store.create_bundle(name, description, author, items);
    Ok(sharing::ShareStore::export_bundle(&bundle))
}

#[tauri::command]
pub(crate) async fn parse_share_bundle(json: String) -> Result<sharing::ShareBundle, String> {
    sharing::ShareStore::parse_bundle(&json)
}

#[tauri::command]
pub(crate) async fn record_import(
    bundle: sharing::ShareBundle,
    result: sharing::ImportResult,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.connect.share_store.lock().unwrap_or_else(|e| e.into_inner());
    store.record_import(&bundle, &result);
    Ok(())
}

#[tauri::command]
pub(crate) async fn is_item_shared(item_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let store = state.connect.share_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.is_shared(&item_id))
}

#[tauri::command]
pub(crate) async fn list_shared_items(state: State<'_, AppState>) -> Result<Vec<sharing::ShareRecord>, String> {
    let store = state.connect.share_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_shared())
}

#[tauri::command]
pub(crate) async fn list_import_history(state: State<'_, AppState>) -> Result<Vec<sharing::ImportRecord>, String> {
    let store = state.connect.share_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_imports())
}

#[tauri::command]
pub(crate) async fn copy_to_clipboard(text: String) -> Result<(), String> {
    std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
            Ok(())
        })
        .map_err(|e| format!("Clipboard error: {}", e))
}

/// A single row in the unified plugin registry (W7): connectors, workflows,
/// and installed plugins as one queryable list, so "what's installed" has
/// one answer instead of three separate stores.
#[derive(serde::Serialize)]
pub(crate) struct RegistryEntry {
    pub id: String,
    pub name: String,
    /// "plugin" | "connector" | "workflow"
    pub kind: String,
    pub status: String,
}

#[tauri::command]
pub(crate) async fn list_registry(state: State<'_, AppState>) -> Result<Vec<RegistryEntry>, String> {
    let mut out: Vec<RegistryEntry> = Vec::new();

    // Installed plugins.
    {
        let store = state.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner());
        for p in store.list() {
            out.push(RegistryEntry {
                id: p.id.clone(),
                name: p.manifest.name.clone(),
                kind: "plugin".to_string(),
                status: format!("{:?}", p.status).to_lowercase(),
            });
        }
    }
    // Connector modules (runtime-activated, signed Deno modules).
    for slug in connector_modules::list_module_slugs() {
        out.push(RegistryEntry {
            id: slug.clone(),
            name: slug,
            kind: "connector".to_string(),
            status: "active".to_string(),
        });
    }
    // Native catalog connector instances (W5.2). The native and Deno
    // connector systems coexist; this unifies their SURFACE so "what am I
    // connected to" has one answer. Runtime consolidation (expressing native
    // connectors as Deno modules) is deferred: it would break working
    // connectors and their credential store for no capability gain.
    {
        let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        for inst in instances.iter() {
            out.push(RegistryEntry {
                id: inst.id.clone(),
                name: inst.name.clone(),
                kind: "connector".to_string(),
                status: format!("{:?}", inst.status).to_lowercase(),
            });
        }
    }
    // Saved workflows.
    {
        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        for w in store.list() {
            out.push(RegistryEntry {
                id: w.id.clone(),
                name: w.name.clone(),
                kind: "workflow".to_string(),
                status: match w.trigger {
                    workflow::WorkflowTrigger::Schedule(_) => "scheduled",
                    workflow::WorkflowTrigger::Command(_) => "command",
                    workflow::WorkflowTrigger::Manual => "manual",
                }.to_string(),
            });
        }
    }
    Ok(out)
}
