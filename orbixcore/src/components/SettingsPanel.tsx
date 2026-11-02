import { useState, useEffect, useCallback } from 'react';
import type { AppVersion, VoiceSettings } from '../types';
import { invoke } from '@tauri-apps/api/core';
import {
  getAppVersion, getMonitorSettings, setMonitorSettings, openRunMonitor,
  getAlwaysOnSettings, setAlwaysOnSettings, type AlwaysOnSettings,
  getQuickBarStatus, setQuickBarSettings, type QuickBarStatus,
  workspaceGet, workspaceSet,
  getVoiceSettings,
  bobCliInstall, type BobInstallReport,
  appBridgeList, automationCheck, automationRequest,
  wiCheckAccessibility, wiRequestAccessibility, openSystemPreferences,
  type AppBridgeStatus, type AutomationStatus,
  endpointGet, endpointSet, endpointRotateToken, type EndpointInfo,
} from '../hooks/api';
import { checkForUpdateOrThrow, installUpdate, type UpdateInfo } from '../updater';
import ModelSelector from './ModelSelector';
import { VoiceSettingsBody } from './VoicePanel';
import BrandSection from './BrandSection';
import WorkspacesSection from './WorkspacesSection';
import './Panel.css';
import './SettingsPanel.css';

interface Props {
  onClose: () => void;
  voiceSettings?: VoiceSettings;
  onVoiceSettingsChange?: (settings: VoiceSettings) => void;
  onOpenPanel?: (panel: string) => void;
}

/* ── Settings categories (left rail) ── */

type CategoryId = 'general' | 'voice' | 'brand' | 'models' | 'connectors' | 'automation' | 'knowledge' | 'privacy';

const CATEGORIES: { id: CategoryId; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'voice', label: 'Voice' },
  { id: 'brand', label: 'Brand' },
  { id: 'models', label: 'Models & Agents' },
  { id: 'connectors', label: 'Connectors' },
  { id: 'automation', label: 'Automation' },
  { id: 'knowledge', label: 'Knowledge' },
  { id: 'privacy', label: 'Data & Privacy' },
];

/* A row that opens one of the full-size editor panels. Small settings render
   inline in their category; large editors keep their panel and get launched
   from here so everything is reachable from one place. */
function Launcher({ title, desc, panel, onOpenPanel }: {
  title: string;
  desc: string;
  panel: string;
  onOpenPanel?: (panel: string) => void;
}) {
  return (
    <button className="settings-launcher" onClick={() => onOpenPanel?.(panel)}>
      <span className="settings-launcher-text">
        <span className="settings-launcher-title">{title}</span>
        <span className="settings-launcher-desc">{desc}</span>
      </span>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <polyline points="9 18 15 12 9 6" />
      </svg>
    </button>
  );
}

const PLATFORM_LABEL = (() => {
  const p = document.documentElement.dataset.platform;
  return p === 'mac' ? 'macOS' : p === 'win' ? 'Windows' : p === 'linux' ? 'Linux' : 'Desktop';
})();

/** Run monitor: the free-standing log window. Runs open it on their own
 *  unless the switch is off; the button opens it regardless. */
function RunMonitorSection() {
  const [autoOpen, setAutoOpen] = useState(true);
  useEffect(() => {
    getMonitorSettings().then(s => setAutoOpen(s.auto_open)).catch(() => {});
  }, []);
  const flip = () => {
    const next = !autoOpen;
    setAutoOpen(next);
    setMonitorSettings({ auto_open: next }).catch(() => setAutoOpen(!next));
  };
  return (
    <>
      <div className="accent-color-row" style={{ marginBottom: 10 }}>
        <span className="accent-color-label">Open when an agent starts a shift</span>
        <button
          className={`toggle-switch ${autoOpen ? 'on' : 'off'}`}
          role="switch"
          aria-checked={autoOpen}
          onClick={flip}
          title={autoOpen ? 'Runs open the monitor automatically' : 'Runs never open the monitor by themselves'}
        >
          <div className="toggle-thumb" />
        </button>
      </div>
      <button className="panel-btn full" onClick={() => openRunMonitor().catch(() => {})}>
        Open the run monitor
      </button>
    </>
  );
}

