import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  Agent, AgentInput, McpServer, McpServerInput,
  Message, OrbState, CliTemplate,
  VoiceSettings, TtsVoice,
  MemoryEntry, MemoryCategory, ConversationSummary,
  LocalModel, DownloadProgress, LocalLlmStatus,
  ToolDef, ToolCall,
  BrowserSession, BrowserActionResult,
  Workflow, WorkflowStep, WorkflowRunState, TeachingSession,
  SystemInfo, RunningApp, WindowInfo, OsActionResult,
  AppVersion,
  ScheduleEntry, SchedulePreset, ScheduleRunLog,
  AgentChain, AgentChainNode, ChainMode, ChainRunState, ChainTemplate,
  Suggestion, SuggestionPreferences,
  WorkflowTemplate, TemplateCategory,
  AgentCapabilities, RouteMatch, UnsupportedModelFile,
} from '../types';

// ── Agents ────────────────────────────────────
export const listAgents = () => invoke<Agent[]>('list_agents');
export const addAgent = (input: AgentInput) => invoke<Agent>('add_agent', { input });
export const updateAgent = (id: string, input: AgentInput) => invoke<Agent>('update_agent', { id, input });
export const removeAgent = (id: string) => invoke<void>('remove_agent', { id });
export const toggleAgent = (id: string) => invoke<Agent>('toggle_agent', { id });
export const setDefaultAgent = (id: string) => invoke<void>('set_default_agent', { id });
export const seedOrbixSystems = () => invoke<string>('seed_orbix_systems');

// ── Agent Routing (Epic 4) ─────────────────
export const updateAgentCapabilities = (agentId: string, capabilities: AgentCapabilities) =>
  invoke<import('../types').Agent>('update_agent_capabilities', { agentId, capabilities });
export const getRouteSuggestion = (message: string) =>
  invoke<RouteMatch | null>('get_route_suggestion', { message });
export const listCapabilityPresets = () =>
  invoke<[string, AgentCapabilities][]>('list_capability_presets');

// ── MCP Servers ───────────────────────────────
export const listMcpServers = () => invoke<McpServer[]>('list_mcp_servers');
export const addMcpServer = (input: McpServerInput) => invoke<McpServer>('add_mcp_server', { input });
export const removeMcpServer = (id: string) => invoke<void>('remove_mcp_server', { id });
export const toggleMcpServer = (id: string) => invoke<McpServer>('toggle_mcp_server', { id });

// ── Conversation ──────────────────────────────
export const getConversation = () => invoke<Message[]>('get_conversation');
export const sendMessage = (content: string, agentId?: string) =>
  invoke<Message>('send_message', { input: { content, agent_id: agentId ?? null } });
export const clearConversation = () => invoke<void>('clear_conversation');

// ── Orb ───────────────────────────────────────
export const getOrbState = () => invoke<OrbState>('get_orb_state');
export const setOrbState = (newState: OrbState) => invoke<void>('set_orb_state', { newState });

// ── CLI ───────────────────────────────────────
export const generateCliTemplate = (templateType: string) =>
  invoke<CliTemplate>('generate_cli_template', { templateType });

// ── Voice (Epic C) ───────────────────────────
export const speakText = (text: string) => invoke<void>('speak_text', { text });
export const stopSpeaking = () => invoke<void>('stop_speaking');
export const listTtsVoices = () => invoke<TtsVoice[]>('list_tts_voices');
export const startListening = () => invoke<void>('start_listening');
export const stopListening = () => invoke<string>('stop_listening');
export const getVoiceSettings = () => invoke<VoiceSettings>('get_voice_settings');
export const updateVoiceSettings = (settings: VoiceSettings) =>
  invoke<void>('update_voice_settings', { settings });

// ── Conversation History (Epic D) ────────────
export const listConversations = () => invoke<ConversationSummary[]>('list_conversations');
export const loadConversation = (id: string) => invoke<Message[]>('load_conversation', { id });
export const deleteConversation = (id: string) => invoke<void>('delete_conversation', { id });
export const renameConversation = (id: string, title: string) =>
  invoke<void>('rename_conversation', { id, title });
export const saveCurrentConversation = () => invoke<void>('save_current_conversation');
export const exportConversation = (id?: string) => invoke<string>('export_conversation', { id: id ?? null });

// ── Memory (Epic D) ─────────────────────────
export const listMemories = () => invoke<MemoryEntry[]>('list_memories');
export const addMemory = (key: string, value: string, category: MemoryCategory) =>
  invoke<MemoryEntry>('add_memory', { key, value, category });
export const updateMemory = (id: string, value: string) =>
  invoke<MemoryEntry>('update_memory', { id, value });
export const removeMemory = (id: string) => invoke<void>('remove_memory', { id });
export const searchMemories = (query: string) => invoke<MemoryEntry[]>('search_memories', { query });
export const getMemoryContext = () => invoke<string>('get_memory_context');
export const clearAllMemories = () => invoke<void>('clear_all_memories');
export const saveCompanyContext = (context: Record<string, string>) =>
  invoke<MemoryEntry[]>('save_company_context', { context });
export const getCompanyContext = () =>
  invoke<Record<string, string>>('get_company_context');

// ── Permissions ──────────────────────────────
export const checkAllPermissions = () => invoke<{ accessibility: boolean; full_disk_access: boolean; notifications: boolean; local_network: boolean }>('check_all_permissions');
export const openSystemPreferences = (pane: string) => invoke<void>('open_system_preferences', { pane });

// ── Write destinations (bring-your-own-database) ──
export interface DestColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  primary_key: boolean;
}
export interface DestSchema {
  target: string;
  columns: DestColumn[];
}
export interface DestWriteOutcome {
  written: number;
  ids: string[];
  errors: string[];
}
export const destinationDescribe = (spec: unknown) =>
  invoke<DestSchema>('destination_describe', { spec });
export const destinationTestWrite = (spec: unknown, rows: unknown[]) =>
  invoke<DestWriteOutcome>('destination_test_write', { spec, rows });

// ── App bridges (Work on top of apps, FABLE A4) ──
export interface AppBridgeStatus {
  app: string;
  display_name: string;
  status: 'ready' | 'limited' | 'not_running' | 'not_installed' | 'error';
  detail: string;
  actions: [string, string][];
}
export interface AutomationStatus {
  granted: boolean;
  state: 'granted' | 'denied' | 'undetermined' | 'unknown';
  message: string;
}
export const appBridgeList = () => invoke<AppBridgeStatus[]>('app_bridge_list');
export const automationCheck = () => invoke<AutomationStatus>('automation_check');
export const automationRequest = () => invoke<AutomationStatus>('automation_request');

// ── Local endpoint (FABLE B1) ──
export interface EndpointInfo {
  enabled: boolean;
  token: string;
  base_url: string;
  lan_url: string | null;
}
export const endpointGet = () => invoke<EndpointInfo>('endpoint_get');
export const endpointSet = (enabled: boolean) => invoke<EndpointInfo>('endpoint_set', { enabled });
export const endpointRotateToken = () => invoke<EndpointInfo>('endpoint_rotate_token');

// ── System Scan ──────────────────────────────
export const scanSystemForModels = () => invoke<any>('scan_system_for_models');

// ── Data Control (Confidential Folders) ──────
export const listConfidentialFolders = () => invoke<any[]>('list_confidential_folders');
export const addConfidentialFolder = (name: string, path: string, allowedAgents: string[], accessLevel: string) =>
  invoke<any>('add_confidential_folder', { name, path, allowedAgents, accessLevel });
export const updateConfidentialFolder = (id: string, name?: string, allowedAgents?: string[], accessLevel?: string) =>
  invoke<any>('update_confidential_folder', { id, name: name ?? null, allowedAgents: allowedAgents ?? null, accessLevel: accessLevel ?? null });
export const removeConfidentialFolder = (id: string) => invoke<boolean>('remove_confidential_folder', { id });
export const listFolderContents = (folderId: string) => invoke<any>('list_folder_contents', { folderId });
export const getFolderAccessLog = (folderId?: string) => invoke<any[]>('get_folder_access_log', { folderId: folderId ?? null });
export const pickFolder = () => invoke<string>('pick_folder');

// ── Local LLM ────────────────────────────────
export const listLocalModels = () => invoke<LocalModel[]>('list_local_models');
export interface ModelGuidance {
  total_gb: number;
  usable_gb: number;
  comfortable_model_gb: number;
  recommended_id: string | null;
  recommended_name: string | null;
  recommended_size_bytes: number | null;
  recommended_downloaded: boolean;
}
export const modelGuidance = () => invoke<ModelGuidance>('model_guidance');
export const downloadModel = (modelId: string) => invoke<void>('download_model', { modelId });
export const deleteLocalModel = (modelId: string) => invoke<void>('delete_local_model', { modelId });
export const listUnsupportedModelFiles = () => invoke<UnsupportedModelFile[]>('list_unsupported_model_files');
export const deleteUnsupportedModelFile = (filename: string) => invoke<void>('delete_unsupported_model_file', { filename });
export interface HfGgufFile {
  filename: string;
  size_bytes: number;
  quant: string;
  url: string;
  split: boolean;
}
/** List the GGUF files of a HuggingFace repo ("org/name" or URL) so the
 *  user can pick a quant. Token is per-request only, never stored. */
export const hfResolveRepo = (repo: string, token?: string) =>
  invoke<HfGgufFile[]>('hf_resolve_repo', { repo, token: token || null });

export const addCustomModel = (
  name: string,
  url: string,
  description?: string,
  sizeHint?: number,
) =>
  invoke<LocalModel>('add_custom_model', {
    name,
    url,
    description: description ?? null,
    sizeHint: sizeHint ?? null,
  });
export const removeCustomModel = (modelId: string) =>
  invoke<boolean>('remove_custom_model', { modelId });
export const getDownloadProgress = () => invoke<DownloadProgress>('get_download_progress');
export const selectLocalModel = (modelId: string) => invoke<void>('select_local_model', { modelId });
export const getSelectedLocalModel = () => invoke<string | null>('get_selected_local_model');
export const getLocalLlmStatus = () => invoke<LocalLlmStatus>('get_local_llm_status');
export const stopLocalLlm = () => invoke<void>('stop_local_llm');

// ── Tools (Epic E) ──────────────────────────
export const listTools = () => invoke<ToolDef[]>('list_tools');
export const updateAgentEmployee = (id: string, charter: string, grantedTools: string[], scheduleMinutes: number | null) =>
  invoke<Agent>('update_agent_employee', { id, charter, grantedTools, scheduleMinutes });
export const runAgentShift = (id: string) => invoke<string>('run_agent_shift', { id });
// ── Git repos for /repo chat binding ──
export interface RepoInfo {
  name: string;
  path: string;
  branch: string;
  last_activity?: string | null;
}
export const listGitRepos = () => invoke<RepoInfo[]>('list_git_repos');

export interface SkillMeta {
  name: string;
  description: string;
  path: string;
}
export const listSkills = () => invoke<SkillMeta[]>('list_skills');

// ── Imagine: Bob with free rein ──
export interface ScannedApp {
  app: string;
  windows: string[];
  tabs: string[];
}
export interface ImagineRun {
  run_id: string;
  seed: string | null;
  scan: ScannedApp[];
  started_at: string;
}
export interface ImagineEndedEvent {
  run_id: string;
  ok: boolean;
  stopped: boolean;
  result: string;
  error: string | null;
}
export const imagineStart = (seed?: string) =>
  invoke<ImagineRun>('imagine_start', { seed: seed?.trim() ? seed.trim() : null });
export const imagineStatus = () => invoke<ImagineRun | null>('imagine_status');
export const imagineNudge = (content: string) => invoke<void>('imagine_nudge', { content });
export const imagineStop = () => invoke<void>('imagine_stop');
export const onImagineStarted = (cb: (run: ImagineRun) => void) =>
  listen<ImagineRun>('imagine-started', (e) => cb(e.payload));
export const onImagineEnded = (cb: (e: ImagineEndedEvent) => void) =>
  listen<ImagineEndedEvent>('imagine-ended', (e) => cb(e.payload));
/** Step labels for an Imagine run ride the shift-step channel under the run id. */
export const onImagineStep = (cb: (runId: string, label: string | null) => void) =>
  listen<ShiftStepEvent>('shift-step', (e) => {
    if (e.payload.shift_id.startsWith('imagine-')) cb(e.payload.shift_id, e.payload.label);
  });
// ── Workspace profiles: multiple Orbix Cores, switch with restart ──
export interface WorkspaceProfile {
  id: string;
  name: string;
  created_at: string;
}
export const listWorkspaceProfiles = () =>
  invoke<{ profiles: WorkspaceProfile[]; active: string }>('list_workspace_profiles');
export const createWorkspaceProfile = (name: string, cloneCurrent: boolean, includeHistory: boolean) =>
  invoke<WorkspaceProfile>('create_workspace_profile', { name, cloneCurrent, includeHistory });
export const switchWorkspaceProfile = (id: string) => invoke<void>('switch_workspace_profile', { id });
export const renameWorkspaceProfile = (id: string, name: string) =>
  invoke<void>('rename_workspace_profile', { id, name });
export const deleteWorkspaceProfile = (id: string) => invoke<void>('delete_workspace_profile', { id });
// ── Shift monitor: watch, steer, stop a running shift ──
export interface ShiftSession {
  shift_id: string;
  agent_id: string;
  agent_name: string;
  started_at: string;
}
export const listActiveShifts = () => invoke<ShiftSession[]>('list_active_shifts');
export const sendToAgentShift = (shiftId: string, content: string) =>
  invoke<void>('send_to_agent_shift', { shiftId, content });
export const stopAgentShift = (shiftId: string) => invoke<void>('stop_agent_shift', { shiftId });
export const openRunMonitor = () => invoke<void>('open_run_monitor');

