//! Application state: AppState and runtime holders, split from main.rs (W0.6).

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;
use tauri::menu::MenuItem;

pub(crate) struct TrayStopRecItem(pub(crate) Mutex<MenuItem<tauri::Wry>>);

// ─────────────────────────────────────────────
//  AppState sub-states (W0.6b): cohesive groups instead of ~78 flat fields
// ─────────────────────────────────────────────

/// Conversation surface: agents, messages, activity, session cost.
pub(crate) struct ChatState {
    pub(crate) agents: Mutex<Vec<Agent>>,
    pub(crate) mcp_servers: Mutex<Vec<McpServer>>,
    pub(crate) conversations: Mutex<Vec<Message>>,
    pub(crate) conversation_id: Mutex<String>,
    pub(crate) orb_state: Mutex<OrbState>,
    pub(crate) memory: Mutex<memory::MemoryStore>,
    pub(crate) review_responses: Mutex<std::collections::HashMap<String, serde_json::Value>>,
    pub(crate) session_cost: Mutex<SessionCost>,
    pub(crate) activity_log: Mutex<Vec<AgentActivity>>,
}

/// Local model + tool execution state.
pub(crate) struct EngineState {
    pub(crate) brand_store: Mutex<crate::brand::BrandStore>,
    pub(crate) voice_settings: Mutex<voice::VoiceSettings>,
    pub(crate) recording: Arc<voice::RecordingState>,
    pub(crate) selected_local_model: Mutex<Option<String>>,
    pub(crate) tool_registry: Mutex<ToolRegistry>,
    pub(crate) browser: Mutex<BrowserEngine>,
    pub(crate) code_assistant_store: Mutex<code_assistant::CodeAssistantStore>,
    pub(crate) council_store: Mutex<agent_council::CouncilStore>,
    pub(crate) relay_store: Mutex<secure_relay::RelayStore>,
    pub(crate) workspace: Arc<Mutex<workspace::WorkspaceSettings>>,
}

/// Workflows, schedules, jobs, and extensibility stores.
pub(crate) struct AutomationState {
    pub(crate) workflow_store: Mutex<WorkflowStore>,
    pub(crate) run_store: Mutex<crate::run::RunStore>,
    pub(crate) template_store: Mutex<workflow_templates::TemplateStore>,
    pub(crate) trigger_store: Mutex<triggers::TriggerStore>,
    pub(crate) pipeline_store: Mutex<pipeline::BoardStore>,
    pub(crate) scheduler_store: Mutex<scheduler::SchedulerStore>,
    pub(crate) orchestrator_store: Mutex<orchestrator::OrchestratorStore>,
    pub(crate) suggestion_store: Mutex<suggestions::SuggestionStore>,
    pub(crate) job_profiles: Arc<Mutex<job_profile::JobProfileStore>>,
    pub(crate) review_queue: Arc<Mutex<review_queue::ReviewQueue>>,
    pub(crate) preset_store: Mutex<presets::PresetStore>,
    pub(crate) shortcuts_store: Mutex<shortcuts::ShortcutsStore>,
    pub(crate) plugin_store: Mutex<plugins::PluginStore>,
    pub(crate) dynamic_panels_store: Mutex<dynamic_panels::DynamicPanelsStore>,
}

/// Connectors, sockets, and outside-world integration state.
pub(crate) struct ConnectState {
    pub(crate) connector_instances: Mutex<Vec<connectors::ConnectorInstance>>,
    pub(crate) universal_connectors: Mutex<Vec<connectors::UniversalConnector>>,
    pub(crate) connector_bus: Arc<connector_bus::ConnectorBus>,
    pub(crate) connector_runtime: Arc<connector_runtime::RuntimeSupervisor>,
    pub(crate) data_control: Mutex<data_control::DataControlStore>,
    pub(crate) window_intelligence_store: Mutex<window_intelligence::WindowIntelligenceStore>,
    pub(crate) share_store: Mutex<sharing::ShareStore>,
    pub(crate) mobile_notifications: Mutex<Vec<serde_json::Value>>,
    pub(crate) ws_broadcast: tokio::sync::broadcast::Sender<String>,
    pub(crate) notification_store: Mutex<notifications::NotificationStore>,
    pub(crate) kb_store: Mutex<knowledge::KBStore>,
    pub(crate) search_index: Mutex<search::SearchIndex>,
}

