export type TabId = 'run' | 'build';


// ── Dynamic UI panels (Bob-extensible runtime) ──
export interface DynamicPanelBlock {
  kind: string;
  // Pass-through props per kind. Common kinds: header, text, metric, list,
  // table, divider, action.
  [key: string]: unknown;
}

export interface DynamicPanel {
  id: string;
  title: string;
  description?: string | null;
  blocks: DynamicPanelBlock[];
  pinned: boolean;
  conversation_id?: string | null;
  created_at: string;
  updated_at: string;
  created_by: string;
}

export interface DynamicPanelPatch {
  title?: string;
  description?: string;
  blocks?: DynamicPanelBlock[];
  pinned?: boolean;
  conversation_id?: string | null;
}

export type OrbState = 'idle' | 'listening' | 'thinking' | 'executing' | 'learning' | 'error';

export type AgentProvider = 'local' | 'anthropic' | 'openai' | 'ollama' | 'lmstudio' | 'custom';

export interface AgentCapabilities {
  skills: string[];
  route_patterns: string[];
  keywords: string[];
  priority: number;
}

export interface RouteMatch {
  agent_id: string;
  agent_name: string;
  score: number;
  reason: string;
}

export interface Agent {
  id: string;
  name: string;
  provider: AgentProvider;
  model: string;
  endpoint: string;
  api_key: string;
  enabled: boolean;
  is_default: boolean;
  created_at: string;
  connector_ids?: string[];
  workflow_ids?: string[];
  capabilities?: AgentCapabilities;
  /** P2 employee fields */
  charter?: string;
  granted_tools?: string[];
  schedule_minutes?: number | null;
  last_run_at?: string | null;
}

export interface AgentInput {
  name: string;
  provider: AgentProvider;
  model: string;
  endpoint: string;
  api_key: string;
  is_default: boolean;
  connector_ids?: string[];
  capabilities?: AgentCapabilities;
}

export type McpTransport = 'stdio' | 'http' | 'sse';
export type McpStatus = 'stopped' | 'running' | 'error' | 'unknown';

export interface McpServer {
  id: string;
  name: string;
  transport: McpTransport;
  command: string;
  args: string[];
  env: Record<string, string>;
  url?: string;
  enabled: boolean;
  status: McpStatus;
  created_at: string;
}

export interface McpServerInput {
  name: string;
  transport: McpTransport;
  command: string;
  args: string[];
  env: Record<string, string>;
  url?: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  agent_id?: string;
  timestamp: string;
}

export interface CliTemplate {
  name: string;
  description: string;
  template_type: string;
  code: string;
  config: string;
}

// ── Voice (Epic C) ─────────────────────────────

export interface VoiceSettings {
  tts_voice: string;
  tts_rate: number;
  auto_speak: boolean;
  stt_language: string;
  stt_model?: string;
}

export interface TtsVoice {
  name: string;
  display_name: string;
  lang: string;
  gender: 'female' | 'male' | 'other';
  provider: 'kokoro' | 'system';
}

// ── Memory (Epic D) ────────────────────────────

export type MemoryCategory = 'fact' | 'preference' | 'context' | 'skill';

export interface MemoryEntry {
  id: string;
  key: string;
  value: string;
  category: MemoryCategory;
  source: string;
  created_at: string;
  updated_at: string;
}

export interface ConversationSummary {
  id: string;
  title: string;
  message_count: number;
  last_message_at: string;
  created_at: string;
}

// ── Local LLM ──────────────────────────────────

export interface LocalModel {
  id: string;
  name: string;
  description: string;
  filename: string;
  url: string;
  size_bytes: number;
  downloaded: boolean;
  /** False when this device's memory cannot run the model (it would OOM). */
  fits: boolean;
  /** Computed per device at list time: great | ok | tight | too_big | unknown. */
  fit_class: string;
  /** "gemma" | "qwen" for the curated catalog; empty for user-added models. */
  family: string;
  /** The model can see images once its projector is downloaded. */
  vision: boolean;
  /** Curated catalog entry (false = user-added, removable from the list). */
  builtin: boolean;
  /** The projector is on disk, so this model can see images right now. */
  vision_ready: boolean;
}

export interface UnsupportedModelFile {
  filename: string;
  size_bytes: number;
}

export interface DownloadProgress {
  is_downloading: boolean;
  model_id: string;
  downloaded: number;
  total: number;
  percent: number;
  /** Present when the second, vision-projector file is downloading. */
  phase?: string;
}

export interface LocalLlmStatus {
  running: boolean;
  model_id: string;
  server_available: boolean;
  /** The running model has an image projector loaded, so it can see images. */
  vision?: boolean;
}

// ── Tools (Epic E) ─────────────────────────────

export type ToolCategory = 'file_system' | 'shell' | 'network' | 'utility' | 'custom';
export type ToolCallStatus = 'pending' | 'approved' | 'denied' | 'running' | 'completed' | 'failed';

export interface ToolParam {
  name: string;
  description: string;
  param_type: string;
  required: boolean;
}

export interface ToolDef {
  id: string;
  name: string;
  description: string;
  category: ToolCategory;
  parameters: ToolParam[];
  requires_approval: boolean;
  enabled: boolean;
}

export interface ToolResult {
  success: boolean;
  output: string;
  error?: string;
}

export interface ToolCall {
  id: string;
  tool_name: string;
  arguments: Record<string, unknown>;
  status: ToolCallStatus;
  result?: ToolResult;
  created_at: string;
  resolved_at?: string;
}

// ── Browser (Epic F) ───────────────────────────

export type BrowserStatus = 'idle' | 'navigating' | 'ready' | 'error' | 'closed';

export interface BrowserSession {
  active: boolean;
  current_url: string;
  page_title: string;
  status: BrowserStatus;
}

export interface BrowserActionResult {
  success: boolean;
  data: string;
  error?: string;
}

// ── Workflows / Teach Mode (Epic G) ────────────

export type StepType = 'prompt' | 'action' | 'condition' | 'wait' | 'loop' | 'relay';
export type WorkflowRunStatus = 'idle' | 'running' | 'paused' | 'completed' | 'failed' | 'stopped';

export interface WorkflowStep {
  id: string;
  order: number;
  step_type: StepType;
  label: string;
  config: Record<string, unknown>;
  /** Optional: universal connector binding for this step */
  connector_id?: string;
  endpoint_key?: string;
}

export type WorkflowTrigger =
  | 'manual'
  | { command: string }
  | { schedule: string };

/** Where a workflow's result goes when a run finishes. */
export type Deliver = 'chat' | 'document' | 'panel' | 'notify' | { app: string };

export interface Workflow {
  id: string;
  name: string;
  description: string;
  steps: WorkflowStep[];
  trigger: WorkflowTrigger;
  deliver?: Deliver;
  created_at: string;
  updated_at: string;
}

/** A schedule as a person states it; times are local. */
export interface ScheduleSpec {
  kind: 'daily' | 'weekdays' | 'weekly' | 'monthly' | 'hourly' | 'minutes';
  hour: number;
  minute: number;
  weekday: number;
  day: number;
  every: number;
}

export interface WhenInfo {
  schedule: ScheduleSpec | null;
  cron: string | null;
  next_run: string | null;
  triggers: string[];
}

export interface TeachingSession {
  active: boolean;
  name: string;
  description: string;
  steps: WorkflowStep[];
  started_at?: string;
}

export interface WorkflowRunState {
  workflow_id: string;
  status: WorkflowRunStatus;
  current_step: number;
  total_steps: number;
  output: string[];
  error?: string;
}

// ── OS Automation (Epic H) ─────────────────────