// ── Run log: one timeline of everything a run does (the Run monitor) ──
export interface RunLogEntry {
  seq: number;
  ts: string;
  run_id: string;
  kind: 'run' | 'step' | 'tool' | 'model' | 'egress' | 'error' | 'user';
  level: 'info' | 'ok' | 'warn' | 'error';
  text: string;
  detail?: string;
  run_kind?: string;
  status?: 'started' | 'done' | 'failed' | 'stopped';
}
export const runLogRecent = (limit = 400) => invoke<RunLogEntry[]>('run_log_recent', { limit });
export interface MonitorSettings { auto_open: boolean }
export const getMonitorSettings = () => invoke<MonitorSettings>('get_monitor_settings');
export const setMonitorSettings = (settings: MonitorSettings) =>
  invoke<void>('set_monitor_settings', { settings });
export interface AlwaysOnSettings { keep_running: boolean; launch_at_login: boolean }
export const getAlwaysOnSettings = () => invoke<AlwaysOnSettings>('get_always_on_settings');
export const setAlwaysOnSettings = (settings: AlwaysOnSettings) =>
  invoke<AlwaysOnSettings>('set_always_on_settings', { settings });
export interface QuickBarSettings { enabled: boolean; shortcut: string }
export interface QuickBarStatus { settings: QuickBarSettings; registered: boolean; error: string | null; choices: string[] }
export const quickBarAsk = (content: string) => invoke<Message>('quick_bar_ask', { content });
export const quickBarHide = () => invoke<void>('quick_bar_hide');
export const quickBarOpenMain = () => invoke<void>('quick_bar_open_main');
export const getQuickBarStatus = () => invoke<QuickBarStatus>('get_quick_bar_status');
export const setQuickBarSettings = (settings: QuickBarSettings) =>
  invoke<QuickBarStatus>('set_quick_bar_settings', { settings });
export const onRunLog = (cb: (e: RunLogEntry) => void) =>
  listen<RunLogEntry>('run-log', (e) => cb(e.payload));
export interface ShiftStartedEvent { shift_id: string; agent_id: string; agent_name: string; started_at: string }
export interface ShiftStepEvent { shift_id: string; label: string | null }
export interface ShiftUserMessageEvent { shift_id: string; content: string }
export interface ShiftEndedEvent { shift_id: string; agent_name: string; ok: boolean; stopped: boolean; report: string }
export const onShiftStarted = (cb: (data: ShiftStartedEvent) => void) =>
  listen<ShiftStartedEvent>('shift-started', (e) => cb(e.payload));
export const onShiftStep = (cb: (data: ShiftStepEvent) => void) =>
  listen<ShiftStepEvent>('shift-step', (e) => cb(e.payload));
export const onShiftUserMessage = (cb: (data: ShiftUserMessageEvent) => void) =>
  listen<ShiftUserMessageEvent>('shift-user-message', (e) => cb(e.payload));
export const onShiftEnded = (cb: (data: ShiftEndedEvent) => void) =>
  listen<ShiftEndedEvent>('shift-ended', (e) => cb(e.payload));
// ── Brand kits + branded documents ──────────
export interface GeneratedDoc {
  id: string;
  kind: string;
  title: string;
  path: string;
  spec: { title: string; subtitle?: string | null; sections: DocSection[] };
  kit_name: string;
  created_at: string;
}
export const listGeneratedDocuments = () => invoke<GeneratedDoc[]>('list_generated_documents');
export const previewGeneratedDocument = (path: string) => invoke<void>('preview_generated_document', { path });
export const openGeneratedDocument = (path: string) => invoke<void>('open_generated_document', { path });
export const revealGeneratedDocument = (path: string) => invoke<void>('reveal_generated_document', { path });
export const regenerateDocument = (id: string, kind: string, spec: { title: string; subtitle?: string; sections: DocSection[] }) =>
  invoke<GeneratedDoc>('regenerate_document', { id, kind, spec });
export const deleteGeneratedDocument = (id: string, deleteFile: boolean) =>
  invoke<void>('delete_generated_document', { id, deleteFile });

export interface BrandColor { name: string; hex: string; role: string }
export interface BrandKit {
  id: string;
  name: string;
  colors: BrandColor[];
  heading_font: string;
  body_font: string;
  tone: string;
  source_path?: string | null;
  logo_path?: string | null;
  created_at: string;
}
export interface DocSection { heading: string; body?: string; bullets?: string[] }
export const listBrandKits = () =>
  invoke<{ kits: BrandKit[]; active_id: string | null }>('list_brand_kits');
export const getActiveBrandKit = () => invoke<BrandKit>('get_active_brand_kit');
export const setActiveBrandKit = (id: string) => invoke<void>('set_active_brand_kit', { id });
export const updateBrandKit = (kit: BrandKit) => invoke<void>('update_brand_kit', { kit });
export const deleteBrandKit = (id: string) => invoke<void>('delete_brand_kit', { id });
export const getBrandLogoData = () =>
  invoke<{ mime: string; base64: string } | null>('get_brand_logo_data');
export const extractBrandKitFromFile = (path: string) =>
  invoke<BrandKit>('extract_brand_kit_from_file', { path });
export const createBrandedDocument = (kind: string, spec: { title: string; subtitle?: string; sections: DocSection[] }, openAfter = true) =>
  invoke<string>('create_branded_document', { kind, spec, openAfter });

export const listAgentGrantableTools = () =>
  invoke<{ name: string; description: string }[]>('list_agent_grantable_tools');
export const toggleTool = (toolId: string) => invoke<ToolDef>('toggle_tool', { toolId });
export const setToolAutoApprove = (toolName: string, autoApprove: boolean) =>
  invoke<void>('set_tool_auto_approve', { toolName, autoApprove });
export const getPendingToolCalls = () => invoke<ToolCall[]>('get_pending_tool_calls');
export const approveToolCall = (callId: string) => invoke<ToolCall>('approve_tool_call', { callId });
export const denyToolCall = (callId: string) => invoke<ToolCall>('deny_tool_call', { callId });
export const requestToolCall = (toolName: string, args: Record<string, unknown>) =>
  invoke<ToolCall>('request_tool_call', { toolName, arguments: args });

// ── Browser (Epic F) ────────────────────────
export const browserNavigate = (url: string) =>
  invoke<BrowserActionResult>('browser_navigate', { url });
export const browserClick = (selector: string) =>
  invoke<BrowserActionResult>('browser_click', { selector });
export const browserTypeText = (selector: string, text: string) =>
  invoke<BrowserActionResult>('browser_type_text', { selector, text });
export const browserScreenshot = () =>
  invoke<BrowserActionResult>('browser_screenshot');
export const browserGetContent = () =>
  invoke<BrowserActionResult>('browser_get_content');
export const browserEvaluate = (script: string) =>
  invoke<BrowserActionResult>('browser_evaluate', { script });
export const browserGetSession = () =>
  invoke<BrowserSession>('browser_get_session');
export const browserClose = () =>
  invoke<void>('browser_close');

// ── Workflows / Teach (Epic G) ──────────────
export const listWorkflows = () => invoke<Workflow[]>('list_workflows');
export const getWorkflow = (id: string) => invoke<Workflow>('get_workflow', { id });
export const createWorkflow = (name: string, description: string) =>
  invoke<Workflow>('create_workflow', { name, description });
export const updateWorkflow = (id: string, name?: string, description?: string) =>
  invoke<Workflow>('update_workflow', { id, name: name ?? null, description: description ?? null });
export const deleteWorkflow = (id: string) => invoke<void>('delete_workflow', { id });
export const updateWorkflowSteps = (workflowId: string, steps: WorkflowStep[]) =>
  invoke<Workflow>('update_workflow_steps', { workflowId, steps });
export interface WorkflowRunSummary {
  id: string;
  status: string;
  current_step: number;
  total_steps: number;
  error?: string | null;
  created_at: string;
  updated_at: string;
}
export const testWorkflowStep = (workflowId: string, stepId: string) =>
  invoke<{ success: boolean; output: string }>('test_workflow_step', { workflowId, stepId });
export const listWorkflowRuns = (workflowId: string, limit?: number) =>
  invoke<WorkflowRunSummary[]>('list_workflow_runs', { workflowId, limit: limit ?? null });
export const runWorkflow = (id: string) => invoke<WorkflowRunState>('run_workflow', { id });
export const stopWorkflow = (workflowId?: string) => invoke<void>('stop_workflow', { workflowId: workflowId ?? null });
export const getWorkflowStatus = () => invoke<WorkflowRunState | null>('get_workflow_status');

// ── Workflow Templates (Epic 3) ─────────────
export const listWorkflowTemplates = () => invoke<WorkflowTemplate[]>('list_workflow_templates');
export const getWorkflowTemplate = (id: string) => invoke<WorkflowTemplate>('get_workflow_template', { id });
export const listTemplateCategories = () => invoke<TemplateCategory[]>('list_template_categories');
export const instantiateTemplate = (templateId: string) => invoke<Workflow>('instantiate_template', { templateId });
export const saveWorkflowAsTemplate = (workflowId: string, input: { name: string; description: string; category: string }) =>
  invoke<WorkflowTemplate>('save_workflow_as_template', { workflowId, input });
export const deleteWorkflowTemplate = (id: string) => invoke<void>('delete_workflow_template', { id });
export const updateWorkflowStepConnector = (workflowId: string, stepId: string, connectorId: string | null, endpointKey: string | null) =>
  invoke<Workflow>('update_workflow_step_connector', { workflowId, stepId, connectorId, endpointKey });

// ── OS Automation (Epic H) ──────────────────
export const osGetSystemInfo = () => invoke<SystemInfo>('os_get_system_info');
export const osOpenApplication = (name: string) => invoke<OsActionResult>('os_open_application', { name });
export const osQuitApplication = (name: string) => invoke<OsActionResult>('os_quit_application', { name });
export const osListRunningApps = () => invoke<RunningApp[]>('os_list_running_apps');
export const osGetClipboard = () => invoke<string>('os_get_clipboard');
export const osSetClipboard = (text: string) => invoke<OsActionResult>('os_set_clipboard', { text });
export const osSendNotification = (title: string, body: string) =>
  invoke<OsActionResult>('os_send_notification', { title, body });
export const osOpenUrl = (url: string) => invoke<OsActionResult>('os_open_url', { url });
export const osOpenFile = (path: string) => invoke<OsActionResult>('os_open_file', { path });
export const osRevealInFinder = (path: string) => invoke<OsActionResult>('os_reveal_in_finder', { path });
export const osGetActiveWindow = () => invoke<WindowInfo>('os_get_active_window');
export const osListWindows = () => invoke<WindowInfo[]>('os_list_windows');
export const osRunApplescript = (script: string) => invoke<OsActionResult>('os_run_applescript', { script });

// ── App version ──────────────────────
export const getAppVersion = () => invoke<AppVersion>('get_app_version');

// ── Events ───────────────────────────────────
export const onOrbStateChange = (cb: (state: OrbState) => void) =>
  listen<OrbState>('orb-state-change', (e) => cb(e.payload));

export const onNewMessage = (cb: (msg: Message) => void) =>
  listen<Message>('new-message', (e) => cb(e.payload));

// ── Streaming events ─────────────────────────
export interface MessageChunk {
  message_id: string;
  chunk: string;
  content: string;
}

export interface MessageStreamEvent {
  message_id: string;
  agent_id?: string | null;
  content?: string;
}

export const onMessageStreamStart = (cb: (data: MessageStreamEvent) => void) =>
  listen<MessageStreamEvent>('message-stream-start', (e) => cb(e.payload));

export const onMessageChunk = (cb: (data: MessageChunk) => void) =>
  listen<MessageChunk>('message-chunk', (e) => cb(e.payload));

export const onMessageStreamEnd = (cb: (data: MessageStreamEvent) => void) =>
  listen<MessageStreamEvent>('message-stream-end', (e) => cb(e.payload));

export const onOpenConnectorSetup = (cb: (data: { connector_id: string }) => void) =>
  listen<{ connector_id: string }>('open-connector-setup', (e) => cb(e.payload));

export const onDataChanged = (cb: (reason: string) => void) =>
  listen<string>('data-changed', (e) => cb(e.payload));

// Native macOS menu bar (File/View/Help items with custom actions)
export const onMenuAction = (cb: (action: string) => void) =>
  listen<string>('menu-action', (e) => cb(e.payload));

// ── Proactivity (W3): Bob surfaces suggestions unprompted ──
export interface BobToastPayload {
  kind: string;
  title: string;
  message: string;
}
export const onBobToast = (cb: (payload: BobToastPayload) => void) =>
  listen<BobToastPayload>('bob-toast', (e) => cb(e.payload));

// ── Bob Review ────────────────────────────────
export interface BobReviewPayload {
  id: string;
  title: string;
  summary: string;
  content: string;
  type: 'created' | 'updated' | 'executed' | 'generated';
  files_changed?: string[];
}

export const onBobReview = (cb: (data: BobReviewPayload) => void) =>
  listen<BobReviewPayload>('bob-review', (e) => cb(e.payload));

export const bobReviewRespond = (id: string, action: string, edits?: string) =>
  invoke<string>('bob_review_respond', { id, action, edits });

export const bobLiteSend = (content: string) =>
  invoke<Message>('bob_lite_send', { content });

export const bobCancel = () => invoke<void>('bob_cancel');

export const prewarmLlm = () => invoke<void>('prewarm_llm');
export const voiceConversationSet = (active: boolean) =>
  invoke<void>('voice_conversation_set', { active });

export const onBobStep = (cb: (label: string | null) => void) =>
  listen<{ label: string | null }>('bob-step', (e) => cb(e.payload?.label ?? null));

export const bobSend = (content: string, images?: string[]) =>
  invoke<import('../types').Message>('bob_send', { content, images });