/// Device fleet management stores.
pub(crate) struct FleetState {
    pub(crate) fleet_store: Mutex<fleet::FleetStore>,
    pub(crate) fleet_routing_store: Mutex<fleet_routing::FleetRoutingStore>,
    pub(crate) fleet_monitor_store: Mutex<fleet_monitor::FleetMonitorStore>,
    pub(crate) playbook_store: Mutex<playbook::PlaybookStore>,
    pub(crate) fleet_perf_store: Mutex<fleet_perf::FleetPerfStore>,
    pub(crate) audit_store: Mutex<audit::AuditStore>,
    pub(crate) fleet_auto_store: Mutex<fleet_auto::FleetAutoStore>,
    pub(crate) fleet_report_store: Mutex<fleet_reports::FleetReportStore>,
    pub(crate) fleet_tag_store: Mutex<fleet_tags::FleetTagStore>,
    pub(crate) fleet_sla_store: Mutex<fleet_sla::FleetSlaStore>,
    pub(crate) fleet_timeline_store: Mutex<fleet_timeline::FleetTimelineStore>,
    pub(crate) fleet_group_store: Mutex<fleet_groups::FleetGroupStore>,
    pub(crate) fleet_incident_store: Mutex<fleet_incidents::FleetIncidentStore>,
    pub(crate) fleet_maintenance_store: Mutex<fleet_maintenance::FleetMaintenanceStore>,
    pub(crate) fleet_command_store: Mutex<fleet_commands::FleetCommandStore>,
    pub(crate) fleet_capacity_store: Mutex<fleet_capacity::FleetCapacityStore>,
    pub(crate) fleet_webhook_store: Mutex<fleet_webhooks::FleetWebhookStore>,
    pub(crate) fleet_runbook_store: Mutex<fleet_runbooks::FleetRunbookStore>,
    pub(crate) fleet_environment_store: Mutex<fleet_environments::FleetEnvironmentStore>,
}

/// Architecture v2 infrastructure: bus, config, twin, audit, compliance.
pub(crate) struct CoreState {
    pub(crate) core_event_bus: Arc<tokio::sync::RwLock<event_bus::EventBus>>,
    pub(crate) core_config: Arc<config::UnifiedConfig>,
    pub(crate) core_job_scheduler: Mutex<job_scheduler::JobScheduler>,
    pub(crate) core_state_machine: Mutex<state_machine::StateMachineRegistry>,
    pub(crate) core_session_manager: Mutex<session_manager::SessionManager>,
    pub(crate) core_observability: Arc<observability::ObservabilityStore>,
    pub(crate) core_gateway: Mutex<gateway::RequestRouter>,
    pub(crate) event_ledger: Mutex<event_ledger::EventLedger>,
    pub(crate) audit_log: Arc<audit_log::AuditLogState>,
    pub(crate) compliance: Mutex<compliance::CompliancePolicy>,
    pub(crate) twin_store: Mutex<semantic_twin::TwinStore>,
    pub(crate) preflight_store: Mutex<semantic_twin::PreflightStore>,
}

/// User profile: role, onboarding.
pub(crate) struct ProfileState {
    pub(crate) onboarding: Mutex<UserOnboarding>,
}

pub(crate) struct AppState {
    pub(crate) chat: ChatState,
    pub(crate) engine: EngineState,
    pub(crate) automation: AutomationState,
    pub(crate) connect: ConnectState,
    pub(crate) fleet: FleetState,
    pub(crate) core: CoreState,
    pub(crate) profile: ProfileState,
}