export interface SystemInfo {
  os_version: string;
  hostname: string;
  cpu_model: string;
  cpu_cores: string;
  memory_total: string;
  memory_used: string;
  disk_total: string;
  disk_used: string;
  battery_percent?: string;
  battery_charging?: boolean;
  uptime: string;
}

export interface RunningApp {
  name: string;
  pid: string;
}

export interface WindowInfo {
  app_name: string;
  title: string;
}

export interface OsActionResult {
  success: boolean;
  output: string;
  error?: string;
}

// ── App version ─────────────────────────

export interface AppVersion {
  version: string;
  build: string;
}

// ── Screen Recording (Epic K) ─────────────────

export interface CapturedFrame {
  index: number;
  path: string;
  timestamp_ms: number;
}

export interface Recording {
  id: string;
  name: string;
  frames: CapturedFrame[];
  audio_path?: string;
  duration_secs: number;
  created_at: string;
}

export interface ProcessStep {
  description: string;
  app?: string;
  url?: string;
  action_type: string;
  timestamp_ms: number;
}

export interface ScreenRecordingState {
  is_recording: boolean;
  recording_id?: string;
  frame_count: number;
  elapsed_secs: number;
  audio_levels: number[];
}

// ── Role Profiles (Epic T) ───────────────────

export interface WorkflowTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  icon: string;
  steps: { step_type: string; label: string; config: Record<string, unknown>; connector_id?: string; endpoint_key?: string }[];
  is_builtin: boolean;
  created_at: string;
}

export interface TemplateCategory {
  id: string;
  label: string;
  icon: string;
}

export interface UserOnboarding {
  completed: boolean;
  user_name: string;
}

// ── Connectors (Epic U) ─────────────────────

export type ConnectorId = 'microsoft' | 'google' | 'salesforce' | 'sap' | 'slack' | 'jira' | 'github' | 'custom';
export type AuthMethod = 'oauth2' | 'api_key' | 'basic_auth' | 'bearer_token' | 'client_credentials';
export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error' | 'expired';

export interface ConnectorField {
  key: string;
  label: string;
  placeholder: string;
  field_type: string;
  required: boolean;
  validation_regex: string | null;
  help_text: string;
}

export interface ConnectorDefinition {
  id: ConnectorId;
  name: string;
  icon: string;
  description: string;
  auth_method: AuthMethod;
  fields: ConnectorField[];
  test_endpoint: string | null;
  docs_url: string;
}

export interface ConnectorInstance {
  id: string;
  connector_id: ConnectorId;
  name: string;
  status: ConnectionStatus;
  created_at: string;
  last_tested: string | null;
  error_message: string | null;
  credential_keys: string[];
}

export interface ConnectorTestResult {
  success: boolean;
  message: string;
  details: string | null;
}

export interface LoginAssistantMessage {
  field_key: string;
  hint: string;
  example: string;
  where_to_find: string;
}

export interface ValidationResult {
  field_key: string;
  valid: boolean;
  message: string;
}

// ── Universal Connectors ────────────────────

export type UniversalAuthType = 'none' | 'api_key' | 'bearer' | 'oauth2' | 'basic_auth';

export interface AuthConfig {
  header_name?: string;
  header_value?: string;
  token?: string;
  client_id?: string;
  client_secret?: string;
  username?: string;
  password?: string;
  token_url?: string;
  scopes?: string[];
}

export interface ConnectorEndpoint {
  key: string;
  method: string;
  path: string;
  description: string;
  body_template?: string;
  query_params?: Record<string, string>;
}

export interface UniversalConnector {
  id: string;
  name: string;
  description: string;
  base_url: string;
  auth_type: UniversalAuthType;
  auth_config: AuthConfig;
  default_headers: Record<string, string>;
  endpoints: ConnectorEndpoint[];
  status: ConnectionStatus;
  created_at: string;
  updated_at: string;
}

export interface DiscoveryResult {
  found: boolean;
  spec_type?: string;
  endpoints: ConnectorEndpoint[];
  base_url?: string;
  title?: string;
  description?: string;
}

// ── OAuth Login ─────────────────────────────

export interface OAuthResult {
  success: boolean;
  connector_id: string;
  user_email: string | null;
  user_name: string | null;
  error: string | null;
}

// ── Event Triggers (Epic V) ─────────────────

export type TriggerSource = 'email' | 'webhook' | 'file_watch';
export type EventStatus = 'triggered' | 'running' | 'completed' | 'failed' | 'skipped';
export type EmailProvider = 'gmail' | 'outlook';
export type EmailAccountStatus = 'connected' | 'disconnected' | 'expired' | 'error';
export type RuleConditionType = 'sender_contains' | 'subject_contains' | 'body_contains' | 'has_label' | 'sender_equals' | 'subject_matches';
export type RuleLogic = 'and' | 'or';
export type FileWatchEvent = 'created' | 'modified' | 'deleted' | 'any';

export interface EmailAccount {
  id: string;
  provider: EmailProvider;
  email: string;
  display_name: string;
  status: EmailAccountStatus;
  poll_interval_secs: number;
  last_polled: string | null;
  created_at: string;
}

export interface RuleCondition {
  condition_type: RuleConditionType;
  value: string;
}

export interface TriggerRule {
  id: string;
  name: string;
  source: TriggerSource;
  enabled: boolean;
  priority: number;
  conditions: RuleCondition[];
  logic: RuleLogic;
  workflow_id: string;
  extract_template: string | null;
  created_at: string;
  updated_at: string;
}

export interface WebhookEndpoint {
  id: string;
  name: string;
  path: string;
  secret: string | null;
  enabled: boolean;
  workflow_id: string;
  received_count: number;
  created_at: string;
}

export interface FileWatch {
  id: string;
  name: string;
  path: string;
  recursive: boolean;
  event_filter: FileWatchEvent;
  pattern: string | null;
  enabled: boolean;
  workflow_id: string;
  created_at: string;
}

export interface EventLogEntry {
  id: string;
  source: TriggerSource;
  source_detail: string;
  rule_id: string | null;
  rule_name: string | null;
  workflow_id: string | null;
  workflow_name: string | null;
  status: EventStatus;
  message: string;
  data: Record<string, unknown> | null;
  timestamp: string;
}

// ── Inbox & Pipeline (Epic W) ───────────────

export type ItemSource = 'email' | 'webhook' | 'file_watch' | 'manual' | 'workflow';
export type ItemStatus = 'unread' | 'read' | 'processed' | 'archived';

export interface PipelineStage {
  id: string;
  name: string;
  order: number;
  color: string;
}

export interface ProcessPipeline {
  id: string;
  name: string;
  role_id: string;
  stages: PipelineStage[];
  stale_threshold_hours: number | null;
  created_at: string;
}

export interface StageTransition {
  from_stage: string | null;
  to_stage: string;
  timestamp: string;
  note: string | null;
}

export interface PipelineItem {
  id: string;
  title: string;
  snippet: string;
  source: ItemSource;
  source_id: string | null;
  source_detail: string | null;
  pipeline_id: string | null;
  stage_id: string | null;
  status: ItemStatus;
  data: Record<string, unknown> | null;
  ai_summary: string | null;
  history: StageTransition[];
  entered_stage_at: string | null;
  created_at: string;
  updated_at: string;
}

// ── Scheduler (Epic X) ───────────────────────────

export type SchedulePreset =
  | 'every_minute' | 'every_5_minutes' | 'every_15_minutes'
  | 'every_hour' | 'daily' | 'weekday_mornings'
  | 'weekly' | 'monthly' | 'custom';