/** Background: Bob keeps running when the window is closed, and can
 *  start at login. Bob is the product; the window is one way to reach him. */
function BackgroundSection() {
  const [s, setS] = useState<AlwaysOnSettings>({ keep_running: true, launch_at_login: false });
  const [error, setError] = useState('');
  useEffect(() => {
    getAlwaysOnSettings().then(setS).catch(() => {});
  }, []);
  const where = PLATFORM_LABEL === 'macOS' ? 'menu bar' : 'system tray';
  const flip = (key: keyof AlwaysOnSettings) => {
    const prev = s;
    const next = { ...s, [key]: !s[key] };
    setS(next);
    setError('');
    setAlwaysOnSettings(next).then(setS).catch(e => { setS(prev); setError(String(e)); });
  };
  const row = (key: keyof AlwaysOnSettings, label: string, on: string, off: string) => (
    <div className="accent-color-row" style={{ marginBottom: 10 }}>
      <span className="accent-color-label">{label}</span>
      <button
        className={`toggle-switch ${s[key] ? 'on' : 'off'}`}
        role="switch"
        aria-checked={s[key]}
        onClick={() => flip(key)}
        title={s[key] ? on : off}
      >
        <div className="toggle-thumb" />
      </button>
    </div>
  );
  return (
    <>
      {row('keep_running', 'Keep Bob running when the window is closed',
        `Closing the window leaves Bob in the ${where}; Quit there stops him`,
        'Closing the window quits Orbix Core')}
      {row('launch_at_login', 'Start Bob when you log in',
        `Bob starts in the ${where} at login, without opening the window`,
        'Bob starts only when you open Orbix Core')}
      {error && <div className="form-error">{error}</div>}
    </>
  );
}

/** How a shortcut string reads on this platform: Alt+Space -> Option+Space. */
function shortcutLabel(s: string): string {
  const mac = PLATFORM_LABEL === 'macOS';
  return s.split('+').map(k => {
    if (k === 'Alt') return mac ? 'Option' : 'Alt';
    if (k === 'Super') return mac ? 'Cmd' : 'Win';
    return k;
  }).join('+');
}

/** Quick bar: the global shortcut that opens Bob over any app. */
function QuickBarSection() {
  const [st, setSt] = useState<QuickBarStatus | null>(null);
  useEffect(() => {
    getQuickBarStatus().then(setSt).catch(() => {});
  }, []);
  if (!st) return null;
  const save = (enabled: boolean, shortcut: string) =>
    setQuickBarSettings({ enabled, shortcut }).then(setSt).catch(() => {});
  const { enabled, shortcut } = st.settings;
  return (
    <>
      <div className="accent-color-row" style={{ marginBottom: 10 }}>
        <span className="accent-color-label">Open Bob from any app with {shortcutLabel(shortcut)}</span>
        <button
          className={`toggle-switch ${enabled ? 'on' : 'off'}`}
          role="switch"
          aria-checked={enabled}
          onClick={() => save(!enabled, shortcut)}
          title={enabled ? 'The shortcut opens a small Bob bar over whatever you are doing' : 'No global shortcut'}
        >
          <div className="toggle-thumb" />
        </button>
      </div>
      {enabled && (
        <div className="accent-color-row" style={{ marginBottom: 10 }}>
          <span className="accent-color-label">Shortcut</span>
          <select
            value={shortcut}
            onChange={e => save(true, e.target.value)}
            aria-label="Quick bar shortcut"
          >
            {st.choices.map(c => <option key={c} value={c}>{shortcutLabel(c)}</option>)}
          </select>
        </div>
      )}
      {enabled && st.error && <div className="form-error">{st.error}</div>}
    </>
  );
}

