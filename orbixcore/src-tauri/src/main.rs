// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod voice;
mod storage;
mod memory;
mod local_llm;
mod router;
mod tools;
mod tool_schemas;
mod browser;
mod workflow;
mod os_auto;
mod app_bridge;
mod endpoint;
mod discovery;
mod handoff;
mod destinations;
mod screen_capture;
mod process_ext;
mod app_version;
mod mobile_api;
mod onboarding;
mod connectors;
mod oauth;
mod triggers;
mod replay_log;
mod pipeline;
mod scheduler;
mod orchestrator;
mod suggestions;
mod analytics;
mod export;
mod knowledge;
mod notifications;
mod search;
mod shortcuts;
mod plugins;
mod sharing;
mod fleet;
mod fleet_routing;
mod fleet_monitor;
mod playbook;
mod fleet_perf;
mod audit;
mod fleet_auto;
mod fleet_reports;
mod fleet_tags;
mod fleet_sla;
mod fleet_timeline;
mod fleet_groups;
mod fleet_incidents;
mod fleet_maintenance;
mod fleet_commands;
mod fleet_capacity;
mod fleet_webhooks;
mod fleet_runbooks;
mod fleet_environments;
mod seed;
mod schema_inference;
mod workflow_templates;
mod agent_router;
mod presets;
mod semantic_twin;
mod event_ledger;
mod compliance;
mod audit_log;
mod connector_modules;
mod connector_bus;
mod connector_manifest;
mod connector_runtime;
mod connector_vault;
mod connector_scanner;
mod connector_sdk;
mod connector_signer;
mod connector_activate;
mod deno_sidecar;
mod browser_attach;
mod job_profile;
mod job_profile_scheduler;
mod review_queue;
mod bob_connector_author;
mod workspace;
mod turn_scope;
mod always_on;
mod quick_bar;
mod text_calls;
mod bob_agentic;
mod agent_shifts;
mod shift_monitor;
mod run_log;
mod repos;
mod research;
mod skills;
mod imagine;
mod today;
mod graph_outlook;
mod power;
mod brand;
mod docgen;
mod bob_cli_install;
mod time_travel;
mod cli;
mod cli_auth;
mod data_control;
mod event_bus;
mod config;
mod job_scheduler;
mod state_machine;
mod session_manager;
mod observability;
mod gateway;
mod window_intelligence;
mod peer;
mod pair_drag;
mod user_actions;
mod bridge_author;
mod services;
mod mcp_client;
mod devtools_discover;
mod secure_relay;
mod code_assistant;
mod agent_council;
mod dynamic_panels;
mod models;
pub use models::*;
mod commands;
pub(crate) use commands::*;
mod prelude;
mod state;
pub use state::*;
mod handler;
mod run;
mod embeddings;
mod productivity;
mod dispatcher;
mod proactivity;
mod egress;
mod support;
pub use support::*;
mod llm_engine;
pub(crate) use llm_engine::*;
mod bob_core;
pub(crate) use bob_core::*;

use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::image::Image;
// Global shortcuts disabled for now; tray menu provides stop recording access