export type ScheduleRunResult = 'success' | 'failed' | 'skipped';

export interface ScheduleEntry {
  id: string;
  workflow_id: string;
  preset: SchedulePreset;
  cron_expr: string;
  enabled: boolean;
  timezone: string;
  next_run: string | null;
  last_run: string | null;
  run_count: number;
  fail_count: number;
  created_at: string;
}

export interface ScheduleRunLog {
  id: string;
  schedule_id: string;
  workflow_id: string;
  workflow_name: string;
  result: ScheduleRunResult;
  output: string[];
  error: string | null;
  started_at: string;
  finished_at: string;
  duration_ms: number;
}

// ── Orchestrator (Epic Y) ────────────────────────

export type ChainMode = 'sequential' | 'fan_out' | 'fallback' | 'specialist';
export type ChainRunStatus = 'running' | 'completed' | 'failed' | 'stopped';
export type NodeRunStatus = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

export interface AgentChainNode {
  id: string;
  agent_id: string;
  agent_name: string;
  order: number;
  role_hint: string;
  route_keywords: string[];
  timeout_secs: number;
  retry_on_fail: boolean;
}

export interface AgentChain {
  id: string;
  name: string;
  description: string;
  mode: ChainMode;
  nodes: AgentChainNode[];
  created_at: string;
  updated_at: string;
}

export interface NodeRunResult {
  node_id: string;
  agent_name: string;
  status: NodeRunStatus;
  output: string;
  error: string | null;
  duration_ms: number;
  tokens_used: number;
}

export interface ChainRunState {
  chain_id: string;
  chain_name: string;
  status: ChainRunStatus;
  mode: ChainMode;
  input: string;
  final_output: string | null;
  node_results: NodeRunResult[];
  started_at: string;
  finished_at: string | null;
  total_tokens: number;
}

export interface ChainTemplate {
  id: string;
  name: string;
  description: string;
  mode: ChainMode;
  node_hints: { role_hint: string; description: string; route_keywords: string[] }[];
}

// ── Suggestions (Epic Z) ─────────────────────────

export type SuggestionCategory = 'pipeline' | 'workflow' | 'follow_up' | 'optimization' | 'scheduling';
export type SuggestionPriority = 'low' | 'medium' | 'high';
export type SuggestionStatus = 'active' | 'accepted' | 'dismissed' | 'expired';

export type SuggestionAction =
  | { run_workflow: { workflow_id: string } }
  | { move_item: { item_id: string; stage_id: string } }
  | { create_rule: { pattern: string } }
  | { create_schedule: { workflow_id: string; cron: string } }
  | { open_panel: { panel: string } }
  | { custom: { label: string; command: string } };

export interface Suggestion {
  id: string;
  category: SuggestionCategory;
  priority: SuggestionPriority;
  title: string;
  description: string;
  action: SuggestionAction;
  action_label: string;
  status: SuggestionStatus;
  context_key: string;
  created_at: string;
  resolved_at: string | null;
}

export interface SuggestionPreferences {
  enabled: boolean;
  check_interval_mins: number;
  categories: {
    pipeline: boolean;
    workflow: boolean;
    follow_up: boolean;
    optimization: boolean;
    scheduling: boolean;
  };
  stale_threshold_hours: number;
  followup_threshold_hours: number;
}

// -- Dashboard & Analytics (Epic AA)

export type TimeRange = 'today' | 'week' | 'month';

export interface DashboardMetrics {
  summary: {
    total_items: number;
    active_workflows: number;
    events_today: number;
    stale_items: number;
    active_schedules: number;
  };
  pipeline: {
    items_per_stage: { stage_name: string; color: string; count: number }[];
    items_by_source: { source: string; count: number }[];
    throughput: { label: string; count: number }[];
    stale_count: number;
  };
  triggers: {
    events_by_source: { source: string; count: number }[];
    events_by_day: { label: string; count: number }[];
    top_rules: { rule_name: string; count: number }[];
    failure_rate: number;
  };
  workflows: {
    total_runs: number;
    success_count: number;
    failure_count: number;
    runs_by_day: { label: string; count: number }[];
    most_used: { name: string; runs: number }[];
  };
}

// -- Data Export & Reporting (Epic AB)

export type ExportFormat = 'csv' | 'json';
export type ReportType = 'pipeline' | 'event_log' | 'workflow_runs' | 'full_backup';

export interface ExportOptions {
  report_type: ReportType;
  format: ExportFormat;
  date_from?: string | null;
  date_to?: string | null;
  source_filter?: string | null;
  status_filter?: string | null;
}

export interface ExportResult {
  path: string;
  filename: string;
  size_bytes: number;
  row_count: number;
}

// -- Knowledge Base (Epic AC)

export type KBSource = 'manual' | 'chat' | 'import';

export interface KBArticle {
  id: string;
  title: string;
  content: string;
  category: string;
  tags: string[];
  source: KBSource;
  word_count: number;
  created_at: string;
  updated_at: string;
}

export interface KBSearchResult {
  article_id: string;
  title: string;
  snippet: string;
  score: number;
  category: string;
}

export interface KBContext {
  article_id: string;
  title: string;
  chunk: string;
  relevance: number;
}

// -- Notifications Center (Epic AD)

export type NotificationSource = 'trigger' | 'pipeline' | 'workflow' | 'schedule' | 'suggestion' | 'system';
export type NotificationLevel = 'info' | 'success' | 'warning' | 'error';

export interface AppNotification {
  id: string;
  source: NotificationSource;
  level: NotificationLevel;
  title: string;
  message: string;
  action_panel: string | null;
  action_id: string | null;
  read: boolean;
  created_at: string;
}

export interface NotificationPreferences {
  enabled: boolean;
  do_not_disturb: boolean;
  sound_enabled: boolean;
  desktop_enabled: boolean;
  sources: {
    trigger: boolean;
    pipeline: boolean;
    workflow: boolean;
    schedule: boolean;
    suggestion: boolean;
    system: boolean;
  };
}

// -- Conversation Search (Epic AE)

export interface ConversationSearchResult {
  conversation_id: string;
  conversation_title: string;
  message_id: string;
  role: string;
  content_snippet: string;
  score: number;
  timestamp: string;
}

export interface SearchFilters {
  role?: string | null;
  date_from?: string | null;
  date_to?: string | null;
  conversation_id?: string | null;
}

// -- Keyboard Shortcuts (Epic AF)

export interface ShortcutBinding {
  id: string;
  label: string;
  keys: string;
  category: string;
  enabled: boolean;
}

export interface ShortcutPreferences {
  bindings: ShortcutBinding[];
  vim_mode: boolean;
  show_hints: boolean;
}

// -- Plugin System (Epic AG)

export type PluginType = 'tool' | 'connector' | 'workflow_step' | 'widget';
export type PluginStatus = 'installed' | 'active' | 'disabled' | 'error';

export interface PluginPermission {
  id: string;
  label: string;
  granted: boolean;
}

export interface PluginManifest {
  name: string;
  version: string;
  author: string;
  description: string;
  plugin_type: PluginType;
  min_app_version?: string | null;
  permissions: string[];
  entry_point: string;
  settings_schema?: Record<string, unknown> | null;
}

export interface AppPlugin {
  id: string;
  manifest: PluginManifest;
  status: PluginStatus;
  permissions: PluginPermission[];
  settings: Record<string, unknown>;
  install_path: string;
  installed_at: string;
  updated_at: string;
  error: string | null;
}

// -- Team Collaboration (Epic AH)

export type BundleType = 'workflow' | 'pipeline' | 'agent' | 'mixed';

