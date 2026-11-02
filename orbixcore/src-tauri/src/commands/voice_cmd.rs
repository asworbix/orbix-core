//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Voice (Epic C)
// ─────────────────────────────────────────────

/// Pre-request microphone + speech recognition permissions. Called once on app boot
/// so the permission prompts don't interrupt the user mid-conversation.
#[tauri::command]
pub(crate) async fn request_voice_permissions() -> Result<serde_json::Value, String> {
    let (mic, speech) = voice::request_voice_permissions().await?;
    Ok(serde_json::json!({
        "microphone": mic,
        "speech_recognition": speech,
    }))
}

#[tauri::command]
pub(crate) async fn speak_text(
    text: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let settings = state.engine.voice_settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    crate::event_bus::emit_ui(&app, "tts-state", "speaking").ok();
    let result = voice::speak(&text, &settings.tts_voice, settings.tts_rate, Some(app.clone())).await;
    crate::event_bus::emit_ui(&app, "tts-state", "idle").ok();
    result
}

#[tauri::command]
pub(crate) async fn stop_speaking() -> Result<(), String> {
    voice::stop_speaking().await
}

#[tauri::command]
pub(crate) async fn list_tts_voices() -> Result<Vec<voice::TtsVoice>, String> {
    voice::list_voices().await
}

#[tauri::command]
pub(crate) async fn start_listening(
    state: State<'_, AppState>,
    stream_holder: State<'_, Mutex<AudioStreamHolder>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let recording = Arc::clone(&state.engine.recording);
    let stream = voice::start_recording(&recording, app.clone())?;

    {
        let mut holder = stream_holder.lock().unwrap_or_else(|e| e.into_inner());
        holder.0 = Some(stream);
    }

    {
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Listening;
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Listening).ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn stop_listening(
    state: State<'_, AppState>,
    stream_holder: State<'_, Mutex<AudioStreamHolder>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    {
        let mut holder = stream_holder.lock().unwrap_or_else(|e| e.into_inner());
        holder.0 = None;
    }

    let recording = Arc::clone(&state.engine.recording);
    let wav_path = voice::stop_recording_and_save(&recording)?;

    {
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Thinking;
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Thinking).ok();

    let language = {
        let settings = state.engine.voice_settings.lock().unwrap_or_else(|e| e.into_inner());
        settings.stt_language.clone()
    };

    let text = voice::transcribe_wav(&wav_path, &language).await?;
    let _ = std::fs::remove_file(&wav_path);

    {
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Idle;
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Idle).ok();
    crate::event_bus::emit_ui(&app, "stt-result", &text).ok();

    Ok(text)
}

#[tauri::command]
pub(crate) async fn get_voice_settings(state: State<'_, AppState>) -> Result<voice::VoiceSettings, String> {
    let settings = state.engine.voice_settings.lock().unwrap_or_else(|e| e.into_inner());
    Ok(settings.clone())
}

#[tauri::command]
pub(crate) async fn update_voice_settings(
    settings: voice::VoiceSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Apply an STT model switch (reloads the whisper context, downloads on
    // next use if needed).
    voice::set_whisper_model(&settings.stt_model);
    let mut current = state.engine.voice_settings.lock().unwrap_or_else(|e| e.into_inner());
    *current = settings;
    Ok(())
}


/// Ensure the selected STT model is downloaded + loaded, emitting status so
/// the UI can show a "downloading..." state when the user switches to a
/// not-yet-downloaded whisper model. Returns once ready.
#[tauri::command]
pub(crate) async fn prepare_stt_model(app: tauri::AppHandle) -> Result<String, String> {
    let already = voice::whisper_model_downloaded();
    let model = voice::current_whisper_model();
    if !already {
        crate::event_bus::emit_ui(&app, "stt-model-status",
            serde_json::json!({ "state": "downloading", "model": model })).ok();
    }
    match voice::ensure_whisper_loaded().await {
        Ok(()) => {
            crate::event_bus::emit_ui(&app, "stt-model-status",
                serde_json::json!({ "state": "ready", "model": voice::current_whisper_model() })).ok();
            Ok("ready".to_string())
        }
        Err(e) => {
            crate::event_bus::emit_ui(&app, "stt-model-status",
                serde_json::json!({ "state": "error", "model": model, "error": e.clone() })).ok();
            Err(e)
        }
    }
}
