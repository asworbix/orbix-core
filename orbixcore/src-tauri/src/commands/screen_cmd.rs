//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Screen Recording Commands (Epic K)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn screen_check_permission() -> Result<bool, String> {
    // We no longer do a test screencapture - just return true and handle errors gracefully
    Ok(true)
}

#[tauri::command]
pub(crate) async fn screen_request_permission() -> Result<(), String> {
    screen_capture::open_screen_recording_settings().await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn screen_start_recording(
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
    audio_holder: State<'_, Mutex<ScreenAudioHolder>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let id = recorder.start();

    // Start audio level monitoring
    let rec_clone = Arc::clone(&*recorder);
    match screen_capture::start_audio_monitor(rec_clone) {
        Ok(stream) => {
            audio_holder.lock().unwrap_or_else(|e| e.into_inner()).0 = Some(stream);
        }
        Err(e) => eprintln!("[OrbixCore] Audio monitor failed (non-fatal): {}", e),
    }

    // Start periodic context capture in background
    // Screenshots are attempted but recording works without them (process context is the key data)
    let rec_clone = Arc::clone(&*recorder);
    let recording_id = id.clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut frame_idx = 0usize;
        // Try first screenshot to see if we have permission
        // First frame might be OrbixCore itself (user just clicked Start), so loop briefly
        let mut has_screen_perm = false;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            match screen_capture::capture_frame(&recording_id, 0, true).await {
                Ok(frame) => {
                    has_screen_perm = !frame.path.is_empty();
                    rec_clone.add_frame(frame);
                    frame_idx = 1;
                    let state = rec_clone.get_state();
                    crate::event_bus::emit_ui(&app_clone, "screen-recording-state", state).ok();
                    break;
                }
                Err(_) => {
                    // Likely OrbixCore is still focused, retry
                    continue;
                }
            }
        }

        if !has_screen_perm {
            eprintln!("[OrbixCore] No screen capture permission - recording process context only (apps, URLs, titles)");
        }

        // Poll at 5s instead of 2s - AppleScript context capture is expensive
        // (50-200ms per call) and human perception doesn't notice the difference.
        // State is only emitted when the frame count changes meaningfully, to
        // avoid waking React renderers on every tick.
        let mut last_emitted_frame = usize::MAX;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if !rec_clone.active.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            match screen_capture::capture_frame(&recording_id, frame_idx, has_screen_perm).await {
                Ok(frame) => {
                    rec_clone.add_frame(frame);
                    frame_idx += 1;
                    if frame_idx != last_emitted_frame {
                        let state = rec_clone.get_state();
                        crate::event_bus::emit_ui(&app_clone, "screen-recording-state", state).ok();
                        last_emitted_frame = frame_idx;
                    }
                }
                Err(e) => {
                    if !e.contains("Skipping") {
                        eprintln!("[OrbixCore] Frame capture failed: {}", e);
                    }
                }
            }
        }
    });

    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Executing).ok();

    // Enable "Stop Recording" in tray menu
    if let Some(item) = app.try_state::<TrayStopRecItem>() {
        item.0.lock().unwrap_or_else(|e| e.into_inner()).set_enabled(true).ok();
    }

    Ok(id)
}

#[tauri::command]
pub(crate) async fn screen_stop_recording(
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
    audio_holder: State<'_, Mutex<ScreenAudioHolder>>,
    app: tauri::AppHandle,
) -> Result<Option<screen_capture::Recording>, String> {
    // Stop audio monitor
    audio_holder.lock().unwrap_or_else(|e| e.into_inner()).0 = None;

    let recording = recorder.stop();
    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Idle).ok();

    // Disable "Stop Recording" in tray menu
    if let Some(item) = app.try_state::<TrayStopRecItem>() {
        item.0.lock().unwrap_or_else(|e| e.into_inner()).set_enabled(false).ok();
    }

    Ok(recording)
}

#[tauri::command]
pub(crate) async fn screen_get_state(
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
) -> Result<screen_capture::RecordingState, String> {
    Ok(recorder.get_state())
}

#[tauri::command]
pub(crate) async fn screen_get_preview(
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
) -> Result<String, String> {
    // Return latest frame as base64 if recording, otherwise try a quick preview
    let last_path = {
        let frames = recorder.frames.lock().unwrap_or_else(|e| e.into_inner());
        frames.last()
            .filter(|f| !f.path.is_empty())
            .map(|f| f.path.clone())
    };

    if let Some(path) = last_path {
        match tokio::fs::read(&path).await {
            Ok(data) => {
                use base64::Engine;
                Ok(base64::engine::general_purpose::STANDARD.encode(&data))
            }
            Err(_) => Ok(String::new()),
        }
    } else {
        // No frames with screenshots - return empty (context-only recording)
        Ok(String::new())
    }
}

#[tauri::command]
pub(crate) async fn screen_list_recordings(
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
) -> Result<Vec<screen_capture::Recording>, String> {
    Ok(recorder.list_recordings())
}

#[tauri::command]
pub(crate) async fn screen_delete_recording(
    id: String,
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
) -> Result<bool, String> {
    Ok(recorder.delete_recording(&id))
}