export default function SettingsPanel({ onClose, voiceSettings, onVoiceSettingsChange, onOpenPanel }: Props) {
  const [category, setCategory] = useState<CategoryId>('general');
  const [theme, setTheme] = useState(() =>
    document.documentElement.getAttribute('data-theme') || 'dark'
  );
  const [appVersion, setAppVersion] = useState<AppVersion | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateNote, setUpdateNote] = useState('');

  // Voice state: use the app-provided settings when given (keeps App's copy
  // fresh for auto-speak); otherwise self-load so the panel works standalone.
  const [localVoice, setLocalVoice] = useState<VoiceSettings | null>(null);
  useEffect(() => {
    if (!voiceSettings) getVoiceSettings().then(setLocalVoice).catch(() => {});
  }, [voiceSettings]);
  const effectiveVoice = voiceSettings ?? localVoice;
  const setEffectiveVoice = useCallback((s: VoiceSettings) => {
    setLocalVoice(s);
    onVoiceSettingsChange?.(s);
  }, [onVoiceSettingsChange]);

  useEffect(() => {
    getAppVersion().then(setAppVersion).catch(() => {});
  }, []);

  const handleCheckUpdates = async () => {
    setCheckingUpdate(true);
    setUpdateNote('');
    try {
      const found = await checkForUpdateOrThrow();
      setUpdate(found);
      if (!found) setUpdateNote('You have the latest version.');
    } catch (e) {
      setUpdateNote(`Could not check for updates: ${e instanceof Error ? e.message : String(e)}`);
    }
    setCheckingUpdate(false);
  };

  const handleInstallUpdate = async () => {
    if (!update) return;
    setCheckingUpdate(true);
    try {
      await installUpdate(update, (phase, pct) => {
        if (phase === 'downloading') setUpdateNote(pct != null ? `Downloading ${pct}%` : 'Downloading...');
        else if (phase === 'installing') setUpdateNote('Installing, the app will restart...');
      });
    } catch (e) {
      setUpdateNote(`Update failed: ${e instanceof Error ? e.message : String(e)}`);
      setCheckingUpdate(false);
    }
  };

  const toggleTheme = () => {
    const next = theme === 'light' ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', next);
    localStorage.setItem('orbixcore-theme', next);
    setTheme(next);
  };

  return (
    <div className="panel settings-panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
          Settings
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="settings-layout">
        <nav className="settings-rail" aria-label="Settings categories">
          {CATEGORIES.map(c => (
            <button
              key={c.id}
              className={`settings-rail-item ${category === c.id ? 'active' : ''}`}
              onClick={() => setCategory(c.id)}
            >
              {c.label}
            </button>
          ))}
        </nav>

        <div className="panel-body settings-content">

        {/* ── General: appearance, workspace, role, about ── */}
        {category === 'general' && (
          <>
            <div className="panel-section-header">Workspaces</div>
            <WorkspacesSection />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Appearance</div>
            <div className="accent-color-row" onClick={toggleTheme} style={{ cursor: 'pointer', marginBottom: 12 }}>
              <span className="accent-color-label">Theme</span>
              <span className="accent-color-label" style={{ color: 'var(--text-primary)', fontWeight: 500 }}>
                {theme === 'light' ? 'Light' : 'Dark'}
              </span>
            </div>

            {/* Where Bob's agentic loop runs file ops + shell commands on
                technical requests. The "Open Terminal" button below is an
                escape hatch for users who want to run something themselves
                in the same dir; Bob's own loop never spawns it. */}
            <div className="panel-section-header" style={{ marginTop: 20 }}>Working folder</div>

            <WorkspaceField />

            <BobCliInstallField />

            <button
              className="panel-btn full"
              style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, marginBottom: 8 }}
              onClick={() => invoke('open_claude_terminal').catch(console.error)}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="4 17 10 11 4 5" />
                <line x1="12" y1="19" x2="20" y2="19" />
              </svg>
              Open Terminal in Workspace
            </button>

            <div className="panel-section-header" style={{ marginTop: 20 }}>Quick bar</div>
            <QuickBarSection />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Background</div>
            <BackgroundSection />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Run monitor</div>
            <RunMonitorSection />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Interface</div>
            <Launcher title="How to Orbix" desc="Guides for beginners and power users - every example runs" panel="guide" onOpenPanel={onOpenPanel} />
            <Launcher title="My UI" desc="Panels Bob has built for you" panel="my-ui" onOpenPanel={onOpenPanel} />
            <Launcher title="Search" desc="Search across conversations" panel="search" onOpenPanel={onOpenPanel} />

            <div className="panel-section-header" style={{ marginTop: 20 }}>About</div>

            {appVersion && (
              <div className="about-card">
                <div className="about-row">
                  <span className="about-label">Orbix Core</span>
                  <span className="about-value">v{appVersion.version}</span>
                </div>
                <div className="about-row">
                  <span className="about-label">Build</span>
                  <span className="about-value">{appVersion.build}</span>
                </div>
                <div className="about-row">
                  <span className="about-label">Platform</span>
                  <span className="about-value">{PLATFORM_LABEL}</span>
                </div>
              </div>
            )}

            {update ? (
              <button className="panel-btn accent full" onClick={handleInstallUpdate} disabled={checkingUpdate}>
                {checkingUpdate ? 'Updating...' : `Install v${update.version} and restart`}
              </button>
            ) : (
              <button className="panel-btn full" onClick={handleCheckUpdates} disabled={checkingUpdate}>
                {checkingUpdate ? 'Checking...' : 'Check for Updates'}
              </button>
            )}
            {updateNote && <div className="panel-section-subheader" style={{ textTransform: 'none' }}>{updateNote}</div>}
          </>
        )}

        {/* ── Voice: the full TTS + STT settings, inline ── */}
        {category === 'voice' && (
          effectiveVoice ? (
            <VoiceSettingsBody settings={effectiveVoice} setSettings={setEffectiveVoice} />
          ) : (
            <div className="panel-section-header">Loading voice settings...</div>
          )
        )}

        {/* ── Brand: kit extraction + branded document preview ── */}
        {category === 'brand' && (
          <>
            <BrandSection />
            <div className="panel-section-header" style={{ marginTop: 20 }}>Generated documents</div>
            <Launcher title="Documents" desc="Preview, edit, and regenerate what Bob created" panel="documents" onOpenPanel={onOpenPanel} />
          </>
        )}

        {/* ── Models & Agents ── */}
        {category === 'models' && (
          <>
            <div className="panel-section-header">Local Models</div>
            <ModelSelector compact />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Local Endpoint</div>
            <EndpointSection />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Agents</div>
            <Launcher title="Agents" desc="Connected agents, capabilities and defaults" panel="agents" onOpenPanel={onOpenPanel} />
          </>
        )}

        {/* ── Connectors & capabilities ── */}
        {category === 'connectors' && (
          <>
            <div className="panel-section-header">Work on top of apps</div>
            <AppBridgeSection />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Connections</div>
            <Launcher title="Connectors" desc="Mail, calendar, and service connectors" panel="connectors" onOpenPanel={onOpenPanel} />
            <Launcher title="MCP Servers" desc="Model Context Protocol servers" panel="mcp" onOpenPanel={onOpenPanel} />
            <Launcher title="Plugins" desc="Installed plugins and extensions" panel="plugins" onOpenPanel={onOpenPanel} />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Capabilities</div>
            <Launcher title="Tools" desc="Bob's tool registry and approvals" panel="tools" onOpenPanel={onOpenPanel} />
            <Launcher title="Browser" desc="Chrome driving and sessions" panel="browser" onOpenPanel={onOpenPanel} />
          </>
        )}

        {/* ── Automation ── */}
        {category === 'automation' && (
          <>
            <div className="panel-section-header">Workflows</div>
            <Launcher title="Workflows" desc="Teach and edit saved workflows" panel="teach" onOpenPanel={onOpenPanel} />
            <Launcher title="Scheduler" desc="Scheduled and recurring runs" panel="scheduler" onOpenPanel={onOpenPanel} />
            <Launcher title="Triggers" desc="Event triggers that start workflows" panel="triggers" onOpenPanel={onOpenPanel} />
            <Launcher title="Chains" desc="Multi-agent orchestration chains" panel="orchestrator" onOpenPanel={onOpenPanel} />

            <div className="panel-section-header" style={{ marginTop: 20 }}>Desktop</div>
            <Launcher title="Window Intelligence" desc="App and window awareness" panel="window-intel" onOpenPanel={onOpenPanel} />
          </>
        )}

        {/* ── Knowledge & memory ── */}
        {category === 'knowledge' && (
          <>
            <div className="panel-section-header">Knowledge</div>
            <Launcher title="Knowledge Base" desc="Documents Bob can draw on" panel="knowledge" onOpenPanel={onOpenPanel} />
            <Launcher title="Memory" desc="Saved conversations and recall" panel="memory" onOpenPanel={onOpenPanel} />
          </>
        )}

        {/* ── Data & privacy ── */}
        {category === 'privacy' && (
          <>
            <div className="panel-section-header">Data</div>
            <Launcher title="Data Control" desc="Confidential folders and egress policy" panel="data-control" onOpenPanel={onOpenPanel} />
            <Launcher title="Audit Log" desc="Hash-chained record of every action" panel="audit" onOpenPanel={onOpenPanel} />
          </>
        )}

        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