export interface SharedItem {
  item_type: string;
  original_id: string;
  name: string;
  data: unknown;
}

export interface ShareBundle {
  id: string;
  version: number;
  bundle_type: BundleType;
  name: string;
  description: string;
  author: string;
  items: SharedItem[];
  created_at: string;
  app_version: string;
}

export interface ShareRecord {
  id: string;
  item_type: string;
  item_id: string;
  name: string;
  bundle_id: string | null;
  shared_at: string;
}

export interface ImportRecord {
  id: string;
  bundle_name: string;
  bundle_type: BundleType;
  item_count: number;
  imported_at: string;
}

// -- Device Fleet & Hierarchy (Epic AJ)

export type DeviceStatus = 'Online' | 'Offline' | 'Busy' | 'Error';
export type WorkItemStatus = 'Queued' | 'Assigned' | 'InProgress' | 'HandedOff' | 'Completed' | 'Failed' | 'Blocked';
export type WorkItemPriority = 'Low' | 'Normal' | 'High' | 'Urgent';

export interface FleetDevice {
  id: string;
  name: string;
  hostname: string;
  role: string;
  role_description: string;
  device_type: string;
  ip_address: string;
  port: number;
  parent_id: string | null;
  status: DeviceStatus;
  current_workload: number;
  max_workload: number;
  active_chains: string[];
  active_workflows: string[];
  tags: string[];
  registered_at: string;
  last_heartbeat: string;
  metadata: Record<string, string>;
}

export interface WorkItemEvent {
  timestamp: string;
  event_type: string;
  device_id: string | null;
  details: string;
}

export interface WorkItem {
  id: string;
  title: string;
  description: string;
  source: string;
  source_id: string | null;
  assigned_device: string | null;
  assigned_role: string | null;
  status: WorkItemStatus;
  priority: WorkItemPriority;
  chain_id: string | null;
  workflow_id: string | null;
  parent_item_id: string | null;
  tags: string[];
  sla_deadline: string | null;
  timeline: WorkItemEvent[];
  result: string | null;
  created_at: string;
  updated_at: string;
}

export interface FleetOverview {
  total_devices: number;
  online_devices: number;
  busy_devices: number;
  total_work_items: number;
  items_in_progress: number;
  items_completed_today: number;
  items_queued: number;
  items_blocked: number;
  active_chains: number;
}

export interface DeviceActivity {
  device_id: string;
  device_name: string;
  role: string;
  status: DeviceStatus;
  running_chains: { chain_id: string; chain_name: string; current_step: string; progress_pct: number; started_at: string }[];
  running_workflows: { workflow_id: string; workflow_name: string; status: string; started_at: string }[];
  work_items_active: number;
  work_items_completed_today: number;
}

export interface HierarchyNode {
  device: FleetDevice;
  children: HierarchyNode[];
  active_items: number;
}

// -- Fleet Routing & Communication (Epic AK)

export type RoutingCondition =
  | 'Always'
  | { StatusEquals: string }
  | { PriorityAtLeast: string }
  | { HasTag: string }
  | { SourceEquals: string }
  | 'SlaBreached';

export type RoutingStrategy =
  | 'FirstAvailable'
  | 'LeastBusy'
  | 'RoundRobin'
  | { Specific: string };

export interface RoutingRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  from_role: string;
  to_role: string;
  condition: RoutingCondition;
  strategy: RoutingStrategy;
  fallback_role: string | null;
  auto_start: boolean;
  created_at: string;
  last_triggered: string | null;
  trigger_count: number;
}

export type EscalationCondition =
  | 'SlaBreached'
  | { StuckMinutes: number }
  | 'DeviceOffline'
  | { FailedAttempts: number };

export interface EscalationRule {
  id: string;
  name: string;
  enabled: boolean;
  condition: EscalationCondition;
  escalate_to_role: string;
  priority_boost: boolean;
  alert: boolean;
  created_at: string;
}

export type FleetMessageType = 'Request' | 'Response' | 'Alert' | 'Handoff' | 'Broadcast' | 'Info';

export interface FleetMessage {
  id: string;
  channel: string;
  from_device: string | null;
  from_role: string;
  to_role: string | null;
  to_device: string | null;
  message_type: FleetMessageType;
  subject: string;
  body: string;
  ref_item_id: string | null;
  read: boolean;
  created_at: string;
}

export interface FleetChannel {
  id: string;
  name: string;
  role_a: string;
  role_b: string;
  message_count: number;
  last_message_at: string | null;
}

export interface FleetAlert {
  id: string;
  alert_type: string;
  severity: string;
  title: string;
  details: string;
  device_id: string | null;
  item_id: string | null;
  acknowledged: boolean;
  created_at: string;
}

// -- Fleet Live Monitor (Epic AL)

export type ActivitySeverity = 'Info' | 'Success' | 'Warning' | 'Error';

export interface ActivityEvent {
  id: string;
  event_type: string;
  severity: ActivitySeverity;
  title: string;
  details: string;
  device_id: string | null;
  device_name: string | null;
  role: string | null;
  item_id: string | null;
  timestamp: string;
}

export interface MapNode {
  device_id: string;
  name: string;
  role: string;
  status: string;
  active_items: number;
  completed_today: number;
  x: number;
  y: number;
}

export interface MapEdge {
  from_role: string;
  to_role: string;
  rule_name: string;
  items_routed: number;
  active: boolean;
}

export interface FleetMap {
  nodes: MapNode[];
  edges: MapEdge[];
}

export interface DeviceHealth {
  device_id: string;
  device_name: string;
  role: string;
  uptime_pct: number;
  items_processed_today: number;
  items_per_hour: number;
  avg_processing_mins: number;
  active_items: number;
  failed_items: number;
  is_bottleneck: boolean;
}

export interface FleetHealthSummary {
  total_throughput_today: number;
  avg_items_per_hour: number;
  avg_processing_mins: number;
  bottleneck_role: string | null;
  bottleneck_device: string | null;
  devices: DeviceHealth[];
}

export interface FleetPresetRole {
  name: string;
  description: string;
  device_type: string;
  max_workload: number;
  tags: string[];
}

export interface FleetPresetRoute {
  from_role: string;
  to_role: string;
  condition: string;
  strategy: string;
}

export interface FleetPreset {
  id: string;
  name: string;
  description: string;
  category: string;
  roles: FleetPresetRole[];
  routes: FleetPresetRoute[];
  is_builtin: boolean;
  created_at: string;
}

// -- Role Playbooks (Epic AM)

export type PlaybookCondition =
  | 'Always'
  | { PriorityEquals: string }
  | { PriorityAtLeast: string }
  | { TagContains: string }
  | { SourceEquals: string }
  | { TitleContains: string }
  | { KeywordMatch: string[] }
  | { Custom: string };

export type PlaybookAction =
  | 'Process'
  | { Escalate: string }
  | { Reject: string }
  | { Defer: number }
  | { ApplyTemplate: string }
  | { RunWorkflow: string }
  | { Custom: string };

export interface DecisionRule {
  id: string;
  name: string;
  condition: PlaybookCondition;
  action: PlaybookAction;
  priority: number;
  enabled: boolean;
}

export interface ResponseTemplate {
  id: string;
  name: string;
  category: string;
  content: string;
  variables: string[];
}

export type QualityGateType =
  | { MinResponseLength: number }
  | 'RequiresApproval'
  | { MustContainKeyword: string }
  | { MaxProcessingMinutes: number }
  | { CustomCheck: string };

export interface QualityGate {
  id: string;
  name: string;
  gate_type: QualityGateType;
  required: boolean;
}