export const prepareSttModel = () => invoke<string>('prepare_stt_model');
export interface SttModelStatus { state: 'downloading' | 'ready' | 'error'; model: string; error?: string }
export const onSttModelStatus = (cb: (s: SttModelStatus) => void) =>
  listen<SttModelStatus>('stt-model-status', (e) => cb(e.payload));

export const onSttPartial = (cb: (text: string) => void) =>
  listen<{ text: string }>('stt-partial', (e) => cb(e.payload?.text ?? ''));

export const onSttResult = (cb: (text: string) => void) =>
  listen<string>('stt-result', (e) => cb(e.payload));

export const onTtsState = (cb: (state: string) => void) =>
  listen<string>('tts-state', (e) => cb(e.payload));

export const onAudioLevel = (cb: (level: number) => void) =>
  listen<number>('audio-level', (e) => cb(e.payload));

export const onTtsAmplitude = (cb: (level: number) => void) =>
  listen<number>('tts-amplitude', (e) => cb(e.payload));

export const requestVoicePermissions = () =>
  invoke<{ microphone: boolean; speech_recognition: boolean }>('request_voice_permissions');

export const onSilenceDetected = (cb: () => void) =>
  listen<null>('silence-detected', () => cb());

export const onModelDownloadProgress = (cb: (progress: DownloadProgress) => void) =>
  listen<DownloadProgress>('model-download-progress', (e) => {
    // Debug breadcrumb so we can confirm IPC delivery in the webview console
    // when something looks stuck. Cheap - runs ~once per 500KB on the wire.
    // eslint-disable-next-line no-console
    console.debug('[event] model-download-progress', e.payload);
    cb(e.payload);
  });

export const onLocalLlmStatus = (cb: (status: string) => void) =>
  listen<string>('local-llm-status', (e) => cb(e.payload));

export const onToolCallPending = (cb: (call: ToolCall) => void) =>
  listen<ToolCall>('tool-call-pending', (e) => cb(e.payload));

export const onToolCallUpdate = (cb: (call: ToolCall) => void) =>
  listen<ToolCall>('tool-call-update', (e) => cb(e.payload));

export const onBrowserStatusChange = (cb: (session: BrowserSession) => void) =>
  listen<BrowserSession>('browser-status-change', (e) => cb(e.payload));

export const onWorkflowStatusChange = (cb: (state: WorkflowRunState) => void) =>
  listen<WorkflowRunState>('workflow-status-change', (e) => cb(e.payload));

export interface AgentActivity {
  id: string;
  agent_name: string;
  action: string;
  status: 'start' | 'complete' | 'error';
  workflow_name: string;
  timestamp: string;
  detail?: string;
  result?: string;
  step_type?: string;
}

export const onAgentActivity = (cb: (activity: AgentActivity) => void) =>
  listen<AgentActivity>('agent-activity', (e) => cb(e.payload));
export const getActivityLog = (agentName?: string, limit?: number) =>
  invoke<AgentActivity[]>('get_activity_log', { agentName: agentName ?? null, limit: limit ?? null });
export const clearActivityLog = () => invoke<void>('clear_activity_log');

export const onTeachingSessionChange = (cb: (session: TeachingSession) => void) =>
  listen<TeachingSession>('teaching-session-change', (e) => cb(e.payload));

export const onTeachingStepAdded = (cb: (step: WorkflowStep) => void) =>
  listen<WorkflowStep>('teaching-step-added', (e) => cb(e.payload));

// ── Screen Recording (Epic K) ────────────────
export const screenCheckPermission = () => invoke<boolean>('screen_check_permission');
export const screenRequestPermission = () => invoke<void>('screen_request_permission');
export const screenStartRecording = () => invoke<string>('screen_start_recording');
export const screenStopRecording = () => invoke<Recording | null>('screen_stop_recording');
export const screenGetState = () => invoke<ScreenRecordingState>('screen_get_state');
export const screenGetPreview = () => invoke<string>('screen_get_preview');
export const screenListRecordings = () => invoke<Recording[]>('screen_list_recordings');
export const screenDeleteRecording = (id: string) => invoke<boolean>('screen_delete_recording', { id });
export const screenGetFrame = (recordingId: string, frameIndex: number) =>
  invoke<string>('screen_get_frame', { recordingId, frameIndex });

export const screenGetTimeline = (recordingId: string) =>
  invoke<ProcessStep[]>('screen_get_timeline', { recordingId });
export const screenGenerateWorkflow = (recordingId: string) =>
  invoke<string>('screen_generate_workflow', { recordingId });

export const onScreenRecordingState = (cb: (state: ScreenRecordingState) => void) =>
  listen<ScreenRecordingState>('screen-recording-state', (e) => cb(e.payload));

import type { Recording, ScreenRecordingState, ProcessStep } from '../types';

// ── Window ───────────────────────────────────
export const hideToTray = () => invoke<void>('hide_to_tray');
export const showWindow = () => invoke<void>('show_window');

// ── Role Profiles (Epic T) ──────────────────
import type { UserOnboarding } from '../types';

export const getOnboarding = () => invoke<UserOnboarding>('get_onboarding');
export const completeOnboarding = (userName: string) =>
  invoke<UserOnboarding>('complete_onboarding', { userName });

// ── Connectors (Epic U) ────────────────────
import type {
  ConnectorDefinition, ConnectorInstance, ConnectorTestResult,
  LoginAssistantMessage, ValidationResult, ConnectorId as ConnectorIdType,
} from '../types';

export const listConnectorCatalog = () => invoke<ConnectorDefinition[]>('list_connector_catalog');
export const listConnectorInstances = () => invoke<ConnectorInstance[]>('list_connector_instances');
export const createConnectorInstance = (
  connectorId: ConnectorIdType,
  name: string,
  credentials: Record<string, string>,
) => invoke<ConnectorInstance>('create_connector_instance', { connectorId, name, credentials });
export const testConnector = (instanceId: string) =>
  invoke<ConnectorTestResult>('test_connector', { instanceId });
export const removeConnector = (instanceId: string) =>
  invoke<void>('remove_connector', { instanceId });
export const getLoginHints = (connectorId: ConnectorIdType) =>
  invoke<LoginAssistantMessage[]>('get_login_hints', { connectorId });
export const askConnectorAssistant = (connectorId: ConnectorIdType, fieldKey: string, userInput: string) =>
  invoke<string>('ask_connector_assistant', { connectorId, fieldKey, userInput });
export const validateConnectorField = (connectorId: ConnectorIdType, fieldKey: string, value: string) =>
  invoke<ValidationResult>('validate_connector_field', { connectorId, fieldKey, value });

export const onConnectorStatus = (cb: (data: { instance_id: string; success: boolean; message: string }) => void) =>
  listen<{ instance_id: string; success: boolean; message: string }>('connector-status', (e) => cb(e.payload));

// ── Universal Connectors ─────────────────────
import type {
  UniversalConnector, ConnectorEndpoint, DiscoveryResult,
  UniversalAuthType, AuthConfig,
} from '../types';

export const listUniversalConnectors = () => invoke<UniversalConnector[]>('list_universal_connectors');
export const createUniversalConnector = (
  name: string, baseUrl: string, authType: UniversalAuthType,
  authConfig: AuthConfig, defaultHeaders: Record<string, string>,
) => invoke<UniversalConnector>('create_universal_connector', { name, baseUrl, authType, authConfig, defaultHeaders });
export const updateUniversalConnector = (
  id: string, name?: string, description?: string, baseUrl?: string,
  authType?: UniversalAuthType, authConfig?: AuthConfig, defaultHeaders?: Record<string, string>,
) => invoke<UniversalConnector>('update_universal_connector', {
  id, name: name ?? null, description: description ?? null, baseUrl: baseUrl ?? null,
  authType: authType ?? null, authConfig: authConfig ?? null, defaultHeaders: defaultHeaders ?? null,
});
export const deleteUniversalConnector = (id: string) => invoke<void>('delete_universal_connector', { id });
export const discoverConnectorEndpoints = (baseUrl: string) => invoke<DiscoveryResult>('discover_connector_endpoints', { baseUrl });
export const addConnectorEndpoint = (connectorId: string, endpoint: ConnectorEndpoint) =>
  invoke<void>('add_connector_endpoint', { connectorId, endpoint });
export const removeConnectorEndpoint = (connectorId: string, endpointKey: string) =>
  invoke<void>('remove_connector_endpoint', { connectorId, endpointKey });
export const testUniversalConnector = (connectorId: string) =>
  invoke<ConnectorTestResult>('test_universal_connector', { connectorId });
export const executeConnectorEndpoint = (connectorId: string, endpointKey: string, variables: Record<string, string>) =>
  invoke<{ status: number; body: string }>('execute_connector_endpoint', { connectorId, endpointKey, variables });
export const updateAgentConnectors = (agentId: string, connectorIds: string[]) =>
  invoke<void>('update_agent_connectors', { agentId, connectorIds });

// ── OAuth Login ─────────────────────────────
import type { OAuthResult } from '../types';

export const getOauthConnectors = () => invoke<string[]>('get_oauth_connectors');
export const getOauthClientIds = () => invoke<Record<string, string>>('get_oauth_client_ids');
export const setOauthClientId = (connectorId: string, clientId: string) =>
  invoke<void>('set_oauth_client_id', { connectorId, clientId });
export const startOauthLogin = (connectorId: string) =>
  invoke<OAuthResult>('start_oauth_login', { connectorId });
export const onOauthStatus = (cb: (data: { connector_id: string; status: string; instance_id?: string; error?: string }) => void) =>
  listen<{ connector_id: string; status: string; instance_id?: string; error?: string }>('oauth-status', (e) => cb(e.payload));

// ── Secure Relay (Privacy Bridge) ──────────
import type {
  RelaySession, RedactionPolicy,
} from '../types';

export const listRelaySessions = () => invoke<RelaySession[]>('list_relay_sessions');
export const createRelaySession = (
  name: string, localAgentId: string, cloudAgentId: string,
  policy?: RedactionPolicy,
) => invoke<RelaySession>('create_relay_session', {
  name, localAgentId, cloudAgentId, policy: policy ?? null,
});
export const deleteRelaySession = (sessionId: string) =>
  invoke<void>('delete_relay_session', { sessionId });
export const updateRelayPolicy = (sessionId: string, policy: RedactionPolicy) =>
  invoke<RelaySession>('update_relay_policy', { sessionId, policy });
export const relaySanitiseText = (text: string, sessionId: string) =>
  invoke<{ sanitised_text: string; redaction_count: number }>('relay_sanitise_text', { text, sessionId });
export const runRelaySession = (sessionId: string, documentContent: string) =>
  invoke<RelaySession>('run_relay_session', { sessionId, documentContent });

export const onRelayStepChange = (cb: (data: { session_id: string; step_type: string; status: string }) => void) =>
  listen<{ session_id: string; step_type: string; status: string }>('relay-step-change', (e) => cb(e.payload));

// ── Event Triggers (Epic V) ────────────────
import type {
  EmailAccount, EmailProvider as EmailProviderType,
  TriggerRule, RuleCondition, RuleLogic, TriggerSource,
  WebhookEndpoint, FileWatch, FileWatchEvent,
  EventLogEntry, EventStatus,
} from '../types';

export const emailDeviceStart = (provider: string) =>
  invoke<{ user_code: string; verification_uri: string; device_code: string; expires_in: number; connector_id: string }>(
    'email_device_start', { provider });
export const emailDeviceComplete = (connectorId: string, deviceCode: string) =>
  invoke<EmailAccount>('email_device_complete', { connectorId, deviceCode });
export const listEmailAccounts = () => invoke<EmailAccount[]>('list_email_accounts');
export const addEmailAccount = (
  provider: EmailProviderType, email: string, displayName: string,
  accessToken: string, refreshToken?: string,
) => invoke<EmailAccount>('add_email_account', {
  provider, email, displayName, accessToken, refreshToken: refreshToken ?? null,
});
export const updateEmailAccount = (accountId: string, pollIntervalSecs?: number, enabled?: boolean) =>
  invoke<EmailAccount>('update_email_account', {
    accountId, pollIntervalSecs: pollIntervalSecs ?? null, enabled: enabled ?? null,
  });
export const removeEmailAccount = (accountId: string) =>
  invoke<void>('remove_email_account', { accountId });

export const listTriggerRules = () => invoke<TriggerRule[]>('list_trigger_rules');
export const createTriggerRule = (
  name: string, source: TriggerSource, workflowId: string,
  conditions: RuleCondition[], logic: RuleLogic,
  priority?: number, extractTemplate?: string,
) => invoke<TriggerRule>('create_trigger_rule', {
  name, source, workflowId, conditions, logic,
  priority: priority ?? null, extractTemplate: extractTemplate ?? null,
});
export const updateTriggerRule = (
  ruleId: string, updates: {
    name?: string; enabled?: boolean; conditions?: RuleCondition[];
    logic?: RuleLogic; priority?: number; workflowId?: string; extractTemplate?: string;
  },
) => invoke<TriggerRule>('update_trigger_rule', {
  ruleId,
  name: updates.name ?? null,
  enabled: updates.enabled ?? null,
  conditions: updates.conditions ?? null,
  logic: updates.logic ?? null,
  priority: updates.priority ?? null,
  workflowId: updates.workflowId ?? null,
  extractTemplate: updates.extractTemplate ?? null,
});
export const deleteTriggerRule = (ruleId: string) =>
  invoke<void>('delete_trigger_rule', { ruleId });

export const listWebhooks = () => invoke<WebhookEndpoint[]>('list_webhooks');
export const createWebhook = (name: string, workflowId: string, secret?: string) =>
  invoke<WebhookEndpoint>('create_webhook', { name, workflowId, secret: secret ?? null });
export const deleteWebhook = (webhookId: string) =>
  invoke<void>('delete_webhook', { webhookId });