//  Workspace path field
//
//  Where claude runs when Bob is in agentic mode (ORBIX_HANDOFF.md
//  Step 2). Persisted via workspace_set; the backend refuses paths
//  that aren't real directories.
// ─────────────────────────────────────────────

function WorkspaceField() {
  const [path, setPath] = useState<string>('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  useEffect(() => {
    workspaceGet()
      .then((ws) => setPath(ws.path ?? ''))
      .catch(() => {});
  }, []);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      const ws = await workspaceSet(path.trim() || null);
      setPath(ws.path ?? '');
      setSavedAt(Date.now());
    } catch (e: any) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const showSaved = savedAt !== null && Date.now() - savedAt < 2000;

  return (
    <div style={{ marginBottom: 10 }}>
      <label style={{
        display: 'block',
        fontSize: 11,
        fontWeight: 500,
        color: 'var(--text-secondary)',
        marginBottom: 4,
      }}>
        Workspace folder
        <span style={{ fontWeight: 400, color: 'var(--text-muted)', marginLeft: 6 }}>
          where Bob runs claude for build/fix requests
        </span>
      </label>
      <div style={{ display: 'flex', gap: 8 }}>
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="/Users/you/Documents/GitHub/my-project"
          spellCheck={false}
          style={{
            flex: 1,
            padding: '8px 10px',
            fontFamily: 'var(--font-mono)',
            fontSize: 12,
            background: 'var(--bg-surface)',
            border: '1px solid var(--border)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--text-primary)',
            outline: 'none',
          }}
        />
        <button
          onClick={save}
          disabled={saving}
          style={{
            padding: '8px 14px',
            fontSize: 12,
            fontWeight: 600,
            background: 'var(--accent)',
            color: '#fff',
            border: 'none',
            borderRadius: 'var(--radius-sm)',
            cursor: saving ? 'not-allowed' : 'pointer',
            opacity: saving ? 0.6 : 1,
          }}
        >
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>
      {error && (
        <p style={{ fontSize: 11, color: 'var(--error)', margin: '4px 0 0' }}>{error}</p>
      )}
      {showSaved && !error && (
        <p style={{ fontSize: 11, color: 'var(--success)', margin: '4px 0 0' }}>✓ Saved</p>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────
//  Local endpoint (FABLE B1)
//
//  Off by default. When on, any OpenAI-compatible tool (Claude Code,
//  Continue, Aider) can point at the base URL with the token and run
//  free local inference through Orbix.
// ─────────────────────────────────────────────

function EndpointSection() {
  const [ep, setEp] = useState<EndpointInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState<'url' | 'token' | 'lan' | null>(null);

  useEffect(() => {
    endpointGet().then(setEp).catch(() => {});
  }, []);

  async function toggle() {
    if (!ep) return;
    setBusy(true);
    try {
      setEp(await endpointSet(!ep.enabled));
    } catch { /* keep last known state */ }
    finally { setBusy(false); }
  }

  async function rotate() {
    setBusy(true);
    try {
      setEp(await endpointRotateToken());
    } catch { /* keep last known state */ }
    finally { setBusy(false); }
  }

  function copy(kind: 'url' | 'token' | 'lan', value: string) {
    navigator.clipboard?.writeText(value).catch(() => {});
    setCopied(kind);
    setTimeout(() => setCopied(null), 1500);
  }

  const mono: React.CSSProperties = {
    fontFamily: 'var(--font-mono)',
    fontSize: 11,
    color: 'var(--text-primary)',
    background: 'var(--bg-surface)',
    border: '1px solid var(--border)',
    borderRadius: 'var(--radius-sm)',
    padding: '5px 8px',
    flex: 1,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  };

  return (
    <div style={{ marginBottom: 4 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 12, fontWeight: 500, color: 'var(--text-primary)' }}>
            Expose local model endpoint
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 1 }}>
            Point Claude Code, Continue, or any OpenAI-compatible tool at Orbix
            and run on the free local model
          </div>
        </div>
        <button className="panel-btn" onClick={toggle} disabled={busy || !ep}>
          {ep?.enabled ? 'Turn off' : 'Turn on'}
        </button>
      </div>
      {ep?.enabled && (
        <div style={{ marginTop: 8 }}>
          <div style={{ display: 'flex', gap: 6, alignItems: 'center', marginBottom: 6 }}>
            <span style={mono}>{ep.base_url}</span>
            <span style={{ fontSize: 10, color: 'var(--text-dim)', flexShrink: 0 }}>this device</span>
            <button className="panel-btn" onClick={() => copy('url', ep.base_url)}>
              {copied === 'url' ? 'Copied' : 'Copy'}
            </button>
          </div>
          {ep.lan_url && (
            <div style={{ display: 'flex', gap: 6, alignItems: 'center', marginBottom: 6 }}>
              <span style={mono}>{ep.lan_url}</span>
              <span style={{ fontSize: 10, color: 'var(--text-dim)', flexShrink: 0 }}>other devices</span>
              <button className="panel-btn" onClick={() => copy('lan', ep.lan_url!)}>
                {copied === 'lan' ? 'Copied' : 'Copy'}
              </button>
            </div>
          )}
          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            <span style={mono}>{ep.token}</span>
            <button className="panel-btn" onClick={() => copy('token', ep.token)}>
              {copied === 'token' ? 'Copied' : 'Copy'}
            </button>
            <button className="panel-btn" onClick={rotate} disabled={busy} title="Mint a new token; update every tool that uses the old one">
              Rotate
            </button>
          </div>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: '6px 0 0', lineHeight: 1.5 }}>
            Requests stay on this device. The token is required on every call:
            Authorization: Bearer or x-api-key.
          </p>
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────
//  Work on top of apps (FABLE A4)
//
//  The zero-setup path: the user logs into their apps as usual and Bob
//  reads them. Two OS grants (Automation, Accessibility) replace every
//  per-app OAuth dialog for the read path. Shows both permission states
//  with a grant action, plus the bridges detected on this Mac.
// ─────────────────────────────────────────────

const BRIDGE_STATE: Record<AppBridgeStatus['status'], { label: string; color: string }> = {
  ready: { label: 'Ready', color: 'var(--success)' },
  limited: { label: 'Limited', color: 'var(--warning)' },
  not_running: { label: 'Closed', color: 'var(--text-muted)' },
  not_installed: { label: 'Not installed', color: 'var(--text-dim)' },
  error: { label: 'Error', color: 'var(--error)' },
};

function PermissionRow({ name, hint, ok, statusText, busy, onGrant, pane }: {
  name: string;
  hint: string;
  ok: boolean;
  statusText: string;
  busy: boolean;
  onGrant: () => void;
  pane: string;
}) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 10,
      padding: '8px 10px',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-sm)',
      marginBottom: 6,
    }}>
      <div style={{
        width: 7, height: 7, borderRadius: '50%', flexShrink: 0,
        background: ok ? 'var(--success)' : 'var(--warning)',
      }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 12, fontWeight: 500, color: 'var(--text-primary)' }}>
          {name}
          <span style={{ fontWeight: 400, color: 'var(--text-muted)', marginLeft: 6 }}>{hint}</span>
        </div>
        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 1 }}>{statusText}</div>
      </div>
      {ok ? (
        <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--success)' }}>On</span>
      ) : (
        <>
          <button className="panel-btn" onClick={onGrant} disabled={busy}>
            {busy ? 'Asking…' : 'Grant'}
          </button>
          <button
            className="panel-btn"
            title="Open System Settings"
            onClick={() => openSystemPreferences(pane).catch(() => {})}
          >
            Settings
          </button>
        </>
      )}
    </div>
  );
}