export type KpiMetric = 'AvgResponseTime' | 'ItemsPerHour' | 'CompletionRate' | 'EscalationRate' | 'QualityScore' | 'UptimePercent' | { Custom: string };

export interface KpiTarget {
  id: string;
  name: string;
  metric: KpiMetric;
  target_value: number;
  unit: string;
}

export type IdleBehavior =
  | 'Wait'
  | 'PollForWork'
  | { RunMaintenance: string }
  | { SelfImprove: string }
  | 'NotifyAdmin';

export interface PlaybookVersion {
  version: number;
  changed_at: string;
  summary: string;
}

export interface Playbook {
  id: string;
  name: string;
  role: string;
  description: string;
  instructions: string;
  decision_rules: DecisionRule[];
  response_templates: ResponseTemplate[];
  quality_gates: QualityGate[];
  kpi_targets: KpiTarget[];
  allowed_tools: string[];
  allowed_connectors: string[];
  idle_behavior: IdleBehavior;
  auto_process: boolean;
  max_concurrent: number;
  is_builtin: boolean;
  version: number;
  version_history: PlaybookVersion[];
  created_at: string;
  updated_at: string;
}

// -- Fleet Performance (Epic AN)

export interface KpiSnapshot {
  kpi_name: string;
  target_value: number;
  actual_value: number;
  unit: string;
  pct_of_target: number;
}

export interface DevicePerformance {
  device_id: string;
  device_name: string;
  role: string;
  playbook_name: string | null;
  score: number;
  kpis: KpiSnapshot[];
  items_processed: number;
  items_failed: number;
  items_escalated: number;
  avg_processing_mins: number;
  uptime_pct: number;
  trend: number[];
  rank: number;
  recorded_at: string;
}

export interface RolePerformance {
  role: string;
  device_count: number;
  avg_score: number;
  best_device: string | null;
  worst_device: string | null;
  total_processed: number;
  total_failed: number;
  avg_processing_mins: number;
  kpi_summary: KpiSnapshot[];
}

export interface PerformanceSummary {
  fleet_score: number;
  total_devices: number;
  top_performers: string[];
  underperformers: string[];
  total_processed_today: number;
  total_failed_today: number;
  improvement_suggestions: string[];
}

export interface PerformanceReport {
  id: string;
  title: string;
  period: string;
  date_range: string;
  devices: DevicePerformance[];
  roles: RolePerformance[];
  summary: PerformanceSummary;
  generated_at: string;
}

// -- Fleet Audit Trail & Compliance (Epic AO)

export type AuditCategory = 'Device' | 'WorkItem' | 'Routing' | 'Playbook' | 'Performance' | 'Security' | 'System';
export type AuditSeverity = 'Info' | 'Warning' | 'Critical';

export interface AuditEntry {
  id: string;
  timestamp: string;
  action: string;
  category: AuditCategory;
  actor: string;
  actor_id: string | null;
  target_type: string;
  target_id: string | null;
  target_name: string | null;
  details: string;
  before_state: string | null;
  after_state: string | null;
  severity: AuditSeverity;
}

export type ComplianceCheckType =
  | { RequiresApprovalAbovePriority: string }
  | { MaxProcessingMinutes: number }
  | 'RequiresPlaybook'
  | { MaxFailureRate: number }
  | { MinUptimePercent: number }
  | 'NoUnacknowledgedAlerts'
  | { Custom: string };

export interface ComplianceRule {
  id: string;
  name: string;
  description: string;
  category: string;
  enabled: boolean;
  check_type: ComplianceCheckType;
}

export interface ComplianceViolation {
  id: string;
  rule_id: string;
  rule_name: string;
  device_id: string | null;
  device_name: string | null;
  severity: string;
  details: string;
  detected_at: string;
  resolved: boolean;
  resolved_at: string | null;
}

export interface DeviceComplianceInfo {
  device_id: string;
  device_name: string;
  role: string;
  compliance_score: number;
  rules_passed: number;
  rules_failed: number;
  violations: ComplianceViolation[];
}

export interface ComplianceSummary {
  fleet_compliance_score: number;
  total_rules: number;
  rules_passing: number;
  active_violations: number;
  critical_violations: number;
  devices: DeviceComplianceInfo[];
}

export interface RetentionPolicy {
  max_entries: number;
  max_age_days: number;
  auto_export: boolean;
}

// -- Fleet Automation & Auto-Healing (Epic AP)

export type AutoCondition =
  | 'DeviceOffline'
  | 'DeviceError'
  | 'SlaBreached'
  | { FailureRateAbove: number }
  | { WorkloadAbove: number }
  | { QueueOverflow: number }
  | { DeviceIdleMinutes: number }
  | 'ComplianceViolation'
  | { Custom: string };

export type AutoAction =
  | 'ReassignWorkItems'
  | { EscalateToRole: string }
  | 'PauseDevice'
  | { NotifyAdmin: string }
  | 'RebalanceWorkload'
  | { RunPlaybook: string }
  | { CreateAlert: string }
  | 'RestartDeviceRole'
  | { Chain: AutoAction[] }
  | { Custom: string };

export interface AutomationRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  priority: number;
  condition: AutoCondition;
  action: AutoAction;
  cooldown_secs: number;
  last_triggered: string | null;
  trigger_count: number;
  created_at: string;
}

export interface AutoExecution {
  id: string;
  rule_id: string;
  rule_name: string;
  condition_met: string;
  action_taken: string;
  device_id: string | null;
  device_name: string | null;
  success: boolean;
  details: string;
  executed_at: string;
}

export interface AutoTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  condition: AutoCondition;
  action: AutoAction;
  cooldown_secs: number;
  is_builtin: boolean;
}

export interface AutoStats {
  total_executions: number;
  successful: number;
  failed: number;
  active_rules: number;
  total_rules: number;
  last_execution: string | null;
}

// -- Fleet Reports & Insights (Epic AQ)

export type FleetReportType =
  | 'DailyDigest'
  | 'WeeklySummary'
  | 'ComplianceAudit'
  | 'PerformanceReview'
  | 'RoutingAnalysis'
  | { Custom: string };

export interface ReportRequest {
  report_type: FleetReportType;
  title: string | null;
  date_from: string | null;
  date_to: string | null;
  device_filter: string[] | null;
  role_filter: string[] | null;
}

export type SectionType = 'Summary' | 'DeviceRanking' | 'ComplianceStatus' | 'RoutingEfficiency' | 'WorkItemBreakdown' | 'Alerts' | 'Trends';

export interface ReportEntry {
  label: string;
  value: string;
  change: string | null;
  status: string | null;
}

export interface ReportSection {
  title: string;
  section_type: SectionType;
  entries: ReportEntry[];
}

export interface FleetReport {
  id: string;
  report_type: FleetReportType;
  title: string;
  date_range: string;
  sections: ReportSection[];
  recommendations: string[];
  generated_at: string;
}

export interface ReportTemplate {
  id: string;
  name: string;
  description: string;
  report_type: FleetReportType;
  is_builtin: boolean;
}

// -- Fleet Tags & Filtering (Epic AR)

export type TagResourceType = 'Device' | 'WorkItem' | 'Playbook' | 'RoutingRule';

export interface FleetTag {
  id: string;
  name: string;
  color: string;
  description: string;
  auto_rule: string | null;
  created_at: string;
}

export type FilterCriterion =
  | { HasTag: string }
  | { HasAnyTag: string[] }
  | { Status: string }
  | { Role: string }
  | { TextContains: string }
  | { Priority: string }
  | { CreatedAfter: string }
  | { CreatedBefore: string };

