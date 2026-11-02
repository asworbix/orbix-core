//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Memory (Epic D)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_memories(state: State<'_, AppState>) -> Result<Vec<memory::MemoryEntry>, String> {
    let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    Ok(mem.entries.clone())
}

#[tauri::command]
pub(crate) async fn add_memory(
    key: String,
    value: String,
    category: memory::MemoryCategory,
    state: State<'_, AppState>,
) -> Result<memory::MemoryEntry, String> {
    let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    let entry = mem.add(key, value, category, "user".to_string());
    mem.save()?;
    Ok(entry)
}

#[tauri::command]
pub(crate) async fn update_memory(
    id: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<memory::MemoryEntry, String> {
    let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    let entry = mem.update(&id, value).ok_or("Memory entry not found")?;
    mem.save()?;
    Ok(entry)
}

#[tauri::command]
pub(crate) async fn remove_memory(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    mem.remove(&id);
    mem.save()?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn search_memories(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<memory::MemoryEntry>, String> {
    let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    Ok(mem.search(&query).into_iter().cloned().collect())
}

#[tauri::command]
pub(crate) async fn get_memory_context(state: State<'_, AppState>) -> Result<String, String> {
    let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    Ok(mem.build_context_prompt())
}

#[tauri::command]
pub(crate) async fn clear_all_memories(state: State<'_, AppState>) -> Result<(), String> {
    let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    mem.entries.clear();
    mem.save()?;
    Ok(())
}

/// Save structured company context to memory - called from frontend onboarding/context dialog.
/// Accepts a map of context fields (e.g. company_name, products, target_market, competencies, ideal_customer_profile).
/// Each field becomes a separate memory entry under the "Context" category for easy retrieval.
#[tauri::command]
pub(crate) async fn save_company_context(
    context: HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<Vec<memory::MemoryEntry>, String> {
    let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    let mut saved = Vec::new();
    for (key, value) in &context {
        if !value.trim().is_empty() {
            let entry = mem.add(
                format!("company:{}", key),
                value.clone(),
                memory::MemoryCategory::Context,
                "user".to_string(),
            );
            saved.push(entry);
        }
    }
    mem.save()?;
    Ok(saved)
}

/// Get existing company context from memory - returns all entries with "company:" prefix.
/// Frontend uses this to pre-fill the context dialog and to check if context already exists.
#[tauri::command]
pub(crate) async fn get_company_context(
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
    let mut context = HashMap::new();
    for entry in &mem.entries {
        if entry.key.starts_with("company:") {
            let field = entry.key.trim_start_matches("company:").to_string();
            context.insert(field, entry.value.clone());
        }
    }
    Ok(context)
}


/// Semantic knowledge-base search (W3): embedding-ranked when an embeddings
/// endpoint is configured, keyword otherwise.
#[tauri::command]
pub(crate) async fn kb_search_semantic(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<knowledge::KBContext>, String> {
    let kb = { state.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    Ok(kb.semantic_context(&query, 2000).await)
}
