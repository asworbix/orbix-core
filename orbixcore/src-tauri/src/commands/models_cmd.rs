//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - System Scan & Model Recommendations
// ─────────────────────────────────────────────


#[tauri::command]
pub(crate) async fn scan_system_for_models() -> Result<SystemScan, String> {
    // Platform-appropriate probes via os_auto (sysctl/sw_vers on macOS,
    // CIM via PowerShell on Windows).
    let info = crate::os_auto::get_system_info().await?;
    let ram_gb: u64 = info
        .memory_total
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|g| g.round() as u64)
        .unwrap_or(16);
    let chip = info.cpu_model.clone();
    let os_version = info.os_version.clone();

    // Build recommendations based on RAM
    let models = local_llm::available_models();
    let mut recommendations: Vec<ModelRecommendation> = Vec::new();

    for m in &models {
        let size_gb = m.size_bytes as f64 / 1_000_000_000.0;
        // Model needs ~1.2x its size in RAM for comfortable inference
        let ram_needed = (size_gb * 1.2) as u64;
        let fits = ram_gb >= ram_needed + 4; // keep 4GB for OS

        // Every curated chat model can see images once its projector is
        // downloaded; the embedding model is neither.
        let category = if local_llm::is_embedding_model(&m.id) {
            "embedding"
        } else if local_llm::is_vision_model(&m.id) {
            "vision"
        } else {
            "text"
        };

        let reason = if fits {
            if ram_gb >= ram_needed + 12 {
                "Runs great - plenty of headroom".to_string()
            } else if ram_gb >= ram_needed + 6 {
                "Good fit - smooth performance".to_string()
            } else {
                "Fits - may be tight with heavy multitasking".to_string()
            }
        } else {
            format!("Needs ~{}GB RAM - this device has {}GB", ram_needed + 4, ram_gb)
        };

        recommendations.push(ModelRecommendation {
            model_id: m.id.clone(),
            model_name: m.name.clone(),
            size_gb,
            fits,
            reason,
            category: category.to_string(),
        });
    }

    // Pick the best recommendation: largest fitting chat model
    let best = recommendations.iter()
        .filter(|r| r.fits && r.category != "embedding")
        .max_by(|a, b| a.size_gb.partial_cmp(&b.size_gb).unwrap_or(std::cmp::Ordering::Equal));

    let (rec_id, rec_name, rec_reason) = match best {
        Some(r) => (r.model_id.clone(), r.model_name.clone(), r.reason.clone()),
        None => ("gemma-4-e2b".into(), "Gemma 4 E2B".into(), "Smallest capable model".into()),
    };

    Ok(SystemScan {
        ram_gb,
        chip,
        os_version,
        recommended_model: rec_id,
        recommended_model_name: rec_name,
        recommended_reason: rec_reason,
        all_recommendations: recommendations,
    })
}


// ─────────────────────────────────────────────
//  Commands - Local LLM
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_local_models() -> Result<Vec<local_llm::LocalModel>, String> {
    Ok(local_llm::list_models_with_status())
}

/// This Mac's model capability in one object, for guidance UI: what it
/// can run well and which curated model is the best pick.
#[tauri::command]
pub(crate) async fn model_guidance() -> Result<serde_json::Value, String> {
    let total = local_llm::total_memory_gb();
    let usable = total * 0.70;
    // Largest comfortable model size: needed = size*1.15 <= usable*0.80
    let comfortable_gb = usable * 0.80 / 1.15;
    let rec = local_llm::recommended_model_id();
    let rec_model = rec
        .as_deref()
        .and_then(|id| local_llm::available_models().into_iter().find(|m| m.id == id));
    Ok(serde_json::json!({
        "total_gb": total.round(),
        "usable_gb": usable.round(),
        "comfortable_model_gb": comfortable_gb.round(),
        "recommended_id": rec,
        "recommended_name": rec_model.as_ref().map(|m| m.name.clone()),
        "recommended_size_bytes": rec_model.as_ref().map(|m| m.size_bytes),
        "recommended_downloaded": rec_model
            .as_ref()
            .map(|m| local_llm::is_model_downloaded(&m.filename))
            .unwrap_or(false),
    }))
}