export type FilterLogic = 'And' | 'Or';

export interface SavedFilter {
  id: string;
  name: string;
  description: string;
  criteria: FilterCriterion[];
  logic: FilterLogic;
  created_at: string;
}

export interface TagUsage {
  tag_id: string;
  tag_name: string;
  color: string;
  device_count: number;
  work_item_count: number;
  playbook_count: number;
  routing_rule_count: number;
  total_count: number;
}

export interface TagAnalytics {
  total_tags: number;
  total_assignments: number;
  most_used: string | null;
  least_used: string | null;
  untagged_devices: number;
  usage: TagUsage[];
}

// -- Fleet SLA Management (Epic AS)

export interface SlaPolicy {
  id: string;
  name: string;
  description: string;
  priority_filter: string | null;
  role_filter: string | null;
  response_minutes: number;
  resolution_minutes: number;
  warning_pct: number;
  enabled: boolean;
  is_builtin: boolean;
  created_at: string;
}

export type SlaStatus = 'OnTrack' | 'AtRisk' | 'Breached' | 'Met';

export interface SlaTracker {
  id: string;
  policy_id: string;
  policy_name: string;
  work_item_id: string;
  work_item_title: string;
  device_id: string | null;
  device_name: string | null;
  role: string | null;
  priority: string;
  status: SlaStatus;
  response_deadline: string;
  resolution_deadline: string;
  responded_at: string | null;
  resolved_at: string | null;
  response_breached: boolean;
  resolution_breached: boolean;
  remaining_minutes: number;
  started_at: string;
}

export type BreachType = 'Response' | 'Resolution';
export type BreachSeverity = 'Minor' | 'Major' | 'Critical';

export interface SlaBreach {
  id: string;
  tracker_id: string;
  policy_id: string;
  policy_name: string;
  work_item_id: string;
  work_item_title: string;
  device_name: string | null;
  breach_type: BreachType;
  severity: BreachSeverity;
  overdue_minutes: number;
  acknowledged: boolean;
  breached_at: string;
}

export interface SlaDashboard {
  total_active: number;
  on_track: number;
  at_risk: number;
  breached: number;
  met_today: number;
  compliance_rate: number;
  avg_resolution_mins: number;
  total_breaches: number;
  unacknowledged_breaches: number;
}

// -- Fleet Activity Timeline (Epic AT)

export type TimelineCategory = 'DeviceEvent' | 'WorkItemLifecycle' | 'SlaEvent' | 'AutomationTrigger' | 'ComplianceChange' | 'RoutingActivity' | 'SystemEvent';
export type TimelineSeverity = 'Info' | 'Success' | 'Warning' | 'Error';

export interface TimelineEvent {
  id: string;
  category: TimelineCategory;
  severity: TimelineSeverity;
  title: string;
  description: string;
  source_system: string;
  source_id: string | null;
  device_id: string | null;
  device_name: string | null;
  work_item_id: string | null;
  related_event_ids: string[];
  bookmarked: boolean;
  tags: string[];
  timestamp: string;
}

export interface TimelineStats {
  total_events: number;
  events_today: number;
  bookmarked: number;
  by_category: [string, number][];
  by_severity: [string, number][];
  most_active_device: string | null;
}

// -- Fleet Device Groups (Epic AU)

export type GroupMatchType = 'Role' | 'Tag' | 'Status' | 'NameContains';

export interface GroupAutoRule {
  match_type: GroupMatchType;
  value: string;
}