export const listFileWatches = () => invoke<FileWatch[]>('list_file_watches');
export const createFileWatch = (
  name: string, path: string, workflowId: string,
  recursive?: boolean, eventFilter?: FileWatchEvent, pattern?: string,
) => invoke<FileWatch>('create_file_watch', {
  name, path, workflowId,
  recursive: recursive ?? null, eventFilter: eventFilter ?? null, pattern: pattern ?? null,
});
export const deleteFileWatch = (watchId: string) =>
  invoke<void>('delete_file_watch', { watchId });

export const getEventLog = (sourceFilter?: TriggerSource, statusFilter?: EventStatus, limit?: number) =>
  invoke<EventLogEntry[]>('get_event_log', {
    sourceFilter: sourceFilter ?? null, statusFilter: statusFilter ?? null, limit: limit ?? null,
  });
export const clearEventLog = () => invoke<void>('clear_event_log');
export const pollEmailsNow = () => invoke<EventLogEntry[]>('poll_emails_now');
export const testEmailConnection = (accountId: string) =>
  invoke<string>('test_email_connection', { accountId });

export const onTriggerEvent = (cb: (entry: EventLogEntry) => void) =>
  listen<EventLogEntry>('trigger-event', (e) => cb(e.payload));

// ── Inbox & Pipeline (Epic W) ──────────────
import type {
  ProcessPipeline, PipelineItem, ItemSource as ItemSourceType, ItemStatus as ItemStatusType,
} from '../types';

export const listPipelines = () => invoke<ProcessPipeline[]>('list_pipelines');
export const createPipeline = (name: string, stages: [string, string][]) =>
  invoke<ProcessPipeline>('create_pipeline', { name, stages });
export const deletePipeline = (pipelineId: string) =>
  invoke<void>('delete_pipeline', { pipelineId });

export const listInboxItems = (sourceFilter?: ItemSourceType, limit?: number) =>
  invoke<PipelineItem[]>('list_inbox_items', {
    sourceFilter: sourceFilter ?? null, limit: limit ?? null,
  });
export const listPipelineItems = (pipelineId: string) =>
  invoke<PipelineItem[]>('list_pipeline_items', { pipelineId });
export const createInboxItem = (
  title: string, snippet: string, source: ItemSourceType,
  pipelineId?: string, data?: Record<string, unknown>,
) => invoke<PipelineItem>('create_inbox_item', {
  title, snippet, source, pipelineId: pipelineId ?? null, data: data ?? null,
});
export const movePipelineItem = (itemId: string, stageId: string, note?: string) =>
  invoke<PipelineItem>('move_pipeline_item', { itemId, stageId, note: note ?? null });
export const assignItemToPipeline = (itemId: string, pipelineId: string) =>
  invoke<PipelineItem>('assign_item_to_pipeline', { itemId, pipelineId });
export const updateItemStatus = (itemId: string, status: ItemStatusType) =>
  invoke<PipelineItem>('update_item_status', { itemId, status });
export const bulkUpdateItems = (itemIds: string[], action: string, pipelineId?: string) =>
  invoke<number>('bulk_update_items', { itemIds, action, pipelineId: pipelineId ?? null });
export const deleteInboxItem = (itemId: string) =>
  invoke<void>('delete_inbox_item', { itemId });
export const getStaleItems = () => invoke<PipelineItem[]>('get_stale_items');
export const summarizeItem = (itemId: string) => invoke<string>('summarize_item', { itemId });

// ── Scheduler (Epic X) ──────────────────────────
export const listSchedules = () => invoke<ScheduleEntry[]>('list_schedules');
export const createSchedule = (
  workflowId: string, preset: SchedulePreset, customCron?: string, timezone?: string,
) => invoke<ScheduleEntry>('create_schedule', {
  workflowId, preset, customCron: customCron ?? null, timezone: timezone ?? null,
});
export const updateSchedule = (
  id: string, preset?: SchedulePreset, customCron?: string, enabled?: boolean,
) => invoke<ScheduleEntry>('update_schedule', {
  id, preset: preset ?? null, customCron: customCron ?? null, enabled: enabled ?? null,
});
export const deleteSchedule = (id: string) => invoke<boolean>('delete_schedule', { id });
export const toggleSchedule = (id: string) => invoke<ScheduleEntry>('toggle_schedule', { id });
export const getScheduleHistory = (scheduleId?: string, limit?: number) =>
  invoke<ScheduleRunLog[]>('get_schedule_history', { scheduleId: scheduleId ?? null, limit: limit ?? null });
export const previewSchedule = (cronExpr: string, count?: number) =>
  invoke<string[]>('preview_schedule', { cronExpr, count: count ?? null });

// ── Orchestrator (Epic Y) ───────────────────────
export const listChains = () => invoke<AgentChain[]>('list_chains');
export const getChain = (id: string) => invoke<AgentChain>('get_chain', { id });
export const createChain = (
  name: string, description: string, mode: ChainMode, nodes: AgentChainNode[],
) => invoke<AgentChain>('create_chain', { name, description, mode, nodes });
export const updateChain = (
  id: string, name?: string, description?: string,
  mode?: ChainMode, nodes?: AgentChainNode[],
) => invoke<AgentChain>('update_chain', {
  id, name: name ?? null, description: description ?? null,
  mode: mode ?? null, nodes: nodes ?? null,
});
export const deleteChain = (id: string) => invoke<boolean>('delete_chain', { id });
export const listChainTemplates = () => invoke<ChainTemplate[]>('list_chain_templates');
export const runChain = (chainId: string, input: string) =>
  invoke<ChainRunState>('run_chain', { chainId, input });
export const stopChain = () => invoke<void>('stop_chain');
export const getChainRun = () => invoke<ChainRunState | null>('get_chain_run');
export const getChainHistory = (limit?: number) =>
  invoke<ChainRunState[]>('get_chain_history', { limit: limit ?? null });

// ── Suggestions (Epic Z) ────────────────────────
export const listSuggestions = () => invoke<Suggestion[]>('list_suggestions');
export const getSuggestionHistory = (limit?: number) =>
  invoke<Suggestion[]>('get_suggestion_history', { limit: limit ?? null });
export const acceptSuggestion = (id: string) => invoke<Suggestion>('accept_suggestion', { id });
export const dismissSuggestion = (id: string) => invoke<Suggestion>('dismiss_suggestion', { id });
export const dismissAllSuggestions = () => invoke<void>('dismiss_all_suggestions');
export const getSuggestionPreferences = () => invoke<SuggestionPreferences>('get_suggestion_preferences');
export const updateSuggestionPreferences = (prefs: SuggestionPreferences) =>
  invoke<SuggestionPreferences>('update_suggestion_preferences', { prefs });
export const getSuggestionCount = () => invoke<number>('suggestion_count');

// ── Dashboard & Analytics (Epic AA) ─────────────
import type { DashboardMetrics, TimeRange } from '../types';

export const getDashboardMetrics = (range: TimeRange = 'week') =>
  invoke<DashboardMetrics>('get_dashboard_metrics', { range });

// ── Data Export & Reporting (Epic AB) ───────────
import type { ExportOptions, ExportResult } from '../types';

export const exportReport = (options: ExportOptions) =>
  invoke<ExportResult>('export_report', { options });
export const listReports = () => invoke<ExportResult[]>('list_reports');
export const deleteReport = (filename: string) => invoke<void>('delete_report', { filename });
export const openReportsFolder = () => invoke<void>('open_reports_folder');

// ── Knowledge Base (Epic AC) ────────────────────
import type { KBArticle, KBSource, KBSearchResult, KBContext } from '../types';

export const listKBArticles = (category?: string, tag?: string) =>
  invoke<KBArticle[]>('list_kb_articles', { category: category ?? null, tag: tag ?? null });
export const getKBArticle = (id: string) => invoke<KBArticle>('get_kb_article', { id });
export const createKBArticle = (
  title: string, content: string, category: string, tags: string[], source: KBSource = 'manual',
) => invoke<KBArticle>('create_kb_article', { title, content, category, tags, source });
export const updateKBArticle = (
  id: string, title?: string, content?: string, category?: string, tags?: string[],
) => invoke<KBArticle>('update_kb_article', {
  id, title: title ?? null, content: content ?? null, category: category ?? null, tags: tags ?? null,
});
export const deleteKBArticle = (id: string) => invoke<boolean>('delete_kb_article', { id });
export const searchKB = (query: string, limit?: number) =>
  invoke<KBSearchResult[]>('search_kb', { query, limit: limit ?? null });
export const getKBContext = (query: string, maxChars?: number) =>
  invoke<KBContext[]>('get_kb_context', { query, maxChars: maxChars ?? null });
export const listKBCategories = () => invoke<string[]>('list_kb_categories');
export const listKBTags = () => invoke<string[]>('list_kb_tags');

// ── Notifications Center (Epic AD) ──────────────
import type { AppNotification, NotificationPreferences as NotifPrefs } from '../types';

export const listNotifications = (limit?: number) =>
  invoke<AppNotification[]>('list_notifications', { limit: limit ?? null });
export const notificationUnreadCount = () => invoke<number>('notification_unread_count');
export const markNotificationRead = (id: string) => invoke<boolean>('mark_notification_read', { id });
export const markAllNotificationsRead = () => invoke<void>('mark_all_notifications_read');
export const dismissNotification = (id: string) => invoke<boolean>('dismiss_notification', { id });
export const dismissAllNotifications = () => invoke<void>('dismiss_all_notifications');
export const getNotificationPreferences = () => invoke<NotifPrefs>('get_notification_preferences');
export const updateNotificationPreferences = (prefs: NotifPrefs) =>
  invoke<NotifPrefs>('update_notification_preferences', { prefs });

// ── Conversation Search (Epic AE) ───────────────
import type { ConversationSearchResult } from '../types';

export const searchConversations = (
  query: string, role?: string, dateFrom?: string, dateTo?: string,
  conversationId?: string, limit?: number,
) => invoke<ConversationSearchResult[]>('search_conversations', {
  query, role: role ?? null, dateFrom: dateFrom ?? null, dateTo: dateTo ?? null,
  conversationId: conversationId ?? null, limit: limit ?? null,
});
export const indexConversation = (
  conversationId: string, title: string, messages: [string, string, string, string][],
) => invoke<void>('index_conversation', { conversationId, title, messages });
export const removeConversationIndex = (conversationId: string) =>
  invoke<void>('remove_conversation_index', { conversationId });
export const getSearchStats = () => invoke<[number, number]>('get_search_stats');

// ── Keyboard Shortcuts (Epic AF) ────────────────
import type { ShortcutPreferences, ShortcutBinding } from '../types';

export const getShortcutPreferences = () => invoke<ShortcutPreferences>('get_shortcut_preferences');
export const updateShortcutPreferences = (prefs: ShortcutPreferences) =>
  invoke<ShortcutPreferences>('update_shortcut_preferences', { prefs });
export const updateShortcutBinding = (id: string, keys?: string, enabled?: boolean) =>
  invoke<ShortcutBinding>('update_shortcut_binding', { id, keys: keys ?? null, enabled: enabled ?? null });
export const addRecentCommand = (command: string) => invoke<void>('add_recent_command', { command });
export const getRecentCommands = () => invoke<string[]>('get_recent_commands');
export const resetShortcuts = () => invoke<ShortcutPreferences>('reset_shortcuts');

// ── Plugin System (Epic AG) ─────────────────────
import type { AppPlugin, PluginManifest } from '../types';

export const listPlugins = () => invoke<AppPlugin[]>('list_plugins');
export const getPlugin = (id: string) => invoke<AppPlugin>('get_plugin', { id });
export const installPlugin = (manifest: PluginManifest) => invoke<AppPlugin>('install_plugin', { manifest });
export const uninstallPlugin = (id: string) => invoke<boolean>('uninstall_plugin', { id });
export const enablePlugin = (id: string) => invoke<AppPlugin>('enable_plugin', { id });
export const disablePlugin = (id: string) => invoke<AppPlugin>('disable_plugin', { id });
export const grantPluginPermission = (pluginId: string, permissionId: string) =>
  invoke<boolean>('grant_plugin_permission', { pluginId, permissionId });
export const updatePluginSettings = (id: string, settings: Record<string, unknown>) =>
  invoke<AppPlugin>('update_plugin_settings', { id, settings });

// ── Team Collaboration (Epic AH) ────────────────
import type { SharedItem, ShareBundle, ShareRecord, ImportRecord } from '../types';

export const createShareBundle = (
  name: string, description: string, author: string, items: SharedItem[],
) => invoke<string>('create_share_bundle', { name, description, author, items });
export const parseShareBundle = (json: string) => invoke<ShareBundle>('parse_share_bundle', { json });
export const listSharedItems = () => invoke<ShareRecord[]>('list_shared_items');
export const listImportHistory = () => invoke<ImportRecord[]>('list_import_history');
export const copyToClipboard = (text: string) => invoke<void>('copy_to_clipboard', { text });

// ── Device Fleet & Hierarchy (Epic AJ) ──────────
import type {
  FleetDevice, DeviceStatus, WorkItem, WorkItemStatus,
  FleetOverview, DeviceActivity, HierarchyNode,
} from '../types';

export const listFleetDevices = () => invoke<FleetDevice[]>('list_fleet_devices');
export const getFleetDevice = (id: string) => invoke<FleetDevice>('get_fleet_device', { id });
export const registerFleetDevice = (device: FleetDevice) =>
  invoke<FleetDevice>('register_fleet_device', { device });
export const removeFleetDevice = (id: string) => invoke<boolean>('remove_fleet_device', { id });
export const updateFleetDeviceStatus = (id: string, status: DeviceStatus) =>
  invoke<FleetDevice>('update_fleet_device_status', { id, status });
export const fleetHeartbeat = (id: string, workload: number, chains: string[], workflows: string[]) =>
  invoke<FleetDevice>('fleet_heartbeat', { id, workload, chains, workflows });