impl Default for AppState {
    fn default() -> Self {
        // Cheap construction only: channels, Arcs, and tiny single-file
        // reads. All store loads happen in hydrate() off the launch path
        // (W0.6) so the window appears immediately.
        let (ws_tx, _) = tokio::sync::broadcast::channel::<String>(256);

        // ── Audit log (shared with the connector bus). Built up-front so the
        // bus and the AppState field both hold an Arc clone, recording into the
        // same hash chain. ──
        let audit_log_arc: Arc<audit_log::AuditLogState> = {
            let workspace_id = std::fs::read_to_string(storage::data_dir().join("workspace.id"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            let mut log = audit_log::AuditLog::load_or_init(&workspace_id);
            log.append(
                "system",
                audit_log::AuditKind::PolicyLoaded,
                serde_json::json!({"reason": "app startup"}),
                None,
            );
            // Phase 1 of CONNECTORS.md: ensure the modules dir exists,
            // scan it, and anchor a row so the chain records every boot's
            // installed connector inventory.
            let _ = connector_modules::ensure_modules_dir();
            let slugs = connector_modules::list_module_slugs();
            log.append(
                "system",
                audit_log::AuditKind::ConnectorModulesScanned,
                serde_json::json!({"count": slugs.len(), "slugs": slugs}),
                None,
            );
            Arc::new(Mutex::new(log))
        };

        // ── Connector bus + in-process echo backend ──
        let connector_bus_arc: Arc<connector_bus::ConnectorBus> = {
            let bus = connector_bus::ConnectorBus::new(audit_log_arc.clone());
            let echo = Arc::new(connector_bus::InProcessBackend::new());
            echo.register("echo", "ping", |inputs| async move {
                connector_bus::ConnectorResult::ok(serde_json::json!({
                    "echoed": inputs,
                    "pong": true,
                }))
            });
            bus.register_backend(echo);
            Arc::new(bus)
        };

        // ── Connector runtime (Phase 2): registered as a bus backend now,
        // module subprocesses are spawned in .setup() once Tauri's tokio
        // runtime is up (tokio::spawn requires an active runtime). ──
        let connector_runtime_arc: Arc<connector_runtime::RuntimeSupervisor> = {
            // Bundled Deno sidecar path is wired in a follow-up; for now
            // None means the supervisor falls back to `deno` on PATH.
            let supervisor = Arc::new(connector_runtime::RuntimeSupervisor::new(None));
            connector_bus_arc.register_backend(supervisor.clone());
            supervisor
        };

        Self {
            chat: ChatState {
                    agents: Mutex::new(Vec::new()),
                    mcp_servers: Mutex::new(Vec::new()),
                    conversations: Mutex::new(Vec::new()),
                    conversation_id: Mutex::new(Uuid::new_v4().to_string()),
                    orb_state: Mutex::new(OrbState::Idle),
                    memory: Mutex::new(memory::MemoryStore::default()),
                    review_responses: Mutex::new(std::collections::HashMap::new()),
                    session_cost: Mutex::new(SessionCost::default()),
                    activity_log: Mutex::new(Vec::new()),
            },
            engine: EngineState {
                    brand_store: Mutex::new(crate::brand::BrandStore::load()),
                    voice_settings: Mutex::new(voice::VoiceSettings::default()),
                    recording: Arc::new(voice::RecordingState::default()),
                    selected_local_model: Mutex::new(None),
                    tool_registry: Mutex::new(ToolRegistry::default()),
                    browser: Mutex::new(BrowserEngine::default()),
                    code_assistant_store: Mutex::new(code_assistant::CodeAssistantStore::default()),
                    council_store: Mutex::new(agent_council::CouncilStore::default()),
                    relay_store: Mutex::new(secure_relay::RelayStore::default()),
                    workspace: Arc::new(Mutex::new(workspace::WorkspaceSettings::default())),
            },
            automation: AutomationState {
                    workflow_store: Mutex::new(WorkflowStore::default()),
            run_store: Mutex::new(crate::run::RunStore::default()),
                    template_store: Mutex::new(workflow_templates::TemplateStore::default()),
                    trigger_store: Mutex::new(triggers::TriggerStore::default()),
                    pipeline_store: Mutex::new(pipeline::BoardStore::default()),
                    scheduler_store: Mutex::new(scheduler::SchedulerStore::default()),
                    orchestrator_store: Mutex::new(orchestrator::OrchestratorStore::default()),
                    suggestion_store: Mutex::new(suggestions::SuggestionStore::default()),
                    job_profiles: Arc::new(Mutex::new(job_profile::JobProfileStore::default())),
                    review_queue: Arc::new(Mutex::new(review_queue::ReviewQueue::default())),
                    preset_store: Mutex::new(presets::PresetStore::default()),
                    shortcuts_store: Mutex::new(shortcuts::ShortcutsStore::default()),
                    plugin_store: Mutex::new(plugins::PluginStore::default()),
                    dynamic_panels_store: Mutex::new(dynamic_panels::DynamicPanelsStore::default()),
            },
            connect: ConnectState {
                    connector_instances: Mutex::new(Vec::new()),
                    universal_connectors: Mutex::new(Vec::new()),
                    connector_bus: connector_bus_arc.clone(),
                    connector_runtime: connector_runtime_arc.clone(),
                    data_control: Mutex::new(data_control::DataControlStore::default()),
                    window_intelligence_store: Mutex::new(window_intelligence::WindowIntelligenceStore::default()),
                    share_store: Mutex::new(sharing::ShareStore::default()),
                    mobile_notifications: Mutex::new(Vec::new()),
                    ws_broadcast: ws_tx,
                    notification_store: Mutex::new(notifications::NotificationStore::default()),
                    kb_store: Mutex::new(knowledge::KBStore::default()),
                    search_index: Mutex::new(search::SearchIndex::default()),
            },
            fleet: FleetState {
                    fleet_store: Mutex::new(fleet::FleetStore::default()),
                    fleet_routing_store: Mutex::new(fleet_routing::FleetRoutingStore::default()),
                    fleet_monitor_store: Mutex::new(fleet_monitor::FleetMonitorStore::default()),
                    playbook_store: Mutex::new(playbook::PlaybookStore::default()),
                    fleet_perf_store: Mutex::new(fleet_perf::FleetPerfStore::default()),
                    audit_store: Mutex::new(audit::AuditStore::default()),
                    fleet_auto_store: Mutex::new(fleet_auto::FleetAutoStore::default()),
                    fleet_report_store: Mutex::new(fleet_reports::FleetReportStore::default()),
                    fleet_tag_store: Mutex::new(fleet_tags::FleetTagStore::default()),
                    fleet_sla_store: Mutex::new(fleet_sla::FleetSlaStore::default()),
                    fleet_timeline_store: Mutex::new(fleet_timeline::FleetTimelineStore::default()),
                    fleet_group_store: Mutex::new(fleet_groups::FleetGroupStore::default()),
                    fleet_incident_store: Mutex::new(fleet_incidents::FleetIncidentStore::default()),
                    fleet_maintenance_store: Mutex::new(fleet_maintenance::FleetMaintenanceStore::default()),
                    fleet_command_store: Mutex::new(fleet_commands::FleetCommandStore::default()),
                    fleet_capacity_store: Mutex::new(fleet_capacity::FleetCapacityStore::default()),
                    fleet_webhook_store: Mutex::new(fleet_webhooks::FleetWebhookStore::default()),
                    fleet_runbook_store: Mutex::new(fleet_runbooks::FleetRunbookStore::default()),
                    fleet_environment_store: Mutex::new(fleet_environments::FleetEnvironmentStore::default()),
            },
            core: CoreState {
                    core_event_bus: Arc::new(tokio::sync::RwLock::new(event_bus::EventBus::default())),
                    core_config: Arc::new(config::UnifiedConfig::new()),
                    core_job_scheduler: Mutex::new(job_scheduler::JobScheduler::default()),
                    core_state_machine: Mutex::new(state_machine::StateMachineRegistry::default()),
                    core_session_manager: Mutex::new(session_manager::SessionManager::default()),
                    core_observability: Arc::new(observability::ObservabilityStore::default()),
                    core_gateway: Mutex::new(gateway::RequestRouter::default()),
                    event_ledger: Mutex::new(event_ledger::EventLedger::default()),
                    audit_log: audit_log_arc.clone(),
                    compliance: Mutex::new({
                        // First-run bootstrap: stable workspace UUID; owner email
                        // gets back-filled when the user activates a license.
                        let workspace_id = {
                            let path = storage::data_dir().join("workspace.id");
                            if let Ok(s) = std::fs::read_to_string(&path) {
                                let trimmed = s.trim().to_string();
                                if !trimmed.is_empty() { trimmed }
                                else {
                                    let id = uuid::Uuid::new_v4().to_string();
                                    let _ = std::fs::create_dir_all(storage::data_dir());
                                    let _ = crate::storage::atomic_write(&path, &id);
                                    id
                                }
                            } else {
                                let id = uuid::Uuid::new_v4().to_string();
                                let _ = std::fs::create_dir_all(storage::data_dir());
                                let _ = crate::storage::atomic_write(&path, &id);
                                id
                            }
                        };
                        let owner = std::env::var("ORBIX_OWNER_EMAIL").unwrap_or_else(|_| "owner@local".into());
                        let policy = compliance::CompliancePolicy::load_or_default(&workspace_id, &owner);
                        policy
                    }),
                    twin_store: Mutex::new(semantic_twin::TwinStore::default()),
                    preflight_store: Mutex::new(semantic_twin::PreflightStore::default()),
            },
            profile: ProfileState {
                    onboarding: Mutex::new(UserOnboarding::default()),
            },
        }
    }
}

impl AppState {
    pub(crate) fn hydrate(&self) {
        let mut agents: Vec<Agent> = storage::load_agents().unwrap_or_default();
        let mut workflow_store = WorkflowStore::load();
        if agents.is_empty() && workflow_store.workflows.is_empty() {
            crate::bob_core::seed_starter_pack(&mut agents, &mut workflow_store);
            storage::save_agents(&agents).ok();
            workflow_store.save();
            eprintln!("[seed] Pre-installed starter agents and workflows for first-launch user");
        }
        *self.chat.agents.lock().unwrap_or_else(|e| e.into_inner()) = agents;
        *self.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner()) = workflow_store;
        *self.automation.run_store.lock().unwrap_or_else(|e| e.into_inner()) = crate::run::RunStore::load();
        *self.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner()) = storage::load_mcp_servers().unwrap_or_default();
        *self.chat.memory.lock().unwrap_or_else(|e| e.into_inner()) = memory::MemoryStore::load();
        *self.automation.template_store.lock().unwrap_or_else(|e| e.into_inner()) = workflow_templates::TemplateStore::load();
        *self.profile.onboarding.lock().unwrap_or_else(|e| e.into_inner()) = storage::load_onboarding().unwrap_or_default();
        *self.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner()) = connectors::load_connector_instances().unwrap_or_default();
        *self.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner()) = connectors::load_universal_connectors();
        *self.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner()) = triggers::TriggerStore::load();
        *self.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner()) = pipeline::BoardStore::load();
        *self.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner()) = scheduler::SchedulerStore::load();
        *self.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner()) = knowledge::KBStore::load();
        *self.connect.search_index.lock().unwrap_or_else(|e| e.into_inner()) = search::SearchIndex::load();
        *self.automation.plugin_store.lock().unwrap_or_else(|e| e.into_inner()) = plugins::PluginStore::load();
        *self.connect.share_store.lock().unwrap_or_else(|e| e.into_inner()) = sharing::ShareStore::load();
        *self.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()) = presets::PresetStore::load();
        {
            let mut twin = self.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(graph) = storage::load_twin_graph() {
                twin.graph = graph;
            }
        }
        *self.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner()) = storage::load_twin_events().unwrap_or_default();
        *self.automation.job_profiles.lock().unwrap_or_else(|e| e.into_inner()) = job_profile::JobProfileStore::load();
        *self.automation.review_queue.lock().unwrap_or_else(|e| e.into_inner()) = review_queue::ReviewQueue::load();
        *self.engine.workspace.lock().unwrap_or_else(|e| e.into_inner()) = workspace::WorkspaceSettings::load();
        *self.connect.window_intelligence_store.lock().unwrap_or_else(|e| e.into_inner()) =
            window_intelligence::WindowIntelligenceStore::load().unwrap_or_default();
        *self.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner()) = secure_relay::RelayStore::load();
        *self.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner()) = dynamic_panels::DynamicPanelsStore::load();
        // Restore the user's model choice so a restart does not silently
        // fall back to the suite default (Gemma) when they last ran
        // another model (e.g. gpt-oss).
        *self.engine.selected_local_model.lock().unwrap_or_else(|e| e.into_inner()) =
            storage::load_json::<String>("selected_model.json").ok();
    }
}