use workflow::WorkflowStore;
fn main() {
    // ── Window Intelligence subprocess mode ───
    // When invoked with --wi-profile <pid> <app_name>, profile the window and exit.
    // This runs in an isolated process so ObjC/AX crashes don't kill the main app.
    {
        let args: Vec<String> = std::env::args().collect();
        if args.len() >= 4 && args[1] == "--wi-profile" {
            let pid: i64 = args[2].parse().unwrap_or(0);
            let app_name = args[3].clone();
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            match window_intelligence::WindowIntelligenceStore::run_profile_subprocess(pid, app_name) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = (pid, app_name);
                eprintln!("--wi-profile is only supported on macOS and Windows");
                std::process::exit(1);
            }
        }
        #[cfg(target_os = "macos")]
        if args.len() >= 4 && args[1] == "--wi-ax-dump" {
            let pid: i64 = args[2].parse().unwrap_or(0);
            match window_intelligence::run_ax_dump_subprocess(pid, args[3].clone()) {
                Ok(()) => std::process::exit(0),
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            }
        }
        // --wi-ax-probe / --wi-ax-send <pid> <app_name>: the pairing ladder's
        // accessibility rung (peer.rs), isolated for the same reason.
        if args.len() >= 4 && (args[1] == "--wi-ax-probe" || args[1] == "--wi-ax-send") {
            let pid: i64 = args[2].parse().unwrap_or(0);
            let app_name = args[3].clone();
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                let r = if args[1] == "--wi-ax-probe" {
                    window_intelligence::run_ax_probe_subprocess(pid, app_name)
                } else {
                    window_intelligence::run_ax_send_subprocess(pid, app_name)
                };
                match r {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = (pid, app_name);
                eprintln!("accessibility modes are only supported on macOS and Windows");
                std::process::exit(1);
            }
        }
        // --wi-read-text <pid> <app_name>: same isolation for the
        // window-content reader (FABLE A2).
        if args.len() >= 4 && args[1] == "--wi-read-text" {
            let pid: i64 = args[2].parse().unwrap_or(0);
            let app_name = args[3].clone();
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            match window_intelligence::run_read_text_subprocess(pid, app_name) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = (pid, app_name);
                eprintln!("--wi-read-text is only supported on macOS and Windows");
                std::process::exit(1);
            }
        }
    }

    // ── CLI mode ──────────────────────────────
    // If invoked with "auth" subcommand, run CLI and exit without launching GUI.
    let cli_cmd = cli::parse_args();
    match cli_cmd {
        cli::CliCommand::LaunchGui => { /* fall through to Tauri */ }
        cmd => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            let handled = rt.block_on(cli::run(cmd));
            if handled {
                std::process::exit(0);
            }
        }
    }

    // Logging: from here on this is the long-lived app process (the
    // subprocess and CLI modes above have exited). Set it up before any
    // service starts so the first warning is captured.
    init_logging();

    // ── GUI mode ──────────────────────────────
    // Adopt our explicit AppUserModelID before any window exists so the
    // taskbar and toast system attribute everything to Orbix Core (the
    // same AUMID register_toast_identity() registers with a name + icon).
    #[cfg(target_os = "windows")]
    {
        let aumid: Vec<u16> = os_auto::APP_USER_MODEL_ID
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
                windows::core::PCWSTR(aumid.as_ptr()),
            );
        }
    }

    // --headless: the full app - services, endpoint, workflows, tray -
    // with no window and no Dock icon. `orbix up` launches this so the
    // whole product runs from a terminal.
    let headless = std::env::args().any(|a| a == "--headless");
    // One instance: a running Orbix Core shows its window instead.
    if always_on::hand_over_to_running_instance(!headless) {
        eprintln!("[orbixcore] already running - handed over to it");
        std::process::exit(0);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::default())
        .manage(Mutex::new(AudioStreamHolder::default()))
        .manage(LlamaServerHolder::default())
        .manage(Arc::new(screen_capture::ScreenRecorder::default()))
        .manage(Mutex::new(ScreenAudioHolder::default()))
        .setup(move |app| {
            storage::remove_retired_account_files();

            // Register the toast identity (AUMID display name + icon) off
            // the launch path; idempotent, ~100ms of PowerShell.
            #[cfg(target_os = "windows")]
            tauri::async_runtime::spawn(async {
                if let Err(e) = os_auto::register_toast_identity().await {
                    log::warn!("[toast] AUMID registration failed: {}", e);
                }
            });

            if headless {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
                // No window means no CloseRequested: Ctrl-C / SIGTERM must
                // clean up the model servers instead.
                let sig_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    #[cfg(unix)]
                    {
                        use tokio::signal::unix::{signal, SignalKind};
                        let mut term = signal(SignalKind::terminate()).expect("signal handler");
                        tokio::select! {
                            _ = tokio::signal::ctrl_c() => {},
                            _ = term.recv() => {},
                        }
                    }
                    // Windows has no SIGTERM; Ctrl-C (and console close,
                    // which tokio maps onto it) is the shutdown signal.
                    #[cfg(not(unix))]
                    {
                        let _ = tokio::signal::ctrl_c().await;
                    }
                    eprintln!("[orbixcore] headless shutdown - cleaning up children");
                    kill_model_servers(&sig_handle);
                    std::process::exit(0);
                });
                eprintln!("[orbixcore] headless - services up, no window. Ctrl-C stops everything.");
            }
            // W0.6: load persisted stores off the launch path. The window
            // appears immediately; stores fill in milliseconds later and
            // the frontend refreshes on data-changed.
            {
                let hydrate_handle = app.handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let state = hydrate_handle.state::<AppState>();
                    state.hydrate();
                    let _ = crate::event_bus::emit_ui(&hydrate_handle, "data-changed", "hydrated");
                });
                // W2: resume any Runs that were mid-flight at last shutdown,
                // once the stores are hydrated. Runs on the async runtime so
                // the resumed workflows can await tool/model calls.
                let resume_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    let state = resume_handle.state::<AppState>();
                    commands::workflows_cmd::resume_interrupted_runs(&state, &resume_handle).await;
                });
            }

            // Keep the Mac awake while Orbix runs - agents and scheduled
            // workflows must survive idle. Display may still sleep.
            power::keep_awake();

            // Background reader: Bob always knows today's calendar
            // (Calendar + Outlook) without any model tool call.
            run_log::init(app.handle().clone());
            today::spawn_refresher();

            // ── Native macOS menu bar: App / File / Edit / View / Window / Help.
            // Custom ids are prefixed "menu-" and routed to the frontend as a
            // single "menu-action" event; predefined items (copy/paste,
            // minimize, fullscreen) are handled by the OS.
            {
                use tauri::menu::{AboutMetadata, SubmenuBuilder};
                let settings = MenuItemBuilder::with_id("menu-settings", "Settings…")
                    .accelerator("Cmd+,").build(app)?;
                let app_menu = SubmenuBuilder::new(app, "Orbix Core")
                    .about(Some(AboutMetadata::default()))
                    .separator()
                    .item(&settings)
                    .separator()
                    .hide().hide_others().show_all()
                    .separator()
                    .quit()
                    .build()?;

                let new_chat = MenuItemBuilder::with_id("menu-new-chat", "New Chat")
                    .accelerator("Cmd+N").build(app)?;
                let documents = MenuItemBuilder::with_id("menu-documents", "Documents").build(app)?;
                let file_menu = SubmenuBuilder::new(app, "File")
                    .item(&new_chat)
                    .item(&documents)
                    .separator()
                    .close_window()
                    .build()?;

                let edit_menu = SubmenuBuilder::new(app, "Edit")
                    .undo().redo()
                    .separator()
                    .cut().copy().paste().select_all()
                    .build()?;

                let agents = MenuItemBuilder::with_id("menu-agents", "Agents").build(app)?;
                let workflows_item = MenuItemBuilder::with_id("menu-dashboard", "Dashboard").build(app)?;
                let my_ui = MenuItemBuilder::with_id("menu-my-ui", "My UI").build(app)?;
                let audit_item = MenuItemBuilder::with_id("menu-audit", "Audit Log").build(app)?;
                let view_menu = SubmenuBuilder::new(app, "View")
                    .item(&agents)
                    .item(&workflows_item)
                    .item(&my_ui)
                    .item(&audit_item)
                    .separator()
                    .fullscreen()
                    .build()?;

                let window_menu = SubmenuBuilder::new(app, "Window")
                    .minimize().maximize()
                    .separator()
                    .close_window()
                    .build()?;

                let guide = MenuItemBuilder::with_id("menu-guide", "How to Orbix").build(app)?;
                let site = MenuItemBuilder::with_id("menu-website", "Orbix Core Website").build(app)?;
                let help_menu = SubmenuBuilder::new(app, "Help")
                    .item(&guide)
                    .item(&site)
                    .build()?;

                let bar = MenuBuilder::new(app)
                    .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu])
                    .build()?;
                app.set_menu(bar)?;

                app.on_menu_event(|app, event| {
                    let id = event.id().as_ref();
                    let Some(action) = id.strip_prefix("menu-") else { return };
                    match action {
                        "website" => {
                            use tauri_plugin_shell::ShellExt;
                            app.shell().open("https://orbixcore.ai", None).ok();
                        }
                        _ => {
                            if let Some(win) = app.get_webview_window("main") {
                                win.show().ok();
                                win.set_focus().ok();
                            }
                            crate::event_bus::emit_ui(app, "menu-action", action).ok();
                        }
                    }
                });
            }

            // System tray - OrbixCore stays alive in menu bar when window closed
            let show = MenuItemBuilder::with_id("show", "Show OrbixCore").build(app)?;
            let stop_rec = MenuItemBuilder::with_id("stop_recording", "Stop Recording")
                .enabled(false)
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit OrbixCore").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show)
                .separator()
                .item(&stop_rec)
                .separator()
                .item(&quit)
                .build()?;

            // Store the stop_recording menu item for dynamic enable/disable
            app.manage(TrayStopRecItem(Mutex::new(stop_rec)));

            // ── Phase 2: spawn supervised Deno subprocess per active module
            // now that tokio runtime is up. AppState was managed earlier so
            // pull the supervisor from there. ──
            {
                let state: tauri::State<'_, AppState> = app.state();
                let loaded = connector_manifest::load_all_active(
                    &connector_modules::modules_dir(),
                );
                if loaded.is_empty() {
                    eprintln!("[connector_runtime] no modules to supervise yet");
                } else {
                    eprintln!(
                        "[connector_runtime] supervising {} module(s): {:?}",
                        loaded.len(),
                        loaded.iter().map(|m| &m.slug).collect::<Vec<_>>(),
                    );
                    state.connect.connector_runtime.install_modules(loaded);
                }

            }

            let tray_icon = Image::from_path("icons/32x32.png")
                .unwrap_or_else(|_| Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("tray icon"));

            TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .tooltip("OrbixCore")
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "show" => always_on::show_main_window(app),
                        "stop_recording" => {
                            // Stop screen recording from tray
                            let recorder = app.state::<Arc<screen_capture::ScreenRecorder>>();
                            if recorder.active.load(std::sync::atomic::Ordering::SeqCst) {
                                let audio_holder = app.state::<Mutex<ScreenAudioHolder>>();
                                audio_holder.lock().unwrap_or_else(|e| e.into_inner()).0 = None;
                                recorder.stop();
                                crate::event_bus::emit_ui(app, "orb-state-change", OrbState::Idle).ok();
                                // Emit updated recording state so frontend syncs
                                let state = recorder.get_state();
                                crate::event_bus::emit_ui(app, "screen-recording-state", state).ok();
                                // Disable the menu item
                                if let Some(item) = app.try_state::<TrayStopRecItem>() {
                                    item.0.lock().unwrap_or_else(|e| e.into_inner()).set_enabled(false).ok();
                                }
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event {
                        always_on::show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // The quick bar's global shortcut (never fails the launch).
            quick_bar::init(app.handle());

            // Start the local HTTP server (0.0.0.0:51231): the OpenAI-compatible
            // /v1 endpoint for other devices and the /cli bridge routes.
            let mobile_state = mobile_api::MobileApiState {
                app_handle: app.handle().clone(),
            };
            tauri::async_runtime::spawn(async move {
                mobile_api::start_mobile_api(mobile_state).await;
            });

            // Advertise this host on the LAN if the endpoint was left on, so
            // `orbix connect` from another device finds it without an IP.
            {
                let ep = endpoint::EndpointSettings::load();
                if ep.enabled {
                    let model = storage::load_json::<String>("selected_model.json").ok();
                    discovery::set_advertising(true, model.as_deref());
                }
            }

            // ── File-watch trigger tick (W2.6) ──
            // FileWatch triggers carried a workflow_id but nothing ever
            // polled them, so they never fired. This tick snapshots each
            // enabled watch every 15s and fires its workflow on a matching
            // change. The per-watch baseline lives in memory (a fresh
            // baseline after restart is correct: we only fire on changes we
            // actually observe, never on stale pre-restart state).
            {
                let fw_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Manager;
                    use std::collections::HashMap;
                    let mut last: HashMap<String, triggers::FileSnapshot> = HashMap::new();
                    let mut tick = tokio::time::interval(tokio::time::Duration::from_secs(15));
                    loop {
                        tick.tick().await;
                        let st = fw_handle.state::<AppState>();
                        let watches = {
                            let store = st.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                            store.file_watches.clone()
                        };
                        for w in watches.iter().filter(|w| w.enabled) {
                            let cur = triggers::snapshot_path(&w.path);
                            let prev = last.get(&w.id);
                            let fired = triggers::file_change_detected(&w.event_filter, prev, &cur);
                            last.insert(w.id.clone(), cur);
                            if fired && !w.workflow_id.is_empty() {
                                let entry = triggers::EventLogEntry::new(
                                    triggers::TriggerSource::FileWatch,
                                    w.path.clone(),
                                    format!("File watch '{}' fired workflow", w.name),
                                );
                                {
                                    let mut store = st.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                                    store.event_log.push(entry);
                                    store.save();
                                }
                                let _ = commands::workflows_cmd::run_workflow_core(
                                    &w.workflow_id, &st, &fw_handle,
                                ).await;
                            }
                        }
                    }
                });
            }

            // ── Unified dispatcher (W2.3) ──
            // One scheduler, one queue. Replaces the three prior loops
            // (cron tick, job worker, job-profile cadence): the dispatcher
            // detects due cron schedules and job-profile cadences, enqueues
            // each into the single core_job_scheduler queue, and drains it
            // through the Run engine / connector bus with retry + dead-letter.
            dispatcher::spawn(app.handle().clone());

            // ── Proactivity (W3): run the suggestion generators on an
            // interval and surface new suggestions through Bob. ──
            proactivity::spawn(app.handle().clone());
            agent_shifts::spawn(app.handle().clone());

            // File watcher - reload app data when external tools (Bob) modify JSON files
            {
                use notify::{Watcher, RecursiveMode, Event, EventKind, event::ModifyKind};
                let app_handle = app.handle().clone();
                let watch_dir = storage::data_dir();
                let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        if matches!(event.kind, EventKind::Modify(ModifyKind::Data(_))) {
                            let dominated = event.paths.iter().any(|p| {
                                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                matches!(name, "agents.json" | "workflows.json" | "pipeline.json")
                            });
                            if dominated {
                                // Reload in-memory state from disk
                                if let Some(state) = app_handle.try_state::<AppState>() {
                                    let new_agents = storage::load_agents().unwrap_or_default();
                                    if let Ok(mut agents) = state.chat.agents.lock() {
                                        *agents = new_agents;
                                    }
                                    let new_ws = WorkflowStore::load();
                                    if let Ok(mut ws) = state.automation.workflow_store.lock() {
                                        ws.workflows = new_ws.workflows;
                                    }
                                }
                                crate::event_bus::emit_ui(&app_handle, "data-changed", "file-watcher").ok();
                            }
                        }
                    }
                }).expect("file watcher");
                watcher.watch(&watch_dir, RecursiveMode::NonRecursive).ok();
                // Leak the watcher so it lives for the app lifetime
                std::mem::forget(watcher);
            }

            // ── Register preferred local model (DON'T auto-start) ──
            // The 26B-A4B is now safe on 16GB Macs thanks to IQ3_XS quant
            // + KV cache quantization + flash attention. Loads on first use
            // via ensure_model_loaded() and unloads after 3 min idle.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let models = local_llm::list_models_with_status();
                    // The user's persisted choice wins whenever that model is
                    // still on disk; the suite order is only the fallback for
                    // a fresh install or a deleted file.
                    let persisted = crate::storage::load_json::<String>("selected_model.json").ok();
                    let chosen = persisted
                        .as_deref()
                        .and_then(|id| models.iter().find(|m| m.id == id && m.downloaded))
                        .or_else(|| models.iter()
                            .find(|m| m.id == local_llm::SUITE_REASONING_ID && m.downloaded))
                        .or_else(|| models.iter()
                            .find(|m| m.id == local_llm::SUITE_AGENT_ID && m.downloaded))
                        .or_else(|| models.iter().find(|m| m.downloaded))
                        .cloned();
                    if let Some(model) = chosen {
                        if let Some(app_state) = app_handle.try_state::<AppState>() {
                            if let Ok(mut sel) = app_state.engine.selected_local_model.lock() {
                                *sel = Some(model.id.clone());
                            }
                        }
                        crate::event_bus::emit_ui(&app_handle, "local-llm-status", "registered").ok();
                    } else {
                        crate::event_bus::emit_ui(&app_handle, "local-llm-status", "no-model").ok();
                    }
                });
            }

            // ── Warm-ups (UX latency) ──
            // Pre-warm the two cold-start bottlenecks off the launch path so
            // the FIRST message/voice reply is fast. Both are idempotent and
            // the model pool still evicts after 3 min idle, so an unused warm
            // model does not leak RAM.
            {
                // Kokoro TTS engine: first synthesis loads a ~340MB model.
                // Warming it means the first spoken reply has no lag. Only if
                // already downloaded (never triggers the download here).
                let tts_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
                    if voice::kokoro_model_downloaded() {
                        let _ = voice::ensure_kokoro_engine().await;
                        let _ = crate::event_bus::emit_ui(&tts_handle, "tts-warm", "ready");
                    }
                    // Whisper STT: loads a ~140MB model into a context on first
                    // transcription. Warm it too so the first dictation has no
                    // model-load lag. Only if already downloaded.
                    if voice::whisper_model_downloaded() {
                        let _ = voice::ensure_whisper_loaded().await;
                        let _ = crate::event_bus::emit_ui(&tts_handle, "stt-warm", "ready");
                    }
                });
                // Local agent model: load the small E4B into memory with a
                // 1-token nudge so the first user message skips cold-start.
                let model_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    if ensure_model_loaded(&model_handle).await.is_ok() {
                        // Speculative cache warm: prime the server's prefix
                        // cache with Bob's ACTUAL system prompt (empty dynamic
                        // state) so even the FIRST real message reuses the
                        // cached ~2600-token prefix (98ms) instead of paying
                        // the cold ~4.5s prompt-processing. max_tokens:1 so we
                        // do not actually generate a reply.
                        let home = dirs::home_dir().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| "~".to_string());
                        let system_prompt = match model_handle.try_state::<AppState>() {
                            Some(s) => llm_engine::bob_system_message(Some(&*s), &home).await,
                            None => llm_engine::bob_system_message(None, &home).await,
                        };
                        let client = reqwest::Client::new();
                        // Same shape as a real turn - system prompt AND the
                        // tool schemas, thinking switch included - so the
                        // cached prefix is the one the first message reuses.
                        let _ = client
                            .post(format!("{}/v1/chat/completions", local_llm::llama_base_url()))
                            .json(&serde_json::json!({
                                "messages": [
                                    {"role": "system", "content": system_prompt},
                                    {"role": "user", "content": "hi"}
                                ],
                                "tools": crate::tool_schemas::bob_tools(),
                                "tool_choice": "auto",
                                "chat_template_kwargs": { "enable_thinking": llm_engine::thinking_enabled() },
                                "max_tokens": 1,
                                "cache_prompt": true,
                            }))
                            .send()
                            .await;
                        let _ = crate::event_bus::emit_ui(&model_handle, "local-llm-status", "warm");
                    }
                });
            }

            // ── Model pool eviction loop ──
            // Runs every 30s. Unloads the llama-server process after 3 min
            // of idle so the user's Mac never leaks RAM sitting idle.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    model_pool_eviction_loop(app_handle).await;
                });
            }

            Ok(())
        })
        .invoke_handler(handler::invoke_handler())
        .on_window_event(|window, event| {
            // Closing the window (red button) reaps children immediately so
            // the app doesn't hang on orphan processes.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Bob stays on: closing the main window hides it (menu bar
                // only) unless the user turned that off in Settings.
                if window.label() == "main" && always_on::settings().keep_running {
                    api.prevent_close();
                    always_on::hide_main_window(&window.app_handle().clone());
                    return;
                }
                eprintln!("[orbixcore] CloseRequested - cleaning up children");
                kill_model_servers(&window.app_handle().clone());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building OrbixCore application")
        .run(|app_handle, event| {
            // Tray Quit, Dock Quit, and app.exit() all emit ExitRequested but
            // never CloseRequested - without this the model servers survived
            // the app and kept multiple GB of RAM. kill_model_servers uses
            // .take() on the child handles, so overlapping with the window
            // path is a harmless no-op.
            // Opening the app again (Dock, Finder, Spotlight) while Bob runs
            // in the background shows the window.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                always_on::show_main_window(app_handle);
                return;
            }
            if let tauri::RunEvent::ExitRequested { .. } = event {
                eprintln!("[orbixcore] ExitRequested - cleaning up children");
                kill_model_servers(app_handle);
            }
        });
}