export const setFleetDeviceParent = (id: string, parentId: string | null) =>
  invoke<FleetDevice>('set_fleet_device_parent', { id, parentId });
export const getFleetHierarchy = () => invoke<HierarchyNode[]>('get_fleet_hierarchy');
export const listWorkItems = (status?: string, deviceId?: string, role?: string) =>
  invoke<WorkItem[]>('list_work_items', { status: status ?? null, deviceId: deviceId ?? null, role: role ?? null });
export const getWorkItem = (id: string) => invoke<WorkItem>('get_work_item', { id });
export const createWorkItem = (item: WorkItem) => invoke<WorkItem>('create_work_item', { item });
export const assignWorkItem = (itemId: string, deviceId: string) =>
  invoke<WorkItem>('assign_work_item', { itemId, deviceId });
export const updateWorkItemStatus = (itemId: string, status: WorkItemStatus, details?: string) =>
  invoke<WorkItem>('update_work_item_status', { itemId, status, details: details ?? null });
export const handoffWorkItem = (itemId: string, fromDevice: string, toDevice: string) =>
  invoke<WorkItem>('handoff_work_item', { itemId, fromDevice, toDevice });
export const completeWorkItem = (itemId: string, result?: string) =>
  invoke<WorkItem>('complete_work_item', { itemId, result: result ?? null });
export const getFleetOverview = () => invoke<FleetOverview>('get_fleet_overview');
export const getFleetActivity = () => invoke<DeviceActivity[]>('get_fleet_activity');

// ── Fleet Routing & Communication (Epic AK) ────
import type {
  RoutingRule, EscalationRule, FleetMessage, FleetChannel, FleetAlert,
} from '../types';

export const listRoutingRules = () => invoke<RoutingRule[]>('list_routing_rules');
export const createRoutingRule = (rule: RoutingRule) =>
  invoke<RoutingRule>('create_routing_rule', { rule });
export const updateRoutingRule = (rule: RoutingRule) =>
  invoke<RoutingRule>('update_routing_rule', { rule });
export const deleteRoutingRule = (id: string) => invoke<boolean>('delete_routing_rule', { id });
export const toggleRoutingRule = (id: string) => invoke<RoutingRule>('toggle_routing_rule', { id });
export const listEscalationRules = () => invoke<EscalationRule[]>('list_escalation_rules');
export const createEscalationRule = (rule: EscalationRule) =>
  invoke<EscalationRule>('create_escalation_rule', { rule });
export const deleteEscalationRule = (id: string) => invoke<boolean>('delete_escalation_rule', { id });
export const toggleEscalationRule = (id: string) => invoke<EscalationRule>('toggle_escalation_rule', { id });
export const sendFleetMessage = (msg: FleetMessage) =>
  invoke<FleetMessage>('send_fleet_message', { msg });
export const listFleetMessages = (channel?: string, limit?: number) =>
  invoke<FleetMessage[]>('list_fleet_messages', { channel: channel ?? null, limit: limit ?? null });
export const listFleetChannels = () => invoke<FleetChannel[]>('list_fleet_channels');
export const markFleetMessagesRead = (channel: string) =>
  invoke<void>('mark_fleet_messages_read', { channel });
export const fleetUnreadCount = () => invoke<number>('fleet_unread_count');
export const listFleetAlerts = (unackedOnly?: boolean) =>
  invoke<FleetAlert[]>('list_fleet_alerts', { unackedOnly: unackedOnly ?? null });
export const acknowledgeFleetAlert = (id: string) =>
  invoke<boolean>('acknowledge_fleet_alert', { id });
export const acknowledgeAllFleetAlerts = () =>
  invoke<void>('acknowledge_all_fleet_alerts');

// ── Fleet Live Monitor (Epic AL) ────────────────
import type {
  ActivityEvent, FleetMap, FleetHealthSummary, FleetPreset,
} from '../types';

export const logFleetEvent = (event: ActivityEvent) =>
  invoke<ActivityEvent>('log_fleet_event', { event });
export const listFleetEvents = (
  deviceId?: string, role?: string, eventType?: string, severity?: string, limit?: number,
) => invoke<ActivityEvent[]>('list_fleet_events', {
  deviceId: deviceId ?? null, role: role ?? null, eventType: eventType ?? null,
  severity: severity ?? null, limit: limit ?? null,
});
export const clearFleetEvents = () => invoke<void>('clear_fleet_events');
export const getFleetMap = () => invoke<FleetMap>('get_fleet_map');
export const getFleetHealth = () => invoke<FleetHealthSummary>('get_fleet_health');
export const listFleetPresets = () => invoke<FleetPreset[]>('list_fleet_presets');
export const getFleetPreset = (id: string) => invoke<FleetPreset>('get_fleet_preset', { id });
export const createFleetPreset = (preset: FleetPreset) =>
  invoke<FleetPreset>('create_fleet_preset', { preset });
export const deleteFleetPreset = (id: string) => invoke<boolean>('delete_fleet_preset', { id });

// ── Role Playbooks (Epic AM) ────────────────────
import type { Playbook } from '../types';

export const listPlaybooks = () => invoke<Playbook[]>('list_playbooks');
export const getPlaybook = (id: string) => invoke<Playbook>('get_playbook', { id });
export const createPlaybook = (playbook: Playbook) => invoke<Playbook>('create_playbook', { playbook });
export const updatePlaybook = (playbook: Playbook) => invoke<Playbook>('update_playbook', { playbook });
export const deletePlaybook = (id: string) => invoke<boolean>('delete_playbook', { id });
export const duplicatePlaybook = (id: string) => invoke<Playbook>('duplicate_playbook', { id });
export const assignPlaybook = (deviceId: string, playbookId: string) =>
  invoke<void>('assign_playbook', { deviceId, playbookId });
export const unassignPlaybook = (deviceId: string) => invoke<void>('unassign_playbook', { deviceId });
export const getDevicePlaybook = (deviceId: string) => invoke<Playbook>('get_device_playbook', { deviceId });
export const listPlaybookAssignments = () => invoke<[string, string][]>('list_playbook_assignments');

// ── Fleet Performance (Epic AN) ─────────────────
import type { DevicePerformance, RolePerformance, PerformanceSummary, PerformanceReport } from '../types';

export const getFleetPerformance = () => invoke<DevicePerformance[]>('get_fleet_performance');
export const getRolePerformance = () => invoke<RolePerformance[]>('get_role_performance');
export const getPerformanceSummary = () => invoke<PerformanceSummary>('get_performance_summary');
export const listPerfReports = () => invoke<PerformanceReport[]>('list_perf_reports');
export const getPerfReport = (id: string) => invoke<PerformanceReport>('get_perf_report', { id });
export const deletePerfReport = (id: string) => invoke<boolean>('delete_perf_report', { id });

// ── Fleet Audit Trail & Compliance (Epic AO) ────
import type {
  AuditEntry,
  ComplianceRule, ComplianceViolation, ComplianceSummary, RetentionPolicy,
} from '../types';

export const searchAuditLog = (
  query?: string, category?: string, severity?: string,
  deviceId?: string, action?: string, limit?: number,
) => invoke<AuditEntry[]>('search_audit_log', {
  query: query ?? null, category: category ?? null, severity: severity ?? null,
  deviceId: deviceId ?? null, action: action ?? null, limit: limit ?? null,
});
export const logAuditEntry = (entry: AuditEntry) =>
  invoke<AuditEntry>('log_audit_entry', { entry });
export const auditCount = () => invoke<number>('audit_count');
export const clearAuditLog = () => invoke<void>('clear_audit_log');
export const getAuditRetention = () => invoke<RetentionPolicy>('get_audit_retention');
export const updateAuditRetention = (policy: RetentionPolicy) =>
  invoke<RetentionPolicy>('update_audit_retention', { policy });
export const listComplianceRules = () => invoke<ComplianceRule[]>('list_compliance_rules');
export const toggleComplianceRule = (id: string) =>
  invoke<ComplianceRule>('toggle_compliance_rule', { id });
export const createComplianceRule = (rule: ComplianceRule) =>
  invoke<ComplianceRule>('create_compliance_rule', { rule });
export const deleteComplianceRule = (id: string) => invoke<boolean>('delete_compliance_rule', { id });
export const listComplianceViolations = (activeOnly?: boolean) =>
  invoke<ComplianceViolation[]>('list_compliance_violations', { activeOnly: activeOnly ?? null });
export const resolveComplianceViolation = (id: string) =>
  invoke<boolean>('resolve_compliance_violation', { id });
export const getComplianceSummary = () => invoke<ComplianceSummary>('get_compliance_summary');

// ── Fleet Automation & Auto-Healing (Epic AP) ────
import type { AutomationRule, AutoExecution, AutoTemplate, AutoStats } from '../types';

export const listAutoRules = () => invoke<AutomationRule[]>('list_auto_rules');
export const createAutoRule = (rule: AutomationRule) => invoke<AutomationRule>('create_auto_rule', { rule });
export const updateAutoRule = (rule: AutomationRule) => invoke<AutomationRule>('update_auto_rule', { rule });
export const deleteAutoRule = (id: string) => invoke<boolean>('delete_auto_rule', { id });
export const toggleAutoRule = (id: string) => invoke<AutomationRule>('toggle_auto_rule', { id });
export const createRuleFromTemplate = (templateId: string, name?: string) =>
  invoke<AutomationRule>('create_rule_from_template', { templateId, name: name ?? null });
export const logAutoExecution = (exec: AutoExecution) =>
  invoke<AutoExecution>('log_auto_execution', { exec });
export const listAutoExecutions = (ruleId?: string, successOnly?: boolean, limit?: number) =>
  invoke<AutoExecution[]>('list_auto_executions', {
    ruleId: ruleId ?? null, successOnly: successOnly ?? null, limit: limit ?? null,
  });
export const clearAutoExecutions = () => invoke<void>('clear_auto_executions');
export const getAutoStats = () => invoke<AutoStats>('get_auto_stats');
export const listAutoTemplates = () => invoke<AutoTemplate[]>('list_auto_templates');
export const getAutoTemplate = (id: string) => invoke<AutoTemplate>('get_auto_template', { id });
export const deleteAutoTemplate = (id: string) => invoke<boolean>('delete_auto_template', { id });

// ── Fleet Reports & Insights (Epic AQ) ──────────
import type { FleetReport, ReportRequest, ReportTemplate } from '../types';

export const generateFleetReport = (req: ReportRequest) =>
  invoke<FleetReport>('generate_fleet_report', { req });
export const listFleetReports = () => invoke<FleetReport[]>('list_fleet_reports');
export const getFleetReport = (id: string) => invoke<FleetReport>('get_fleet_report', { id });
export const deleteFleetReport = (id: string) => invoke<boolean>('delete_fleet_report', { id });
export const listReportTemplates = () => invoke<ReportTemplate[]>('list_report_templates');

// ── Fleet Tags & Filtering (Epic AR) ────────────
import type { FleetTag, TagResourceType, SavedFilter, TagAnalytics } from '../types';

export const listFleetTags = () => invoke<FleetTag[]>('list_fleet_tags');
export const createFleetTag = (tag: FleetTag) => invoke<FleetTag>('create_fleet_tag', { tag });
export const updateFleetTag = (tag: FleetTag) => invoke<FleetTag>('update_fleet_tag', { tag });
export const deleteFleetTag = (id: string) => invoke<boolean>('delete_fleet_tag', { id });
export const assignFleetTag = (tagId: string, resourceType: TagResourceType, resourceId: string) =>
  invoke<boolean>('assign_fleet_tag', { tagId, resourceType, resourceId });
export const removeFleetTagAssignment = (tagId: string, resourceType: TagResourceType, resourceId: string) =>
  invoke<boolean>('remove_fleet_tag_assignment', { tagId, resourceType, resourceId });
export const getResourceTags = (resourceType: TagResourceType, resourceId: string) =>
  invoke<FleetTag[]>('get_resource_tags', { resourceType, resourceId });
export const getResourcesByTag = (tagId: string, resourceType?: TagResourceType) =>
  invoke<string[]>('get_resources_by_tag', { tagId, resourceType: resourceType ?? null });
export const bulkAssignFleetTag = (tagId: string, resourceType: TagResourceType, resourceIds: string[]) =>
  invoke<number>('bulk_assign_fleet_tag', { tagId, resourceType, resourceIds });
export const listSavedFilters = () => invoke<SavedFilter[]>('list_saved_filters');
export const createSavedFilter = (filter: SavedFilter) => invoke<SavedFilter>('create_saved_filter', { filter });
export const deleteSavedFilter = (id: string) => invoke<boolean>('delete_saved_filter', { id });
export const getTagAnalytics = () => invoke<TagAnalytics>('get_tag_analytics');

// ── Fleet SLA Management (Epic AS) ──────────────
import type { SlaPolicy, SlaTracker, SlaBreach, SlaDashboard } from '../types';

export const listSlaPolicies = () => invoke<SlaPolicy[]>('list_sla_policies');
export const createSlaPolicy = (policy: SlaPolicy) => invoke<SlaPolicy>('create_sla_policy', { policy });
export const updateSlaPolicy = (policy: SlaPolicy) => invoke<SlaPolicy>('update_sla_policy', { policy });
export const deleteSlaPolicy = (id: string) => invoke<boolean>('delete_sla_policy', { id });
export const toggleSlaPolicy = (id: string) => invoke<SlaPolicy>('toggle_sla_policy', { id });
export const startSlaTracking = (tracker: SlaTracker) => invoke<SlaTracker>('start_sla_tracking', { tracker });
export const listActiveSlaTrackers = () => invoke<SlaTracker[]>('list_active_sla_trackers');
export const listAllSlaTrackers = (limit?: number) =>
  invoke<SlaTracker[]>('list_all_sla_trackers', { limit: limit ?? null });