export interface DeviceGroup {
  id: string;
  name: string;
  description: string;
  color: string;
  icon: string;
  auto_rule: GroupAutoRule | null;
  member_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface GroupMetrics {
  group_id: string;
  group_name: string;
  color: string;
  total_devices: number;
  online_devices: number;
  busy_devices: number;
  total_work_items: number;
  items_completed: number;
  items_failed: number;
  avg_workload: number;
}

// -- Fleet Incident Management (Epic AV)

export type IncidentSeverity = 'Critical' | 'High' | 'Medium' | 'Low';
export type IncidentStatus = 'Open' | 'Investigating' | 'Mitigating' | 'Resolved' | 'Closed';

export interface IncidentUpdate {
  id: string;
  status: IncidentStatus;
  message: string;
  author: string;
  timestamp: string;
}

export interface Incident {
  id: string;
  title: string;
  description: string;
  severity: IncidentSeverity;
  status: IncidentStatus;
  assigned_device_id: string | null;
  assigned_group_id: string | null;
  source: string;
  source_id: string | null;
  affected_device_ids: string[];
  tags: string[];
  updates: IncidentUpdate[];
  created_at: string;
  updated_at: string;
  resolved_at: string | null;
  resolution_notes: string | null;
}

export interface IncidentMetrics {
  total_incidents: number;
  open_incidents: number;
  investigating: number;
  mitigating: number;
  resolved_today: number;
  by_severity: [string, number][];
  avg_resolution_mins: number;
  mttr_mins: number;
}

// -- Fleet Maintenance Windows (Epic AW)

export type MaintenanceStatus = 'Scheduled' | 'Active' | 'Completed' | 'Cancelled';
export type RecurrenceType = 'None' | 'Daily' | 'Weekly' | 'Monthly';

export interface MaintenanceNote {
  id: string;
  message: string;
  author: string;
  timestamp: string;
}

export interface MaintenanceWindow {
  id: string;
  title: string;
  description: string;
  status: MaintenanceStatus;
  start_time: string;
  end_time: string;
  recurrence: RecurrenceType;
  affected_device_ids: string[];
  affected_group_ids: string[];
  suppress_alerts: boolean;
  pause_sla: boolean;
  created_by: string;
  notes: MaintenanceNote[];
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface MaintenanceSummary {
  total_windows: number;
  scheduled: number;
  active: number;
  completed: number;
  cancelled: number;
  total_affected_devices: number;
  upcoming_next_24h: number;
}

// -- Fleet Command Center (Epic AX)

export type CommandStatus = 'Pending' | 'Running' | 'Completed' | 'Failed' | 'Cancelled';
export type CommandTarget = 'SingleDevice' | 'Group' | 'AllDevices';

export interface FleetCommand {
  id: string;
  name: string;
  command: string;
  args: string[];
  target_type: CommandTarget;
  target_id: string | null;
  target_name: string | null;
  status: CommandStatus;
  result: string | null;
  error: string | null;
  issued_by: string;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface CommandTemplate {
  id: string;
  name: string;
  description: string;
  command: string;
  default_args: string[];
  category: string;
  created_at: string;
}

export interface CommandStats {
  total_commands: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  success_rate: number;
  commands_today: number;
}

// -- Fleet Capacity Planning (Epic AY)

export type ThresholdLevel = 'Warning' | 'Critical';

export interface CapacitySnapshot {
  id: string;
  device_id: string;
  device_name: string;
  cpu_percent: number;
  memory_percent: number;
  disk_percent: number;
  workload_percent: number;
  active_connections: number;
  queue_depth: number;
  timestamp: string;
}

export interface CapacityThreshold {
  id: string;
  name: string;
  resource: string;
  level: ThresholdLevel;
  value: number;
  enabled: boolean;
  created_at: string;
}

export interface CapacityAlert {
  id: string;
  device_id: string;
  device_name: string;
  resource: string;
  current_value: number;
  threshold_value: number;
  level: ThresholdLevel;
  acknowledged: boolean;
  timestamp: string;
}

export interface FleetCapacitySummary {
  total_devices: number;
  avg_cpu: number;
  avg_memory: number;
  avg_disk: number;
  avg_workload: number;
  devices_over_warning: number;
  devices_over_critical: number;
  unacknowledged_alerts: number;
  highest_cpu_device: string | null;
  highest_memory_device: string | null;
}

// -- Fleet Webhooks & Integrations (Epic AZ)

export type WebhookEventType = 'DeviceOnline' | 'DeviceOffline' | 'WorkItemCreated' | 'WorkItemCompleted' | 'WorkItemFailed' | 'IncidentCreated' | 'IncidentResolved' | 'SlaBreach' | 'CapacityAlert' | 'MaintenanceStarted' | 'MaintenanceCompleted' | 'CommandIssued' | 'CommandCompleted' | 'All';
export type DeliveryStatus = 'Pending' | 'Success' | 'Failed' | 'Retrying';

export interface Webhook {
  id: string;
  name: string;
  url: string;
  secret: string | null;
  event_types: WebhookEventType[];
  enabled: boolean;
  retry_count: number;
  timeout_secs: number;
  created_at: string;
  updated_at: string;
  last_triggered: string | null;
  total_deliveries: number;
  successful_deliveries: number;
}

export interface WebhookDelivery {
  id: string;
  webhook_id: string;
  webhook_name: string;
  event_type: WebhookEventType;
  status: DeliveryStatus;
  response_code: number | null;
  response_body: string | null;
  attempt: number;
  max_attempts: number;
  error: string | null;
  payload_preview: string;
  timestamp: string;
  duration_ms: number | null;
}

export interface WebhookStats {
  total_webhooks: number;
  active_webhooks: number;
  total_deliveries: number;
  successful_deliveries: number;
  failed_deliveries: number;
  pending_deliveries: number;
  success_rate: number;
  deliveries_today: number;
}

// -- Fleet Runbooks (Epic BA)

export type RunbookStepStatus = 'Pending' | 'InProgress' | 'Completed' | 'Failed' | 'Skipped';
export type RunbookExecutionStatus = 'Running' | 'Completed' | 'Failed' | 'Cancelled';

export interface RunbookStep {
  id: string;
  order: number;
  title: string;
  instructions: string;
  expected_outcome: string;
  rollback: string | null;
  estimated_mins: number;
  requires_confirmation: boolean;
}

export interface Runbook {
  id: string;
  name: string;
  description: string;
  category: string;
  steps: RunbookStep[];
  estimated_mins: number;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface ExecutionStep {
  step_id: string;
  status: RunbookStepStatus;
  notes: string | null;
  started_at: string | null;
  completed_at: string | null;
}

export interface RunbookExecution {
  id: string;
  runbook_id: string;
  runbook_name: string;
  device_id: string | null;
  device_name: string | null;
  status: RunbookExecutionStatus;
  current_step: number;
  total_steps: number;
  step_statuses: ExecutionStep[];
  started_at: string;
  completed_at: string | null;
  executed_by: string;
}

// ── Fleet Environments (Epic BB) ──────────────────
export type EnvTier = 'Production' | 'Staging' | 'Development' | 'Testing' | 'Custom';

export interface FleetEnvironment {
  id: string;
  name: string;
  tier: EnvTier;
  description: string;
  color: string;
  device_ids: string[];
  locked: boolean;
  created_at: string;
}

export interface EnvVariable {
  key: string;
  value: string;
  secret: boolean;
  updated_at: string;
}

export interface EnvPromotion {
  id: string;
  from_env_id: string;
  from_env_name: string;
  to_env_id: string;
  to_env_name: string;
  promoted_by: string;
  notes: string;
  promoted_at: string;
}

export interface EnvSummary {
  total_environments: number;
  total_devices_assigned: number;
  recent_promotions: number;
  environments: { name: string; tier: EnvTier; device_count: number }[];
}

// ── Quick-Launch Presets (Epic 6) ─────────────

export interface VoiceSnapshot {
  tts_voice: string;
  tts_rate: number;
  auto_speak: boolean;
  stt_language: string;
  stt_model?: string;
}

export interface WorkspacePreset {
  id: string;
  name: string;
  description: string;
  icon: string;
  active_tab: TabId;
  active_panel: PanelType | null;
  role_id: string;
  voice_settings: VoiceSnapshot;
  enabled_agent_ids: string[];
  default_agent_id: string | null;
  selected_local_model: string | null;
  connector_instance_ids: string[];
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
}

export type PanelType = 'agents' | 'mcp' | 'voice' | 'memory' | 'tools' | 'browser' | 'settings' | 'connectors' | 'triggers' | 'inbox' | 'scheduler' | 'orchestrator' | 'suggestions' | 'dashboard' | 'knowledge' | 'notifications' | 'search' | 'plugins' | 'monitor' | 'audit' | 'incidents' | 'approvals' | 'window-intel' | 'data-control' | 'code-agents' | 'code-findings' | 'my-ui' | 'documents' | 'guide' | null;

// ── Window Intelligence (Epic H) ────

export type WindowStatus = 'live' | 'stale' | 'disconnected';
export type UpdateFrequency = 'realtime' | 'on_change' | 'manual';
export type TwinEventType = 'value_changed' | 'focus_changed' | 'page_navigated' | 'window_opened' | 'action_triggered' | 'profile_created' | 'profile_updated';

export interface WindowInput {
  id: string;
  label: string;
  input_type: string;
  required: boolean;
  current_value: string | null;
  max_length: number | null;
  validation: string | null;
  accessibility_id: string | null;
}

export interface WindowDataElement {
  id: string;
  element_type: string;
  label: string;
  readable: boolean;
  current_value: string | null;
}

export interface WindowAction {
  id: string;
  label: string;
  action_type: string;
  accessibility_id: string | null;
  enabled: boolean;
}

export interface WindowNavigation {
  sections: string[];
}

export interface WindowProfileData {
  inputs: WindowInput[];
  data_elements: WindowDataElement[];
  actions: WindowAction[];
  navigation: WindowNavigation;
}

export interface WindowProfile {
  window_id: string;
  application: string;
  title: string;
  pid: number;
  url: string | null;
  captured_at: string;
  last_updated: string;
  profile: WindowProfileData;
  field_count: number;
  action_count: number;
  status: WindowStatus;
}

export interface WatchedWindow {
  id: string;
  application: string;
  url_pattern: string | null;
  file_pattern: string | null;
  label: string;
  update_frequency: UpdateFrequency;
  active: boolean;
}

export interface WindowTwinEvent {
  id: string;
  window_id: string;
  event_type: TwinEventType;
  field_id: string | null;
  old_value: string | null;
  new_value: string | null;
  timestamp: string;
}

export interface WindowReadResult {
  window_id: string;
  fields: Record<string, string | null>;
  read_at: string;
}

export interface WindowWriteRequest {
  field: string;
  value: string;
}

export interface WindowWriteResult {
  window_id: string;
  written: string[];
  failed: string[];
  written_at: string;
}

export interface WindowActionResult {
  window_id: string;
  action_id: string;
  success: boolean;
  triggered_at: string;
}

export interface AccessibilityStatus {
  trusted: boolean;
  message: string;
}

export interface WindowLibrary {
  active_windows: WindowProfile[];
  watched: WatchedWindow[];
  total_fields_mapped: number;
  total_actions_mapped: number;
}

export interface WiRunningApp {
  name: string;
  pid: number;
}

// ── Core Infrastructure (Architecture v2) ────

export interface CoreEvent {
  id: string;
  event_type: string;
  source: string;
  timestamp: string;
  payload: unknown;
  correlation_id?: string;
  metadata: Record<string, string>;
}

export interface EventBusStats {
  total_published: number;
  total_delivered: number;
  total_dropped: number;
  subscriber_count: number;
  buffer_capacity: number;
  backpressure_events: number;
}

export type JobStatus = 'pending' | 'running' | 'completed' | 'failed' | 'retrying' | 'dead' | 'cancelled';

export interface JobResult {
  success: boolean;
  output: unknown;
  duration_ms: number;
}

export interface Job {
  id: string;
  job_type: string;
  payload: unknown;
  priority: number;
  max_retries: number;
  timeout_ms: number;
  schedule_at?: string;
  status: JobStatus;
  attempt: number;
  result?: JobResult;
  error?: string;
  created_at: string;
  started_at?: string;
  completed_at?: string;
}

export interface JobInput {
  job_type: string;
  payload: unknown;
  priority?: number;
  max_retries?: number;
  timeout_ms?: number;
  schedule_at?: string;
}

export interface SchedulerStats {
  total_submitted: number;
  total_completed: number;
  total_failed: number;
  total_dead: number;
  queue_depth: number;
  running_count: number;
  dead_letter_count: number;
}

export interface StateGraph {
  name: string;
  initial_state: string;
  terminal_states: string[];
  transitions: Record<string, string[]>;
}

export interface StateTransition {
  from: string;
  to: string;
  timestamp: string;
  reason?: string;
  actor?: string;
}

export interface EntityState {
  entity_id: string;
  entity_type: string;
  current_state: string;
  history: StateTransition[];
  created_at: string;
  updated_at: string;
}

export interface Principal {
  id: string;
  name: string;
  email?: string;
  roles: string[];
  attributes: Record<string, string>;
}

export type SessionState = 'created' | 'active' | 'idle' | 'expired' | 'revoked';

export interface CoreSession {
  token: string;
  principal: Principal;
  state: SessionState;
  metadata: Record<string, string>;
  created_at: string;
  last_activity: string;
  expires_at: string;
  idle_deadline: string;
}

export interface SessionStats {
  total: number;
  active: number;
  idle: number;
  created: number;
  expired: number;
  revoked: number;
}

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  module: string;
  message: string;
  fields: Record<string, unknown>;
  trace_id?: string;
  span_id?: string;
}

export interface MetricSnapshot {
  name: string;
  metric_type: 'counter' | 'gauge' | 'histogram';
  value: number;
  labels: Record<string, string>;
  timestamp: string;
}

export type SpanStatus = 'ok' | 'error' | 'in_progress';

export interface TraceContext {
  trace_id: string;
  span_id: string;
  parent_span_id?: string;
  operation: string;
  start_time: string;
  end_time?: string;
  duration_ms?: number;
  status: SpanStatus;
  attributes: Record<string, string>;
}

export interface CoreRoute {
  path: string;
  method: string;
  handler: string;
  middleware: string[];
  priority: number;
  description?: string;
}

export interface CoreHealthCheck {
  status: string;
  timestamp: string;
  components: Record<string, unknown>;
}

// ── Secure Relay (Privacy Bridge) ────────────────

export type RelayStatus = 'idle' | 'local_analysing' | 'sanitising' | 'cloud_querying' | 'local_comparing' | 'completed' | 'failed';
export type RelayStepType = 'local_analyse' | 'sanitise' | 'cloud_query' | 'local_compare';
export type RelayStepStatus = 'pending' | 'running' | 'completed' | 'failed';

export interface RelaySession {
  id: string;
  name: string;
  status: RelayStatus;
  local_agent_id: string;
  cloud_agent_id: string;
  policy: RedactionPolicy;
  steps: RelayStep[];
  redaction_log: RedactionEntry[];
  created_at: string;
  updated_at: string;
}

export interface RelayStep {
  id: string;
  step_type: RelayStepType;
  status: RelayStepStatus;
  input: string | null;
  output: string | null;
  redaction_count: number;
  timestamp: string;
}

export interface RedactionPolicy {
  redact_emails: boolean;
  redact_phones: boolean;
  redact_amounts: boolean;
  redact_dates: boolean;
  redact_names: boolean;
  custom_patterns: CustomPattern[];
  blocked_terms: string[];
  max_query_length: number;
  strip_quotes: boolean;
}

export interface CustomPattern {
  name: string;
  regex: string;
}

export interface RedactionEntry {
  token: string;
  category: string;
  original_length: number;
  timestamp: string;
}

// ── Code Assistant (Code Tab) ────────────────

export interface CodeProject {
  id: string;
  name: string;
  path: string;
  language: string;
  files: number;
  lines: number;
  scanned_at: string;
}

export interface CodeFile {
  path: string;
  relative_path: string;
  language: string;
  lines: number;
  size_bytes: number;
}

export interface CodeAgent {
  id: string;
  name: string;
  role: string;
  status: string;
  focus: string | null;
  findings: CodeFinding[];
  created_at: string;
}

export interface CodeFinding {
  id: string;
  agent_id: string;
  severity: string;
  category: string;
  title: string;
  description: string;
  file_path: string | null;
  line_start: number | null;
  line_end: number | null;
  suggestion: string | null;
  created_at: string;
}

export interface CodeChat {
  id: string;
  role: string;
  content: string;
  agent_id: string | null;
  timestamp: string;
}

export interface AnalysisResult {
  id: string;
  project_path: string;
  analysis_type: string;
  agents_spawned: string[];
  status: string;
  started_at: string;
  completed_at: string | null;
}

// ── Pair mode (drag a bridge onto another app; peer.rs) ────
export type PeerTransport = 'service' | 'url' | 'mcp_server' | 'devtools' | 'mcp' | 'bridge' | 'accessibility' | 'clipboard';
export type PeerCapability = 'send' | 'read' | 'tools_exposed' | 'service_api' | 'actions' | 'tools';
export type PeerStatus = 'connected' | 'needs_restart' | 'needs_permission' | 'needs_sign_in' | 'limited';

export interface PeerFollowUp {
  label: string;
  /** restart_app | open_accessibility_settings */
  action: string;
}

export interface Peer {
  id: string;
  app: string;
  bundle_id: string;
  process_name: string;
  pid?: number | null;
  transports: PeerTransport[];
  capabilities: PeerCapability[];
  bridge?: string | null;
  mcp_config_path?: string | null;
  scriptable: boolean;
  url_schemes?: string[];
  url_actions?: { id: string; description: string; template: string; verified: boolean }[];
  window_id?: string | null;
  chromium?: boolean;
  devtools_port?: number | null;
  status: PeerStatus;
  /** Bob's plain-language line about what he can do with the app. */
  bob_line: string;
  follow_up?: PeerFollowUp | null;
  paired_at: string;
  last_ok_at?: string | null;
  last_error?: string | null;
}

export type PairStage = 'identifying' | 'connecting' | 'connected' | 'failed' | 'cancelled' | 'unpaired' | 'updated';

export interface PairEvent {
  stage: PairStage;
  text: string;
  peer: Peer | null;
}

export interface PeerSendReport {
  app: string;
  via: PeerTransport;
  detail: string;
  delivered: boolean;
}

export interface PeerReadReport {
  app: string;
  via: PeerTransport;
  title: string;
  text: string;
  truncated: boolean;
}

/** One frame of the drag overlay, in overlay-local CSS pixels. */
export interface PairDragTarget {
  pid: number;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PairDragFrame {
  x: number;
  y: number;
  ox: number;
  oy: number;
  target: PairDragTarget | null;
  active: boolean;
}

/** A step only the user can take, kept visible until done (user_actions.rs). */
export interface UserAction {
  id: string;
  source: string;
  subject: string;
  label: string;
  reason: string;
  action: string;
}