// Wrapper for cpal::Stream which isn't Send+Sync
pub(crate) struct AudioStreamHolder(pub(crate) Option<cpal::Stream>);
unsafe impl Send for AudioStreamHolder {}
unsafe impl Sync for AudioStreamHolder {}

impl Default for AudioStreamHolder {
    fn default() -> Self {
        Self(None)
    }
}

// Wrapper for screen recording audio monitor stream
pub(crate) struct ScreenAudioHolder(pub(crate) Option<cpal::Stream>);
unsafe impl Send for ScreenAudioHolder {}
unsafe impl Sync for ScreenAudioHolder {}

impl Default for ScreenAudioHolder {
    fn default() -> Self {
        Self(None)
    }
}

// Wrapper for llama-server child processes (reasoning + agent)
pub(crate) struct LlamaServerHolder {
    pub(crate) process: tokio::sync::Mutex<local_llm::LlamaServerProcess>,
    pub(crate) agent_process: tokio::sync::Mutex<local_llm::LlamaServerProcess>,
    pub(crate) download_progress: Arc<local_llm::DownloadProgress>,
}

impl Default for LlamaServerHolder {
    fn default() -> Self {
        Self {
            process: tokio::sync::Mutex::new(local_llm::LlamaServerProcess::default()),
            agent_process: tokio::sync::Mutex::new(local_llm::LlamaServerProcess::default()),
            download_progress: Arc::new(local_llm::DownloadProgress::default()),
        }
    }
}