export const markSlaResponded = (trackerId: string) => invoke<SlaTracker>('mark_sla_responded', { trackerId });
export const markSlaResolved = (trackerId: string) => invoke<SlaTracker>('mark_sla_resolved', { trackerId });
export const listSlaBreaches = (unackedOnly?: boolean) =>
  invoke<SlaBreach[]>('list_sla_breaches', { unackedOnly: unackedOnly ?? null });
export const acknowledgeSLABreach = (id: string) => invoke<boolean>('acknowledge_sla_breach', { id });
export const acknowledgeAllSlaBreaches = () => invoke<void>('acknowledge_all_sla_breaches');
export const getSlaDashboard = () => invoke<SlaDashboard>('get_sla_dashboard');

// ── Fleet Activity Timeline (Epic AT) ───────────
import type { TimelineEvent, TimelineStats } from '../types';

export const pushTimelineEvent = (event: TimelineEvent) =>
  invoke<TimelineEvent>('push_timeline_event', { event });
export const listTimelineEvents = (
  category?: string, severity?: string, deviceId?: string,
  sourceSystem?: string, bookmarkedOnly?: boolean, limit?: number,
) => invoke<TimelineEvent[]>('list_timeline_events', {
  category: category ?? null, severity: severity ?? null, deviceId: deviceId ?? null,
  sourceSystem: sourceSystem ?? null, bookmarkedOnly: bookmarkedOnly ?? null, limit: limit ?? null,
});
export const searchTimeline = (query: string, limit?: number) =>
  invoke<TimelineEvent[]>('search_timeline', { query, limit: limit ?? null });
export const toggleTimelineBookmark = (id: string) =>
  invoke<TimelineEvent>('toggle_timeline_bookmark', { id });
export const listTimelineBookmarks = () => invoke<TimelineEvent[]>('list_timeline_bookmarks');
export const correlateTimelineEvents = (eventId: string, relatedId: string) =>
  invoke<boolean>('correlate_timeline_events', { eventId, relatedId });
export const getRelatedTimelineEvents = (eventId: string) =>
  invoke<TimelineEvent[]>('get_related_timeline_events', { eventId });
export const getTimelineStats = () => invoke<TimelineStats>('get_timeline_stats');
export const clearTimeline = () => invoke<void>('clear_timeline');

// ── Fleet Device Groups (Epic AU) ────────────────
import type { DeviceGroup, GroupMetrics } from '../types';

export const listGroups = () => invoke<DeviceGroup[]>('list_groups');
export const getGroup = (id: string) => invoke<DeviceGroup>('get_group', { id });
export const createGroup = (group: DeviceGroup) => invoke<DeviceGroup>('create_group', { group });
export const updateGroup = (group: DeviceGroup) => invoke<DeviceGroup>('update_group', { group });
export const deleteGroup = (id: string) => invoke<boolean>('delete_group', { id });
export const addDeviceToGroup = (groupId: string, deviceId: string) =>
  invoke<boolean>('add_device_to_group', { groupId, deviceId });
export const removeDeviceFromGroup = (groupId: string, deviceId: string) =>
  invoke<boolean>('remove_device_from_group', { groupId, deviceId });
export const getDeviceGroups = (deviceId: string) =>
  invoke<DeviceGroup[]>('get_device_groups', { deviceId });
export const getGroupMembers = (groupId: string) =>
  invoke<string[]>('get_group_members', { groupId });
export const addDevicesToGroup = (groupId: string, deviceIds: string[]) =>
  invoke<number>('add_devices_to_group', { groupId, deviceIds });
export const getAllGroupMetrics = () => invoke<GroupMetrics[]>('get_all_group_metrics');

// ── Fleet Incident Management (Epic AV) ─────────
import type { Incident, IncidentStatus, IncidentMetrics } from '../types';

export const listIncidents = (statusFilter?: string, severityFilter?: string) =>
  invoke<Incident[]>('list_incidents', { statusFilter: statusFilter ?? null, severityFilter: severityFilter ?? null });
export const listActiveIncidents = () => invoke<Incident[]>('list_active_incidents');
export const listResolvedIncidents = (limit?: number) =>
  invoke<Incident[]>('list_resolved_incidents', { limit: limit ?? null });
export const getIncident = (id: string) => invoke<Incident>('get_incident', { id });
export const createIncident = (incident: Incident) => invoke<Incident>('create_incident', { incident });
export const updateIncidentStatus = (id: string, status: IncidentStatus, message: string, author: string) =>
  invoke<Incident>('update_incident_status', { id, status, message, author });
export const assignIncident = (id: string, deviceId?: string, groupId?: string) =>
  invoke<Incident>('assign_incident', { id, deviceId: deviceId ?? null, groupId: groupId ?? null });
export const resolveIncident = (id: string, notes: string) =>
  invoke<Incident>('resolve_incident', { id, notes });
export const closeIncident = (id: string) => invoke<Incident>('close_incident', { id });
export const deleteIncident = (id: string) => invoke<boolean>('delete_incident', { id });
export const getIncidentMetrics = () => invoke<IncidentMetrics>('get_incident_metrics');

// ── Fleet Maintenance Windows (Epic AW) ─────────
import type { MaintenanceWindow, MaintenanceSummary } from '../types';

export const listMaintenanceWindows = (statusFilter?: string) =>
  invoke<MaintenanceWindow[]>('list_maintenance_windows', { statusFilter: statusFilter ?? null });
export const listUpcomingMaintenance = () => invoke<MaintenanceWindow[]>('list_upcoming_maintenance');
export const listActiveMaintenance = () => invoke<MaintenanceWindow[]>('list_active_maintenance');
export const listCompletedMaintenance = (limit?: number) =>
  invoke<MaintenanceWindow[]>('list_completed_maintenance', { limit: limit ?? null });
export const getMaintenanceWindow = (id: string) => invoke<MaintenanceWindow>('get_maintenance_window', { id });
export const createMaintenanceWindow = (window: MaintenanceWindow) =>
  invoke<MaintenanceWindow>('create_maintenance_window', { window });
export const updateMaintenanceWindow = (window: MaintenanceWindow) =>
  invoke<MaintenanceWindow>('update_maintenance_window', { window });
export const deleteMaintenanceWindow = (id: string) => invoke<boolean>('delete_maintenance_window', { id });
export const activateMaintenanceWindow = (id: string) =>
  invoke<MaintenanceWindow>('activate_maintenance_window', { id });
export const completeMaintenanceWindow = (id: string) =>
  invoke<MaintenanceWindow>('complete_maintenance_window', { id });
export const cancelMaintenanceWindow = (id: string) =>
  invoke<MaintenanceWindow>('cancel_maintenance_window', { id });
export const addMaintenanceNote = (id: string, message: string, author: string) =>
  invoke<MaintenanceWindow>('add_maintenance_note', { id, message, author });
export const getMaintenanceSummary = () => invoke<MaintenanceSummary>('get_maintenance_summary');

// ── Fleet Command Center (Epic AX) ──────────────
import type { FleetCommand, CommandTemplate, CommandStats } from '../types';

export const issueFleetCommand = (cmd: FleetCommand) => invoke<FleetCommand>('issue_fleet_command', { cmd });
export const startFleetCommand = (id: string) => invoke<FleetCommand>('start_fleet_command', { id });
export const completeFleetCommand = (id: string, result: string) =>
  invoke<FleetCommand>('complete_fleet_command', { id, result });
export const failFleetCommand = (id: string, error: string) =>
  invoke<FleetCommand>('fail_fleet_command', { id, error });
export const cancelFleetCommand = (id: string) => invoke<FleetCommand>('cancel_fleet_command', { id });
export const listFleetCommands = (statusFilter?: string, limit?: number) =>
  invoke<FleetCommand[]>('list_fleet_commands', { statusFilter: statusFilter ?? null, limit: limit ?? null });
export const listActiveFleetCommands = () => invoke<FleetCommand[]>('list_active_fleet_commands');
export const getFleetCommand = (id: string) => invoke<FleetCommand>('get_fleet_command', { id });
export const deleteFleetCommand = (id: string) => invoke<boolean>('delete_fleet_command', { id });
export const listCommandTemplates = () => invoke<CommandTemplate[]>('list_command_templates');
export const createCommandTemplate = (tpl: CommandTemplate) =>
  invoke<CommandTemplate>('create_command_template', { tpl });
export const deleteCommandTemplate = (id: string) => invoke<boolean>('delete_command_template', { id });
export const getCommandStats = () => invoke<CommandStats>('get_command_stats');

// ── Fleet Capacity Planning (Epic AY) ───────────
import type { CapacitySnapshot, CapacityThreshold, CapacityAlert, FleetCapacitySummary } from '../types';

export const recordCapacitySnapshot = (snap: CapacitySnapshot) =>
  invoke<CapacitySnapshot>('record_capacity_snapshot', { snap });
export const listCapacitySnapshots = () => invoke<CapacitySnapshot[]>('list_capacity_snapshots');
export const getDeviceCapacity = (deviceId: string) =>
  invoke<CapacitySnapshot>('get_device_capacity', { deviceId });
export const listCapacityThresholds = () => invoke<CapacityThreshold[]>('list_capacity_thresholds');
export const createCapacityThreshold = (thr: CapacityThreshold) =>
  invoke<CapacityThreshold>('create_capacity_threshold', { thr });
export const updateCapacityThreshold = (thr: CapacityThreshold) =>
  invoke<CapacityThreshold>('update_capacity_threshold', { thr });
export const toggleCapacityThreshold = (id: string) =>
  invoke<CapacityThreshold>('toggle_capacity_threshold', { id });
export const deleteCapacityThreshold = (id: string) => invoke<boolean>('delete_capacity_threshold', { id });
export const listCapacityAlerts = (unackedOnly?: boolean) =>
  invoke<CapacityAlert[]>('list_capacity_alerts', { unackedOnly: unackedOnly ?? null });
export const acknowledgeCapacityAlert = (id: string) => invoke<boolean>('acknowledge_capacity_alert', { id });
export const acknowledgeAllCapacityAlerts = () => invoke<void>('acknowledge_all_capacity_alerts');
export const getCapacitySummary = () => invoke<FleetCapacitySummary>('get_capacity_summary');

// ── Fleet Webhooks & Integrations (Epic AZ) ─────
import type { Webhook, WebhookDelivery, WebhookStats } from '../types';

export const listFleetWebhooks = () => invoke<Webhook[]>('list_fleet_webhooks');
export const getFleetWebhook = (id: string) => invoke<Webhook>('get_fleet_webhook', { id });
export const createFleetWebhook = (wh: Webhook) => invoke<Webhook>('create_fleet_webhook', { wh });
export const updateFleetWebhook = (wh: Webhook) => invoke<Webhook>('update_fleet_webhook', { wh });
export const toggleFleetWebhook = (id: string) => invoke<Webhook>('toggle_fleet_webhook', { id });
export const deleteFleetWebhook = (id: string) => invoke<boolean>('delete_fleet_webhook', { id });
export const testFleetWebhook = (id: string) => invoke<WebhookDelivery>('test_fleet_webhook', { id });
export const listWebhookDeliveries = (webhookId?: string, limit?: number) =>
  invoke<WebhookDelivery[]>('list_webhook_deliveries', { webhookId: webhookId ?? null, limit: limit ?? null });
export const clearWebhookDeliveries = (webhookId?: string) =>
  invoke<void>('clear_webhook_deliveries', { webhookId: webhookId ?? null });
export const getWebhookStats = () => invoke<WebhookStats>('get_webhook_stats');

// ── Fleet Runbooks (Epic BA) ────────────────────
import type { Runbook, RunbookExecution } from '../types';
import type { FleetEnvironment, EnvPromotion, EnvVariable, EnvSummary } from '../types';

export const listRunbooks = (category?: string) =>
  invoke<Runbook[]>('list_runbooks', { category: category ?? null });
export const getRunbook = (id: string) => invoke<Runbook>('get_runbook', { id });
export const createRunbook = (rb: Runbook) => invoke<Runbook>('create_runbook', { rb });
export const deleteRunbook = (id: string) => invoke<boolean>('delete_runbook', { id });
export const listRunbookCategories = () => invoke<string[]>('list_runbook_categories');
export const startRunbookExecution = (runbookId: string, deviceId?: string, deviceName?: string) =>
  invoke<RunbookExecution>('start_runbook_execution', { runbookId, deviceId: deviceId ?? null, deviceName: deviceName ?? null });
export const advanceRunbookStep = (executionId: string, notes?: string) =>
  invoke<RunbookExecution>('advance_runbook_step', { executionId, notes: notes ?? null });
export const failRunbookStep = (executionId: string, notes?: string) =>
  invoke<RunbookExecution>('fail_runbook_step', { executionId, notes: notes ?? null });
export const skipRunbookStep = (executionId: string) =>
  invoke<RunbookExecution>('skip_runbook_step', { executionId });
export const cancelRunbookExecution = (executionId: string) =>
  invoke<RunbookExecution>('cancel_runbook_execution', { executionId });
export const listActiveRunbookExecutions = () => invoke<RunbookExecution[]>('list_active_runbook_executions');
export const listRunbookExecutions = (limit?: number) =>
  invoke<RunbookExecution[]>('list_runbook_executions', { limit: limit ?? null });
export const getRunbookExecution = (id: string) => invoke<RunbookExecution>('get_runbook_execution', { id });