/// Set up logging for the persistent app process.
///
/// A Finder-launched `.app` has no terminal, so its stderr - env_logger
/// records, ~150 `eprintln!` diagnostics, and any panic message under
/// `panic = "abort"` - goes to /dev/null and support has nothing to ask for.
/// When stderr is not a terminal we redirect fd 2 to a rotating log file in
/// the app data dir; when it IS a terminal (dev `cargo run`, or `orbix up` in
/// a shell) we leave it alone so live output still works. env_logger then
/// writes to whichever stderr is now in effect.
fn init_logging() {
    use std::io::{IsTerminal, Write};

    if !std::io::stderr().is_terminal() {
        let dir = storage::base_dir().join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("orbix.log");
        // Keep one previous log; rotate when the live one passes ~5 MB so it
        // can't grow without bound across long-running sessions.
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > 5 * 1024 * 1024 {
                let _ = std::fs::rename(&path, dir.join("orbix.log.1"));
            }
        }
        if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            #[cfg(unix)]
            {
                use std::os::unix::io::AsRawFd;
                // dup2 makes fd 2 refer to the same open file; the original
                // `file` fd can then close safely without affecting stderr.
                unsafe { libc::dup2(file.as_raw_fd(), 2); }
            }
            let _ = writeln!(
                std::io::stderr(),
                "\n=== orbix session start {} ===",
                chrono::Utc::now().to_rfc3339()
            );
        }
    }

    // Honor RUST_LOG when set; default to info so a shipped build still records
    // warnings and the endpoint/port lines that were previously discarded.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .try_init()
        .ok();
}

/// Kill every llama-server child (reasoning + agent) plus any strays.
/// Shared by the windowed quit path (CloseRequested) and the headless
/// signal handler, so neither exit leaves orphan model servers behind.
fn kill_model_servers(app: &tauri::AppHandle) {
    if let Some(llama) = app.try_state::<LlamaServerHolder>() {
        // Main reasoning server
        if let Ok(mut proc) = llama.process.try_lock() {
            if let Some(mut child) = proc.child.take() {
                let _ = child.start_kill();
            }
        }
        // Agent server (usually dormant now, but kill just in case)
        if let Ok(mut proc) = llama.agent_process.try_lock() {
            if let Some(mut child) = proc.child.take() {
                let _ = child.start_kill();
            }
        }
    }
    // Sweep any stray server we started (an orphan reparented to launchd
    // after a prior crash), matched by our data-dir path so we never kill
    // another app's llama-server - LM Studio, a user's own llama.cpp, etc.
    crate::local_llm::kill_our_llama_servers();
}