/// Leftover GGUF files in the models folder that no listed model uses
/// (retired families, aborted downloads), with sizes, largest first.
#[tauri::command]
pub(crate) fn list_unsupported_model_files() -> Result<Vec<serde_json::Value>, String> {
    Ok(local_llm::unsupported_model_files()
        .into_iter()
        .map(|(name, size)| serde_json::json!({ "filename": name, "size_bytes": size }))
        .collect())
}

/// Delete one leftover file by name. Only files `list_unsupported_model_files`
/// reports can be removed.
#[tauri::command]
pub(crate) fn delete_unsupported_model_file(filename: String, app: tauri::AppHandle) -> Result<(), String> {
    local_llm::delete_unsupported_model_file(&filename)?;
    crate::event_bus::emit_ui(&app, "data-changed", "models").ok();
    Ok(())
}

/// Resolve a model id against built-ins first, then custom user models.
pub(crate) fn resolve_model(model_id: &str) -> Option<local_llm::LocalModel> {
    if let Some(m) = local_llm::available_models().into_iter().find(|m| m.id == model_id) {
        return Some(m);
    }
    local_llm::load_custom_models().into_iter().find(|m| m.id == model_id)
}

#[tauri::command]
pub(crate) async fn download_model(
    model_id: String,
    llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let model = resolve_model(&model_id)
        .ok_or_else(|| format!("Unknown model: {}", model_id))?;
    local_llm::download_model(&model, &llama.download_progress, &app).await?;
    // No restart for a newly downloaded projector: it is passed at launch
    // only for a turn that carries an image (ensure_model_loaded_for), so
    // the next image turn loads it.
    crate::event_bus::emit_ui(&app, "data-changed", "models").ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn add_custom_model(
    name: String,
    url: String,
    description: Option<String>,
    size_hint: Option<u64>,
    app: tauri::AppHandle,
) -> Result<local_llm::LocalModel, String> {
    let model = local_llm::add_custom_model(name, url, description, size_hint)?;
    crate::event_bus::emit_ui(&app, "data-changed", "custom-model-added").ok();
    Ok(model)
}

#[tauri::command]
pub(crate) async fn remove_custom_model(
    model_id: String,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let removed = local_llm::remove_custom_model(&model_id)?;
    if removed {
        crate::event_bus::emit_ui(&app, "data-changed", "custom-model-removed").ok();
    }
    Ok(removed)
}

#[tauri::command]
pub(crate) async fn delete_local_model(model_id: String) -> Result<(), String> {
    if let Some(model) = resolve_model(&model_id) {
        local_llm::delete_model(&model.filename).await
    } else {
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn get_download_progress(
    llama: State<'_, LlamaServerHolder>,
) -> Result<serde_json::Value, String> {
    let p = &llama.download_progress;
    let total = p.total_bytes.load(std::sync::atomic::Ordering::SeqCst);
    let downloaded = p.downloaded_bytes.load(std::sync::atomic::Ordering::SeqCst);
    let is_dl = p.is_downloading.load(std::sync::atomic::Ordering::SeqCst);
    let model_id = p.current_model_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let percent = if total > 0 { ((downloaded as f64 / total as f64) * 100.0) as u32 } else { 0 };
    Ok(serde_json::json!({
        "is_downloading": is_dl,
        "model_id": model_id,
        "downloaded": downloaded,
        "total": total,
        "percent": percent,
    }))
}

#[tauri::command]
pub(crate) async fn select_local_model(
    model_id: String,
    state: State<'_, AppState>,
    llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    
    // Store selection - in memory for this session, on disk so the next
    // launch starts the same model instead of falling back to the suite
    // default.
    {
        let mut sel = state.engine.selected_local_model.lock().unwrap_or_else(|e| e.into_inner());
        *sel = Some(model_id.clone());
    }
    let _ = storage::save_json("selected_model.json", &model_id);
    // Find the model (built-in OR custom - W1.1 bug fix) and start the server
    let model = local_llm::find_model(&model_id)
        .ok_or_else(|| format!("Unknown model id: {}", model_id))?;
    {
        if model.downloaded {
            // Stop existing server
            let mut process = llama.process.lock().await;
            local_llm::stop_server(&mut process.child).await;
            // Start new server
            crate::event_bus::emit_ui(&app, "local-llm-status", "starting").ok();
            match local_llm::start_server(&model.filename, &model.id).await {
                Ok(child) => {
                    process.child = Some(child);
                    process.model_id = model.id.clone();
                    process.vision = false;
                    crate::event_bus::emit_ui(&app, "local-llm-status", "running").ok();
                }
                Err(e) => {
                    crate::event_bus::emit_ui(&app, "local-llm-status", "failed").ok();
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_selected_local_model(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let mut sel = state.engine.selected_local_model.lock().unwrap_or_else(|e| e.into_inner());
    // If hydrate has not populated it yet (frontend queried early on
    // launch), read the persisted choice straight from disk so the UI
    // does not fall back to auto-selecting a default and overwrite it.
    if sel.is_none() {
        if let Ok(id) = storage::load_json::<String>("selected_model.json") {
            *sel = Some(id);
        }
    }
    Ok(sel.clone())
}

#[tauri::command]
pub(crate) async fn get_local_llm_status(
    llama: State<'_, LlamaServerHolder>,
) -> Result<serde_json::Value, String> {
    let process = llama.process.lock().await;
    let running = process.child.is_some();
    let model_id = process.model_id.clone();
    let server_available = if running { local_llm::is_server_running().await } else { false };
    // Vision = the running model has a projector on disk (so it launched with
    // --mmproj). Lets the chat UI hint before a user drops an image onto a
    // model that can't see. mmproj_downloaded is already false for text models.
    let vision = local_llm::mmproj_downloaded(&model_id);
    Ok(serde_json::json!({
        "running": running,
        "model_id": model_id,
        "server_available": server_available,
        "vision": vision,
        "claude_mode": false,
    }))
}

#[tauri::command]
pub(crate) async fn stop_local_llm(
    llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    
    let mut process = llama.process.lock().await;
    local_llm::stop_server(&mut process.child).await;
    process.model_id = String::new();
    crate::event_bus::emit_ui(&app, "local-llm-status", "stopped").ok();
    Ok(())
}


// ─────────────────────────────────────────────
//  Gemma 4 Suite commands
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn get_gemma_suite_status() -> Result<serde_json::Value, String> {
    let (agent_ready, reasoning_ready) = local_llm::is_suite_ready();
    let models = local_llm::list_models_with_status();
    let agent_model = models.iter().find(|m| m.id == local_llm::SUITE_AGENT_ID);
    let reasoning_model = models.iter().find(|m| m.id == local_llm::SUITE_REASONING_ID);
    Ok(serde_json::json!({
        "suite_ready": agent_ready && reasoning_ready,
        "agent": {
            "id": local_llm::SUITE_AGENT_ID,
            "ready": agent_ready,
            "name": agent_model.map(|m| m.name.as_str()).unwrap_or("Gemma 4 E4B"),
            "size_bytes": agent_model.map(|m| m.size_bytes).unwrap_or(0),
        },
        "reasoning": {
            "id": local_llm::SUITE_REASONING_ID,
            "ready": reasoning_ready,
            "name": reasoning_model.map(|m| m.name.as_str()).unwrap_or("Gemma 4 26B-A4B"),
            "size_bytes": reasoning_model.map(|m| m.size_bytes).unwrap_or(0),
        },
    }))
}

#[tauri::command]
pub(crate) async fn download_gemma_suite(
    llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    

    let models = local_llm::available_models();
    let suite = local_llm::suite_models();
    let progress = llama.download_progress.clone();

    for (model_id, role) in &suite {
        if let Some(model) = models.iter().find(|m| m.id == *model_id) {
            if local_llm::is_model_downloaded(&model.filename) {
                crate::event_bus::emit_ui(&app, "gemma-suite-progress", serde_json::json!({
                    "model_id": model_id,
                    "role": format!("{:?}", role),
                    "status": "already_downloaded",
                })).ok();
                continue;
            }

            crate::event_bus::emit_ui(&app, "gemma-suite-progress", serde_json::json!({
                "model_id": model_id,
                "role": format!("{:?}", role),
                "status": "downloading",
            })).ok();

            local_llm::download_model(model, &progress, &app).await?;

            crate::event_bus::emit_ui(&app, "gemma-suite-progress", serde_json::json!({
                "model_id": model_id,
                "role": format!("{:?}", role),
                "status": "complete",
            })).ok();
        }
    }

    // Ensure llama-server binary is available
    local_llm::ensure_llama_server().await?;

    crate::event_bus::emit_ui(&app, "gemma-suite-progress", serde_json::json!({
        "status": "suite_ready",
    })).ok();

    Ok(())
}

#[tauri::command]
pub(crate) async fn start_gemma_suite(
    llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    

    let models = local_llm::list_models_with_status();

    // Start agent server (E4B) - lightweight, always-on
    let agent_model = models.iter().find(|m| m.id == local_llm::SUITE_AGENT_ID && m.downloaded);
    if let Some(model) = agent_model {
        let mut proc = llama.agent_process.lock().await;
        if proc.child.is_none() {
            crate::event_bus::emit_ui(&app, "local-llm-status", "starting-agent").ok();
            match local_llm::start_agent_server(&model.filename, &model.id).await {
                Ok(child) => {
                    proc.child = Some(child);
                    proc.model_id = model.id.clone();
                    proc.port = local_llm::agent_port();
                    crate::event_bus::emit_ui(&app, "local-llm-status", "agent-running").ok();
                }
                Err(e) => {
                    crate::event_bus::emit_ui(&app, "local-llm-status", "agent-failed").ok();
                    return Err(format!("Agent server failed: {}", e));
                }
            }
        }
    }

    // Start reasoning server (26B-A4B) - main model
    let reasoning_model = models.iter().find(|m| m.id == local_llm::SUITE_REASONING_ID && m.downloaded);
    if let Some(model) = reasoning_model {
        let mut proc = llama.process.lock().await;
        if proc.child.is_none() {
            crate::event_bus::emit_ui(&app, "local-llm-status", "starting").ok();
            match local_llm::start_server(&model.filename, &model.id).await {
                Ok(child) => {
                    proc.child = Some(child);
                    proc.model_id = model.id.clone();
                    proc.vision = false;
                    crate::event_bus::emit_ui(&app, "local-llm-status", "running").ok();
                }
                Err(e) => {
                    // 26B might OOM - fall back to agent-only mode
                    crate::event_bus::emit_ui(&app, "local-llm-status", "reasoning-failed-agent-only").ok();
                    return Ok(serde_json::json!({
                        "mode": "agent-only",
                        "reason": format!("26B failed to start ({}), running E4B only", e),
                    }));
                }
            }
        }
    }

    Ok(serde_json::json!({
        "mode": if reasoning_model.is_some() && agent_model.is_some() { "full-suite" }
                else if agent_model.is_some() { "agent-only" }
                else if reasoning_model.is_some() { "reasoning-only" }
                else { "none" },
    }))
}


// ─────────────────────────────────────────────
//  HuggingFace repo resolution (W1.4)
// ─────────────────────────────────────────────

/// List the GGUF files (with sizes and quant labels) of a HuggingFace
/// repo so the user can pick a quant to install. Accepts "org/name" or
/// any huggingface.co URL. Token is per-request only, never stored.
#[tauri::command]
pub(crate) async fn hf_resolve_repo(
    repo: String,
    token: Option<String>,
) -> Result<Vec<local_llm::HfGgufFile>, String> {
    local_llm::hf_list_gguf_files(&repo, token).await
}