// ── Fleet Environments (Epic BB) ──────────────────
export const listEnvironments = () => invoke<FleetEnvironment[]>('list_environments');
export const createEnvironment = (env: FleetEnvironment) => invoke<FleetEnvironment>('create_environment', { env });
export const updateEnvironment = (env: FleetEnvironment) => invoke<FleetEnvironment>('update_environment', { env });
export const deleteEnvironment = (id: string) => invoke<boolean>('delete_environment', { id });
export const assignDeviceToEnv = (envId: string, deviceId: string) => invoke<FleetEnvironment>('assign_device_to_env', { envId, deviceId });
export const removeDeviceFromEnv = (envId: string, deviceId: string) => invoke<FleetEnvironment>('remove_device_from_env', { envId, deviceId });
export const listEnvVariables = (envId: string) => invoke<EnvVariable[]>('list_env_variables', { envId });
export const setEnvVariable = (envId: string, variable: EnvVariable) => invoke<EnvVariable>('set_env_variable', { envId, variable });
export const deleteEnvVariable = (envId: string, key: string) => invoke<boolean>('delete_env_variable', { envId, key });
export const promoteToEnv = (fromEnvId: string, toEnvId: string, notes: string) => invoke<EnvPromotion>('promote_to_env', { fromEnvId, toEnvId, notes });
export const listPromotions = (limit?: number) => invoke<EnvPromotion[]>('list_promotions', { limit: limit ?? null });
export const getEnvSummary = () => invoke<EnvSummary>('get_env_summary');

// ── Quick-Launch Presets (Epic 6) ───────────────
import type { WorkspacePreset, VoiceSnapshot } from '../types';

export const listPresets = () => invoke<WorkspacePreset[]>('list_presets');
export const getPreset = (id: string) => invoke<WorkspacePreset>('get_preset', { id });
export const savePreset = (input: {
  name: string;
  description: string;
  icon: string;
  activeTab: string;
  activePanel: string | null;
  roleId: string;
  voiceSettings: VoiceSnapshot;
  enabledAgentIds: string[];
  defaultAgentId: string | null;
  selectedLocalModel: string | null;
  connectorInstanceIds: string[];
}) => invoke<WorkspacePreset>('save_preset', { input });
export const updatePreset = (id: string, name?: string, description?: string, icon?: string) =>
  invoke<WorkspacePreset>('update_preset', { id, name: name ?? null, description: description ?? null, icon: icon ?? null });
export const deletePreset = (id: string) => invoke<boolean>('delete_preset', { id });
export const applyPreset = (id: string) => invoke<WorkspacePreset>('apply_preset', { id });

// ── Semantic Twin + Time Travel ──
export const getTwinGraph = () => invoke<any>('get_twin_graph');
export const getTwinStats = () => invoke<any>('get_twin_stats');
export const addTwinEntity = (entity: any) => invoke<any>('add_twin_entity', { entity });
export const updateTwinEntity = (id: string, updates: Record<string, any>) => invoke<any>('update_twin_entity', { id, updates });
export const removeTwinEntity = (id: string) => invoke<boolean>('remove_twin_entity', { id });
export const addTwinRelation = (relation: any) => invoke<any>('add_twin_relation', { relation });
export const removeTwinRelation = (id: string) => invoke<boolean>('remove_twin_relation', { id });
export const getTwinSubgraph = (entityId: string, depth: number) => invoke<any>('get_twin_subgraph', { entityId, depth });
export const getTwinEvents = (start?: string, end?: string, entityId?: string, limit?: number) =>
  invoke<any[]>('get_twin_events', { start: start ?? null, end: end ?? null, entityId: entityId ?? null, limit: limit ?? null });
export const getTwinEventCount = () => invoke<number>('get_twin_event_count');
export const getTwinSnapshots = () => invoke<any[]>('get_twin_snapshots');
export const createTwinSnapshot = () => invoke<any>('create_twin_snapshot');
export const twinStateAt = (timestamp: string) => invoke<any>('twin_state_at', { timestamp });
export const twinDiff = (t1: string, t2: string) => invoke<any>('twin_diff', { t1, t2 });
export const twinReplay = (t1: string, t2: string, maxFrames?: number) =>
  invoke<any[]>('twin_replay', { t1, t2, maxFrames: maxFrames ?? null });
export const twinQueryAt = (timestamp: string, entityType?: string) =>
  invoke<any[]>('twin_query_at', { timestamp, entityType: entityType ?? null });
// ── Pre-Flight Simulation ─────────────────────
export const preflightSubmit = (action: any) => invoke<any>('preflight_submit', { action });
export const preflightList = () => invoke<any[]>('preflight_list');
export const preflightSimulate = () => invoke<any>('preflight_simulate');
export const preflightApprove = () => invoke<string>('preflight_approve');
export const preflightReject = () => invoke<string>('preflight_reject');
export const preflightCommit = () => invoke<any[]>('preflight_commit');
export const getTwinPendingActions = () => preflightList().catch(() => []);

// ── Synced Agent Operations (via OrbixSystems API) ──
export const syncAgents = () => invoke<any[]>('sync_agents');
export const syncCreateAgent = (name: string, provider: string, model: string, endpoint: string, apiKey: string, isDefault: boolean) =>
  invoke<any>('sync_create_agent', { name, provider, model, endpoint, apiKey, isDefault });
export const syncUpdateAgent = (id: number, updates: {name?: string, provider?: string, model?: string, endpoint?: string, apiKey?: string, isDefault?: boolean, enabled?: boolean}) =>
  invoke<any>('sync_update_agent', { id, ...updates });
export const syncDeleteAgent = (id: number) => invoke<string>('sync_delete_agent', { id });
export const syncActivityFeed = () => invoke<any>('sync_activity_feed');
export const syncAgentCost = () => invoke<any>('sync_agent_cost');
export const syncApprovals = () => invoke<any>('sync_approvals');
export const syncAuditLog = () => invoke<any>('sync_audit_log');
export const syncConversations = () => invoke<any>('sync_conversations');

// ── Dynamic UI panels (Bob-extensible runtime) ──
import type { DynamicPanel, DynamicPanelPatch } from '../types';

export const listDynamicPanels = () =>
  invoke<DynamicPanel[]>('list_dynamic_panels');
export const getDynamicPanel = (id: string) =>
  invoke<DynamicPanel | null>('get_dynamic_panel', { id });
export const saveDynamicPanel = (panel: DynamicPanel) =>
  invoke<DynamicPanel>('save_dynamic_panel', { panel });
export const updateDynamicPanel = (id: string, patch: DynamicPanelPatch) =>
  invoke<DynamicPanel>('update_dynamic_panel', { id, patch });
export const deleteDynamicPanel = (id: string) =>
  invoke<boolean>('delete_dynamic_panel', { id });

// ── Core Infrastructure (Architecture v2) ──
import type {
  CoreEvent, EventBusStats,
  Job, JobInput, SchedulerStats,
  StateGraph, EntityState,
  Principal, CoreSession, SessionStats,
  LogEntry, MetricSnapshot, TraceContext,
  CoreRoute, CoreHealthCheck,
} from '../types';

// Event Bus
export const coreEventBusStats = () => invoke<EventBusStats>('core_event_bus_stats');
export const corePublishEvent = (eventType: string, source: string, payload: unknown) =>
  invoke<CoreEvent>('core_publish_event', { eventType, source, payload });

// Config
export const coreConfigGet = (key: string) => invoke<unknown>('core_config_get', { key });
export const coreConfigSet = (key: string, value: string) => invoke<void>('core_config_set', { key, value });
export const coreConfigAll = () => invoke<unknown>('core_config_all');

// Job Scheduler
export const coreSubmitJob = (input: JobInput) => invoke<Job>('core_submit_job', { input });
export const coreGetJob = (jobId: string) => invoke<Job | null>('core_get_job', { jobId });
export const coreCancelJob = (jobId: string) => invoke<Job | null>('core_cancel_job', { jobId });
export const coreJobStats = () => invoke<SchedulerStats>('core_job_stats');
export const coreListJobs = (status?: string) => invoke<Job[]>('core_list_jobs', { status: status ?? null });
export const coreDeadLetterQueue = () => invoke<Job[]>('core_dead_letter_queue');

// State Machine
export const coreSmEntityTypes = () => invoke<string[]>('core_sm_entity_types');
export const coreSmGetGraph = (entityType: string) => invoke<StateGraph | null>('core_sm_get_graph', { entityType });
export const coreSmInitEntity = (entityId: string, entityType: string) =>
  invoke<EntityState>('core_sm_init_entity', { entityId, entityType });
export const coreSmTransition = (entityId: string, toState: string, reason?: string, actor?: string) =>
  invoke<EntityState>('core_sm_transition', { entityId, toState, reason: reason ?? null, actor: actor ?? null });
export const coreSmGetState = (entityId: string) => invoke<EntityState | null>('core_sm_get_state', { entityId });
export const coreSmNextStates = (entityId: string) => invoke<string[]>('core_sm_next_states', { entityId });

// Session Manager
export const coreCreateSession = (principal: Principal) => invoke<CoreSession>('core_create_session', { principal });
export const coreValidateSession = (token: string) => invoke<CoreSession | null>('core_validate_session', { token });
export const coreRevokeSession = (token: string) => invoke<boolean>('core_revoke_session', { token });
export const coreSessionStats = () => invoke<SessionStats>('core_session_stats');
export const coreActiveSessions = () => invoke<CoreSession[]>('core_active_sessions');

// Observability
export const coreRecentLogs = (limit?: number, level?: string) =>
  invoke<LogEntry[]>('core_recent_logs', { limit: limit ?? null, level: level ?? null });
export const coreMetrics = () => invoke<MetricSnapshot[]>('core_metrics');
export const corePrometheusMetrics = () => invoke<string>('core_prometheus_metrics');
export const coreRecentTraces = (limit?: number) => invoke<TraceContext[]>('core_recent_traces', { limit: limit ?? null });

// Gateway
export const coreListRoutes = () => invoke<CoreRoute[]>('core_list_routes');
export const coreHealthCheck = () => invoke<CoreHealthCheck>('core_health_check');

// ── Window Intelligence (Epic H) ──

import type { AccessibilityStatus, WiRunningApp, WindowProfile, WindowLibrary, WindowReadResult, WindowWriteRequest, WindowWriteResult, WindowActionResult, WatchedWindow, WindowTwinEvent } from '../types';

export const wiCheckAccessibility = () =>
  invoke<AccessibilityStatus>('wi_check_accessibility');
export const wiRequestAccessibility = () =>
  invoke<AccessibilityStatus>('wi_request_accessibility');
export const wiListRunningApps = () =>
  invoke<WiRunningApp[]>('wi_list_running_apps');
export const wiProfileWindow = (pid: number, appName: string) =>
  invoke<WindowProfile>('wi_profile_window', { pid, appName });
export const wiRefreshProfile = (windowId: string) =>
  invoke<WindowProfile>('wi_refresh_profile', { windowId });
export const wiGetLibrary = () =>
  invoke<WindowLibrary>('wi_get_library');
export const wiGetProfile = (windowId: string) =>
  invoke<WindowProfile | null>('wi_get_profile', { windowId });
export const wiRemoveProfile = (windowId: string) =>
  invoke<void>('wi_remove_profile', { windowId });
export const wiReadFields = (windowId: string, fields: string[]) =>
  invoke<WindowReadResult>('wi_read_fields', { windowId, fields });
export const wiWriteFields = (windowId: string, writes: WindowWriteRequest[]) =>
  invoke<WindowWriteResult>('wi_write_fields', { windowId, writes });
export const wiTriggerAction = (windowId: string, actionId: string) =>
  invoke<WindowActionResult>('wi_trigger_action', { windowId, actionId });
export const wiAddWatched = (watched: WatchedWindow) =>
  invoke<WatchedWindow>('wi_add_watched', { watched });
export const wiRemoveWatched = (id: string) =>
  invoke<void>('wi_remove_watched', { id });
export const wiToggleWatched = (id: string) =>
  invoke<WatchedWindow>('wi_toggle_watched', { id });
export const wiGetEvents = (windowId?: string, limit?: number) =>
  invoke<WindowTwinEvent[]>('wi_get_events', { windowId: windowId ?? null, limit: limit ?? 50 });

// ── Cost Ticker + RBAC + Model Router + Supervisor ──
export const getSessionCost = () =>
  invoke<{ total_tokens_in: number; total_tokens_out: number; total_cost_usd: number; call_count: number; local_calls: number; cloud_calls: number }>('get_session_cost');
export const resetSessionCost = () =>
  invoke<void>('reset_session_cost');
export const getAgentScopes = (agentId: string) =>
  invoke<string[]>('get_agent_scopes', { agentId });
export const routeModel = (task: string) =>
  invoke<{ model: string; location: string; is_local: boolean }>('route_model', { task });
export const supervisorDecompose = (goal: string) =>
  invoke<{ subtasks: { role: string; task: string; tools_needed: string[] }[] }>('supervisor_decompose', { goal });

export const onCostUpdate = (cb: (cost: { total_tokens_in: number; total_tokens_out: number; total_cost_usd: number; call_count: number; local_calls: number; cloud_calls: number }) => void) =>
  listen<any>('cost-update', (e) => cb(e.payload));
export const onSupervisorDecomposition = (cb: (data: any) => void) =>
  listen<any>('supervisor-decomposition', (e) => cb(e.payload));

// ── Agent Council ────────────────────────────
export const councilReview = (output: string, agentName: string, inputContext: string, priorOutputs: string[], tokensUsed: number, isHighStakes: boolean) =>
  invoke<any>('council_review', { output, agentName, inputContext, priorOutputs, tokensUsed, isHighStakes });
export const councilGetDecisions = (limit?: number) =>
  invoke<any[]>('council_get_decisions', { limit: limit ?? 50 });
export const councilGetConfig = () =>
  invoke<{ scope: string; sampling_rate: number; validators_enabled: string[] }>('council_get_config');
export const councilSetScope = (scope: string) =>
  invoke<void>('council_set_scope', { scope });
export const councilGetPolicies = () =>
  invoke<any[]>('council_get_policies');
export const councilAddPolicy = (name: string, pattern: string, action: string) =>
  invoke<any>('council_add_policy', { name, pattern, action });