#[tauri::command]
pub(crate) async fn screen_get_frame(
    recording_id: String,
    frame_index: usize,
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
) -> Result<String, String> {
    let recordings = recorder.recordings.lock().unwrap_or_else(|e| e.into_inner());
    let recording = recordings.iter().find(|r| r.id == recording_id)
        .ok_or("Recording not found")?;
    let frame = recording.frames.get(frame_index)
        .ok_or("Frame not found")?;
    let data = std::fs::read(&frame.path)
        .map_err(|e| format!("Failed to read frame: {}", e))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

#[tauri::command]
pub(crate) async fn screen_get_timeline(
    recording_id: String,
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
) -> Result<Vec<screen_capture::ProcessStep>, String> {
    let recordings = recorder.recordings.lock().unwrap_or_else(|e| e.into_inner());
    let recording = recordings.iter().find(|r| r.id == recording_id)
        .ok_or("Recording not found")?;
    Ok(screen_capture::build_process_timeline(recording))
}

#[tauri::command]
pub(crate) async fn screen_generate_workflow(
    recording_id: String,
    recorder: State<'_, Arc<screen_capture::ScreenRecorder>>,
    state: State<'_, AppState>,
    _llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    // Build the process summary from the recording
    let summary = {
        let recordings = recorder.recordings.lock().unwrap_or_else(|e| e.into_inner());
        let recording = recordings.iter().find(|r| r.id == recording_id)
            .ok_or("Recording not found")?;
        screen_capture::build_process_summary(recording)
    };

    let home_dir = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "~".to_string());

    // Send the process summary to the LLM with a workflow generation prompt
    let prompt = format!(
        "I recorded a process on my computer. Analyze it and create a workflow to replicate it.\n\n\
        {}\n\n\
        IMPORTANT RULES:\n\
        - For every application used, the workflow MUST start by opening it with `open -a \"AppName\"`\n\
        - Applications may not already be running when the workflow executes\n\
        - Add a short delay (sleep 1) after opening apps so they have time to launch\n\
        - Use <action> tags with shell commands, AppleScript, or file operations\n\
        - Use full paths with {home} (not ~)\n\n\
        Output a single <action> tag with type create_workflow that has run:false. \
        Each step should use shell commands (open apps, AppleScript for browser, etc).\n\n\
        Example:\n\
        <action>{{\"type\":\"create_workflow\",\"name\":\"Process Name\",\"description\":\"What it does\",\"run\":false,\"steps\":[\
        {{\"type\":\"action\",\"label\":\"Open Safari\",\"command\":\"open -a Safari && sleep 1\"}},\
        {{\"type\":\"action\",\"label\":\"Navigate to URL\",\"command\":\"osascript -e 'tell application \\\"Safari\\\" to set URL of front document to \\\"https://example.com\\\"'\"}}\
        ]}}</action>",
        summary,
        home = home_dir
    );

    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Thinking).ok();

    // Route through Claude - find an agent with API key
    let agents = storage::load_agents().unwrap_or_default();
    let agent = agents.iter().find(|a| a.enabled && !a.api_key.is_empty())
        .ok_or("No Claude API key configured. Add an Anthropic agent in the Agents panel.")?;

    let history = vec![Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: prompt.clone(),
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    }];

    let raw_content = match call_agent_api(agent, &history).await {
        Ok(text) => text,
        Err(e) => {
            crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Idle).ok();
            return Err(format!("Claude API error: {}", e));
        }
    };

    // Execute the action tags to create the workflow
    let result = execute_actions(&raw_content, &*state, &app).await;

    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Idle).ok();

    if result.contains("Workflow") && result.contains("created") {
        Ok(result)
    } else if raw_content.contains("<action>") {
        Ok(result)
    } else {
        // LLM didn't produce an action - build the workflow directly from the timeline
        let timeline = {
            let recordings = recorder.recordings.lock().unwrap_or_else(|e| e.into_inner());
            let recording = recordings.iter().find(|r| r.id == recording_id)
                .ok_or("Recording not found")?;
            screen_capture::build_process_timeline(recording)
        };

        if timeline.is_empty() {
            return Err("Could not analyze the recording into steps.".into());
        }

        // Build workflow steps directly from timeline
        let wf_steps: Vec<workflow::WorkflowStep> = timeline.iter().enumerate().map(|(i, step)| {
            let mut config = HashMap::new();
            let command = match step.action_type.as_str() {
                "app_switch" => {
                    format!("open -a \"{}\"", step.app.as_deref().unwrap_or("Finder"))
                }
                "browser_nav" => {
                    if let Some(ref url) = step.url {
                        let app_name = step.app.as_deref().unwrap_or("Safari");
                        if app_name.to_lowercase().contains("chrome") {
                            format!("osascript -e 'tell application \"Google Chrome\" to set URL of active tab of front window to \"{}\"'", url)
                        } else {
                            format!("osascript -e 'tell application \"Safari\" to set URL of front document to \"{}\"'", url)
                        }
                    } else {
                        "echo 'No URL captured'".to_string()
                    }
                }
                _ => format!("echo '{}'", step.description),
            };
            config.insert("command".to_string(), serde_json::Value::String(command));

            workflow::WorkflowStep {
                id: uuid::Uuid::new_v4().to_string(),
                order: i,
                step_type: workflow::StepType::Action,
                label: step.description.clone(),
                config,
            }
        }).collect();

        let wf_name = {
            let recordings = recorder.recordings.lock().unwrap_or_else(|e| e.into_inner());
            recordings.iter().find(|r| r.id == recording_id)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "Recorded Process".to_string())
        };

        let wf = workflow::Workflow {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{} (auto)", wf_name),
            description: "Auto-generated from screen recording".to_string(),
            steps: wf_steps,
            trigger: workflow::WorkflowTrigger::Manual,
            deliver: Default::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        let wf_name_result = wf.name.clone();
        let step_count = wf.steps.len();
        state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner()).workflows.push(wf);

        Ok(format!("**Workflow \"{}\" created** with {} steps from your recording.", wf_name_result, step_count))
    }
}