function AppBridgeSection() {
  const [automation, setAutomation] = useState<AutomationStatus | null>(null);
  const [axTrusted, setAxTrusted] = useState<boolean | null>(null);
  const [axMessage, setAxMessage] = useState('');
  const [bridges, setBridges] = useState<AppBridgeStatus[]>([]);
  const [busy, setBusy] = useState<'automation' | 'accessibility' | null>(null);

  const refresh = useCallback(() => {
    automationCheck().then(setAutomation).catch(() => {});
    wiCheckAccessibility()
      .then((s) => { setAxTrusted(s.trusted); setAxMessage(s.message); })
      .catch(() => {});
    appBridgeList().then(setBridges).catch(() => {});
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  async function grantAutomation() {
    setBusy('automation');
    try {
      const s = await automationRequest();
      setAutomation(s);
      if (s.state === 'denied') await openSystemPreferences('automation');
    } catch { /* surface stays on last known state */ }
    finally {
      setBusy(null);
      appBridgeList().then(setBridges).catch(() => {});
    }
  }

  async function grantAccessibility() {
    setBusy('accessibility');
    try {
      const s = await wiRequestAccessibility();
      setAxTrusted(s.trusted);
      setAxMessage(s.message);
    } catch { /* surface stays on last known state */ }
    finally { setBusy(null); }
  }

  return (
    <div style={{ marginBottom: 4 }}>
      <p style={{ fontSize: 11.5, color: 'var(--text-muted)', margin: '0 0 8px', lineHeight: 1.5 }}>
        Log in to your apps as usual - Bob reads them for you. No account setup,
        nothing leaves this device.
      </p>
      <PermissionRow
        name="Automation"
        hint="reads scriptable apps: mail, calendar, notes"
        ok={automation?.granted ?? false}
        statusText={automation?.message ?? 'Checking…'}
        busy={busy === 'automation'}
        onGrant={grantAutomation}
        pane="automation"
      />
      <PermissionRow
        name="Accessibility"
        hint="reads any app window when there is no bridge"
        ok={axTrusted ?? false}
        statusText={axTrusted === null ? 'Checking…' : axMessage}
        busy={busy === 'accessibility'}
        onGrant={grantAccessibility}
        pane="accessibility"
      />
      {bridges.length > 0 && (
        <>
          <div style={{
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            margin: '10px 0 6px',
          }}>
            <span style={{ fontSize: 11, fontWeight: 500, color: 'var(--text-secondary)' }}>
              Detected apps
            </span>
            <button
              onClick={refresh}
              style={{
                fontSize: 11, color: 'var(--text-muted)', background: 'none',
                border: 'none', cursor: 'pointer', padding: 0,
              }}
            >
              Recheck
            </button>
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            {bridges.map((b) => {
              const cfg = BRIDGE_STATE[b.status] ?? BRIDGE_STATE.error;
              return (
                <span
                  key={b.app}
                  title={b.detail || b.display_name}
                  style={{
                    display: 'inline-flex', alignItems: 'center', gap: 5,
                    padding: '3px 9px',
                    fontSize: 11,
                    background: 'var(--bg-surface)',
                    border: '1px solid var(--border)',
                    borderRadius: 999,
                    color: 'var(--text-primary)',
                  }}
                >
                  <span style={{ width: 5, height: 5, borderRadius: '50%', background: cfg.color }} />
                  {b.app}
                  <span style={{ color: cfg.color }}>{cfg.label}</span>
                </span>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────
//  bob CLI install
//
//  Copies the bundled `bob` binary out of the .app's Resources into
//  ~/.local/bin/bob so the user can type `bob` from any terminal.
//  Reports whether ~/.local/bin is on PATH; if not, shows the shell
//  rc line to add.
// ─────────────────────────────────────────────

function BobCliInstallField() {
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<BobInstallReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function install() {
    setBusy(true);
    setError(null);
    try {
      const r = await bobCliInstall();
      setReport(r);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ marginBottom: 10 }}>
      <label style={{
        display: 'block',
        fontSize: 11,
        fontWeight: 500,
        color: 'var(--text-secondary)',
        marginBottom: 4,
      }}>
        Terminal access
        <span style={{ fontWeight: 400, color: 'var(--text-muted)', marginLeft: 6 }}>
          install `bob` so you can `cd` into a repo and ask Bob from your shell
        </span>
      </label>
      <button
        onClick={install}
        disabled={busy}
        style={{
          width: '100%',
          padding: '8px 12px',
          fontSize: 12,
          fontWeight: 500,
          background: 'var(--bg-surface)',
          color: 'var(--text-primary)',
          border: '1px solid var(--border)',
          borderRadius: 'var(--radius-sm)',
          cursor: busy ? 'not-allowed' : 'pointer',
          opacity: busy ? 0.6 : 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 8,
        }}
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="8 9 12 5 16 9" />
          <line x1="12" y1="5" x2="12" y2="15" />
          <path d="M20 16v3a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-3" />
        </svg>
        {busy ? 'Installing…' : report ? 'Reinstall bob CLI' : 'Install bob CLI'}
      </button>
      {error && (
        <p style={{ fontSize: 11, color: 'var(--error)', margin: '4px 0 0' }}>{error}</p>
      )}
      {report && !error && (
        <div style={{ marginTop: 6 }}>
          <p style={{ fontSize: 11, color: 'var(--success)', margin: 0 }}>
            ✓ Installed at <code>{report.installed_at}</code>
            {' · '}
            {(report.version_bytes / 1024 / 1024).toFixed(1)}MB
          </p>
          {!report.on_path && (
            <p style={{ fontSize: 11, color: 'var(--warning)', margin: '4px 0 0', lineHeight: 1.5 }}>
              ~/.local/bin is not on your PATH. Add this to{' '}
              <code>{report.shell_rc_hint ?? '~/.zshrc'}</code>:
              <br />
              <code style={{ display: 'block', padding: '4px 8px', marginTop: 4, background: 'var(--bg-surface)', borderRadius: 4 }}>
                export PATH="$HOME/.local/bin:$PATH"
              </code>
            </p>
          )}
        </div>
      )}
    </div>
  );
}