export const councilRemovePolicy = (id: string) =>
  invoke<void>('council_remove_policy', { id });

export const onCouncilDecision = (cb: (decision: any) => void) =>
  listen<any>('council-decision', (e) => cb(e.payload));

// ── Gemma 4 Suite ────────────────────────────
export const getGemmaSuiteStatus = () =>
  invoke<{ suite_ready: boolean; agent: { id: string; ready: boolean; name: string; size_bytes: number }; reasoning: { id: string; ready: boolean; name: string; size_bytes: number } }>('get_gemma_suite_status');
export const downloadGemmaSuite = () =>
  invoke<void>('download_gemma_suite');
export const startGemmaSuite = () =>
  invoke<{ mode: string; reason?: string }>('start_gemma_suite');

export const onGemmaSuiteProgress = (cb: (data: { model_id?: string; role?: string; status: string }) => void) =>
  listen<{ model_id?: string; role?: string; status: string }>('gemma-suite-progress', (e) => cb(e.payload));

// ── Code Assistant (Code Tab) ────────────────
export const codeScanProject = (path: string) =>
  invoke<import('../types').CodeProject>('code_scan_project', { path });
export const codeListProjects = () =>
  invoke<import('../types').CodeProject[]>('code_list_projects');
export const codeListFiles = (projectId: string) =>
  invoke<import('../types').CodeFile[]>('code_list_files', { projectId });
export const codeReadFile = (filePath: string) =>
  invoke<string>('code_read_file', { filePath });
export const codeStartAnalysis = (projectPath: string, analysisType: string, focusFiles?: string[]) =>
  invoke<import('../types').AnalysisResult>('code_start_analysis', { projectPath, analysisType, focusFiles: focusFiles ?? null });
export const codeListAgents = () =>
  invoke<import('../types').CodeAgent[]>('code_list_agents');
export const codeOpenInEditor = (path: string, line?: number) =>
  invoke<string>('code_open_in_editor', { path, line: line ?? null });
export const codeListFindings = (agentId?: string) =>
  invoke<import('../types').CodeFinding[]>('code_list_findings', { agentId: agentId ?? null });
export const codeSendChat = (content: string, contextFiles?: string[]) =>
  invoke<import('../types').CodeChat>('code_send_chat', { content, contextFiles: contextFiles ?? null });
export const codeGetChats = () =>
  invoke<import('../types').CodeChat[]>('code_get_chats');
export const codeClearChats = () =>
  invoke<void>('code_clear_chats');
export const codeSpawnAgent = (name: string, role: string, focusFiles?: string[]) =>
  invoke<import('../types').CodeAgent>('code_spawn_agent', { name, role, focusFiles: focusFiles ?? null });
export const codeDismissAgent = (agentId: string) =>
  invoke<void>('code_dismiss_agent', { agentId });

export const onCodeAgentUpdate = (cb: (agent: import('../types').CodeAgent) => void) =>
  listen<import('../types').CodeAgent>('code-agent-update', (e) => cb(e.payload));
export const onCodeFinding = (cb: (finding: import('../types').CodeFinding) => void) =>
  listen<import('../types').CodeFinding>('code-finding', (e) => cb(e.payload));

// ─────────────────────────────────────────────
//  Connector System (CONNECTORS.md)
// ─────────────────────────────────────────────

export interface ConnectorModulesSummary {
  count: number;
  slugs: string[];
  dir: string;
}

export const connectorStagingRoot = () => invoke<string>('connector_staging_root');
export const connectorModulesSummary = () =>
  invoke<ConnectorModulesSummary>('connector_modules_summary');

export interface ConnectorResult {
  success: boolean;
  output: unknown;
  error_code: string | null;
  error_message: string | null;
  cached: boolean;
}

export const connectorInvoke = (args: {
  connectorSlug: string;
  actionId: string;
  inputs?: unknown;
  idempotencyKey?: string;
  caller?: string;
}) => invoke<ConnectorResult>('connector_invoke', {
  connectorSlug: args.connectorSlug,
  actionId: args.actionId,
  inputs: args.inputs ?? null,
  idempotencyKey: args.idempotencyKey ?? null,
  caller: args.caller ?? null,
});

// Vault - slug-keyed credential store. Tokens never come back across
// the bus; only the key names + optional expiry hint.
export const connectorVaultListKeys = (slug: string) =>
  invoke<string[]>('connector_vault_list_keys', { slug });
export const connectorVaultDeleteSlug = (slug: string) =>
  invoke<void>('connector_vault_delete_slug', { slug });

export interface ExpiryHint {
  expires_at_unix: number | null;
  has_refresh_token: boolean;
}
export const connectorCompleteOauthForModule = (args: {
  slug: string;
  accessToken: string;
  refreshToken?: string;
  expiresIn?: number;
}) => invoke<ExpiryHint>('connector_complete_oauth_for_module', {
  slug: args.slug,
  accessToken: args.accessToken,
  refreshToken: args.refreshToken ?? null,
  expiresIn: args.expiresIn ?? null,
});

// Scan / sign / activate pipeline.
export interface ScanReport {
  hosts_found: string[];
  env_reads_found: string[];
  imports_found: string[];
  dynamic_access: string[];
  violations: string[];
}
export const connectorScanStaging = (stagingDir: string) =>
  invoke<ScanReport>('connector_scan_staging', { stagingDir });

export interface ActivationReport {
  slug: string;
  version: string;
  installed_at: string;
  scan: ScanReport;
  signed_key_id: string;
  sdk_sha256: string;
}
export const connectorActivateModule = (stagingDir: string) =>
  invoke<ActivationReport>('connector_activate_module', { stagingDir });

export const connectorSignerKeyId = () =>
  invoke<string>('connector_signer_key_id');

// Browser-attach probe.
export interface TabInfo { id: string; url: string; title: string; type: string }
export interface SessionProbe {
  slug: string;
  cdp_reachable: boolean;
  cookie_domains: string[];
  matching_tabs: TabInfo[];
}
export const connectorBrowserSessionProbe = (stagingDir: string) =>
  invoke<SessionProbe>('connector_browser_session_probe', { stagingDir });

// Job profiles.
export type Cadence =
  | { kind: 'hourly'; every_n: number }
  | { kind: 'daily'; hour_utc: number; minute_utc: number }
  | { kind: 'weekly'; weekday: number; hour_utc: number; minute_utc: number }
  | { kind: 'custom'; cron: string };

export type ReviewGate =
  | { kind: 'always' }
  | { kind: 'first_n'; remaining: number }
  | { kind: 'auto_publish' };

export type Engagement =
  | 'post' | 'reply' | 'comment' | 'like' | 'accept_request' | 'send_message';

export type ContentSource =
  | { kind: 'rss'; url: string }
  | { kind: 'git_hub'; owner: string; repo?: string | null }
  | { kind: 'calendar' }
  | { kind: 'manual' };

export interface JobProfile {
  id: string;
  connector_slug: string;
  goal: string;
  voice_profile_id: string;
  sources: ContentSource[];
  cadence: Cadence;
  review_gate: ReviewGate;
  engagement_scope: Engagement[];
  created_at: string;
  updated_at: string;
  last_run_at: string | null;
  approved_count: number;
  enabled: boolean;
}

export const jobProfileCreate = (profile: JobProfile) =>
  invoke<JobProfile>('job_profile_create', { profile });
export const jobProfileList = () => invoke<JobProfile[]>('job_profile_list');
export const jobProfileGet = (id: string) =>
  invoke<JobProfile | null>('job_profile_get', { id });
export const jobProfileUpdate = (args: {
  id: string;
  goal?: string;
  cadence?: Cadence;
  reviewGate?: ReviewGate;
  engagementScope?: Engagement[];
  sources?: ContentSource[];
  enabled?: boolean;
}) => invoke<JobProfile>('job_profile_update', {
  id: args.id,
  goal: args.goal ?? null,
  cadence: args.cadence ?? null,
  reviewGate: args.reviewGate ?? null,
  engagementScope: args.engagementScope ?? null,
  sources: args.sources ?? null,
  enabled: args.enabled ?? null,
});
export const jobProfileDelete = (id: string) =>
  invoke<void>('job_profile_delete', { id });
export const jobProfileRecordApproval = (id: string) =>
  invoke<JobProfile>('job_profile_record_approval', { id });
export const jobProfilePromoteAutopublish = (id: string) =>
  invoke<JobProfile>('job_profile_promote_autopublish', { id });
export const jobProfileDueNow = () =>
  invoke<JobProfile[]>('job_profile_due_now');

// Review queue - pending drafts produced by the JobProfile scheduler
// when review_gate is Always or FirstN.
export interface PendingReview {
  id: string;
  profile_id: string;
  connector_slug: string;
  draft_action: string;
  publish_action: string;
  original_inputs: unknown;
  payload: unknown;
  created_at: string;
  goal: string;
}

export const reviewQueueList = () => invoke<PendingReview[]>('review_queue_list');
export const reviewQueueGet = (id: string) =>
  invoke<PendingReview | null>('review_queue_get', { id });
export const reviewQueueReject = (id: string) =>
  invoke<void>('review_queue_reject', { id });
export const reviewQueueApprove = (id: string) =>
  invoke<ConnectorResult>('review_queue_approve', { id });

// OAuth for module connectors - runs the full PKCE dance, lands the
// token in the slug vault, returns only an expiry hint. The token
// bytes never come back to JS.
export const startOauthForModule = (slug: string) =>
  invoke<ExpiryHint>('start_oauth_for_module', { slug });

// ─────────────────────────────────────────────
//  Bob's authoring loop (CONNECTORS.md §8)
// ─────────────────────────────────────────────

export interface SpecFetch {
  url: string;
  status: number;
  bytes: number;
  body: unknown;
}
export const bobFetchSpec = (vendor: string, url: string) =>
  invoke<SpecFetch>('bob_fetch_spec', { vendor, url });

export interface DraftInputs {
  manifest_json: string;
  module_ts: string;
  tests_ts?: string;
  readme?: string;
}

export interface SmokeReport {
  passed: boolean;
  exit_code: number | null;
  stdout: string;
  stderr: string;
  elapsed_ms: number;
  timed_out: boolean;
}

export interface AuthorReport {
  slug: string;
  version: string;
  installed_at: string;
  activation: ActivationReport;
  smoke: SmokeReport | null;
}

export const bobAuthorConnector = (draft: DraftInputs) =>
  invoke<AuthorReport>('bob_author_connector', { draft });

export const bobSmokeTest = (slug: string) =>
  invoke<SmokeReport>('bob_smoke_test', { slug });

// ─────────────────────────────────────────────
//  Workspace setting (ORBIX_HANDOFF.md Step 2 #1)
// ─────────────────────────────────────────────

export interface WorkspaceSettings { path: string | null }

export const workspaceGet = () => invoke<WorkspaceSettings>('workspace_get');
export const workspaceSet = (path: string | null) =>
  invoke<WorkspaceSettings>('workspace_set', { path });

// ─────────────────────────────────────────────
//  bob CLI install
// ─────────────────────────────────────────────

export interface BobInstallReport {
  installed_at: string;
  on_path: boolean;
  shell_rc_hint: string | null;
  version_bytes: number;
}

export const bobCliInstall = () => invoke<BobInstallReport>('bob_cli_install');

// ── Pair mode (drag a bridge onto another app) ─────────────
export const listPeers = () => invoke<import('../types').Peer[]>('peer_list');
export const pairApp = (name: string) => invoke<import('../types').Peer>('peer_pair_app', { name });
export const unpairApp = (name: string) => invoke<void>('peer_unpair', { name });
export const peerSend = (name: string, text: string) =>
  invoke<import('../types').PeerSendReport>('peer_send', { name, text });
export const peerRead = (name: string) => invoke<import('../types').PeerReadReport>('peer_read', { name });
export const peerFollowUp = (name: string) => invoke<import('../types').Peer>('peer_follow_up', { name });
export const peerRefresh = (name: string) => invoke<import('../types').Peer>('peer_refresh', { name });
/** origin: where the wire starts, in global logical points (the port on the glow). */
export const peerBeginDrag = (origin?: { x: number; y: number }) =>
  invoke<void>('peer_begin_drag', { originX: origin?.x ?? null, originY: origin?.y ?? null });
export const peerCancelDrag = () => invoke<void>('peer_cancel_drag');
export const onPairEvent = (cb: (e: import('../types').PairEvent) => void) =>
  listen<import('../types').PairEvent>('pair-event', (e) => cb(e.payload));
export const onPairDrag = (cb: (f: import('../types').PairDragFrame) => void) =>
  listen<import('../types').PairDragFrame>('pair-drag', (e) => cb(e.payload));

// ── Steps waiting on the user ───────────────────────────────
export const listUserActions = () => invoke<import('../types').UserAction[]>('user_actions_list');
export const runUserAction = (id: string) => invoke<string>('user_action_run', { id });
export const onUserActions = (cb: (pending: import('../types').UserAction[]) => void) =>
  listen<{ pending: import('../types').UserAction[] }>('user-actions', (e) => cb(e.payload.pending));

// ── Workflow authoring from plain language (Build tab) ──────
export const draftWorkflow = (description: string) =>
  invoke<Workflow>('draft_workflow', { description });
export const reviseWorkflow = (id: string, instruction: string) =>
  invoke<Workflow>('revise_workflow', { id, instruction });
export const setWorkflowDeliver = (id: string, deliver: import('../types').Deliver) =>
  invoke<Workflow>('set_workflow_deliver', { id, deliver });
export const getWorkflowWhen = (id: string) => invoke<import('../types').WhenInfo>('get_workflow_when', { id });
export const setWorkflowSchedule = (id: string, spec: import('../types').ScheduleSpec | null) =>
  invoke<import('../types').WhenInfo>('set_workflow_schedule', { id, spec });
