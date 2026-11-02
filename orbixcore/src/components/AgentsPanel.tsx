import { useState, useEffect } from 'react';
import type { Agent, AgentInput, AgentProvider, AgentCapabilities, UniversalConnector } from '../types';
import {
  listAgents, addAgent, updateAgent, removeAgent, toggleAgent, setDefaultAgent,
  listUniversalConnectors, updateAgentConnectors, updateAgentCapabilities,
  listCapabilityPresets,
  updateAgentEmployee, runAgentShift, listAgentGrantableTools, openRunMonitor,
} from '../hooks/api';
import './Panel.css';
import './AgentsPanel.css';

interface Props {
  agents: Agent[];
  setAgents: (agents: Agent[]) => void;
  onClose: () => void;
}

const PROVIDER_INFO: Record<AgentProvider, { label: string; defaultEndpoint: string; defaultModel: string }> = {
  local:     { label: 'Local (Gemma / Qwen)', defaultEndpoint: 'http://127.0.0.1:8847', defaultModel: 'gemma-4-27b' },
  anthropic: { label: 'Anthropic',       defaultEndpoint: 'https://api.anthropic.com', defaultModel: 'claude-sonnet-4-20250514' },
  openai:    { label: 'OpenAI',          defaultEndpoint: 'https://api.openai.com',     defaultModel: 'gpt-4o' },
  ollama:    { label: 'Ollama',          defaultEndpoint: 'http://localhost:11434',      defaultModel: 'llama3.2' },
  lmstudio:  { label: 'LM Studio',      defaultEndpoint: 'http://localhost:1234',       defaultModel: 'local-model' },
  custom:    { label: 'Custom',          defaultEndpoint: '',                            defaultModel: '' },
};

const PROVIDER_COLORS: Record<AgentProvider, string> = {
  local:     '#3b82f6',
  anthropic: '#ec4899',
  openai:    '#34d399',
  ollama:    '#fbbf24',
  lmstudio:  '#a855f7',
  custom:    '#94a3b8',
};

const CLAUDE_MODELS = [
  { id: 'claude-opus-4-20250514', name: 'Claude Opus 4', desc: 'Most capable - deep reasoning, complex tasks' },
  { id: 'claude-sonnet-4-20250514', name: 'Claude Sonnet 4', desc: 'Best balance - fast, capable, cost-effective' },
  { id: 'claude-haiku-4-5-20251001', name: 'Claude Haiku 4.5', desc: 'Fastest - lightweight, low latency' },
];

const emptyForm = (): AgentInput => ({
  name: '',
  provider: 'local',
  model: 'local',
  endpoint: '',
  api_key: '',
  is_default: false,
});

const emptyCapabilities = (): AgentCapabilities => ({
  skills: [],
  route_patterns: [],
  keywords: [],
  priority: 0,
});

export default function AgentsPanel({ agents, setAgents, onClose }: Props) {
  const [view, setView] = useState<'list' | 'add' | 'edit'>('list');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<AgentInput>(emptyForm());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [universalConnectors, setUniversalConnectors] = useState<UniversalConnector[]>([]);
  const [bindingAgent, setBindingAgent] = useState<string | null>(null);
  const [editingCaps, setEditingCaps] = useState<string | null>(null);
  // ── P2 employee editor state ──
  const [employeeEditing, setEmployeeEditing] = useState<string | null>(null);
  const [employeeForm, setEmployeeForm] = useState<{ charter: string; granted_tools: string[]; schedule_minutes: number | null }>({ charter: '', granted_tools: [], schedule_minutes: null });
  const [grantable, setGrantable] = useState<{ name: string; description: string }[]>([]);
  const [shiftRunning, setShiftRunning] = useState<string | null>(null);
  const [shiftReport, setShiftReport] = useState<{ id: string; text: string; ok: boolean } | null>(null);

  useEffect(() => {
    listAgentGrantableTools().then(setGrantable).catch(() => {});
  }, []);

  const handleEditEmployee = (agent: Agent) => {
    if (employeeEditing === agent.id) { setEmployeeEditing(null); return; }
    setEmployeeEditing(agent.id);
    setShiftReport(null);
    setEmployeeForm({
      charter: agent.charter ?? '',
      granted_tools: agent.granted_tools ?? [],
      schedule_minutes: agent.schedule_minutes ?? null,
    });
  };

  const handleSaveEmployee = async () => {
    if (!employeeEditing) return;
    try {
      const updated = await updateAgentEmployee(
        employeeEditing, employeeForm.charter, employeeForm.granted_tools, employeeForm.schedule_minutes,
      );
      setAgents(agents.map(a => a.id === updated.id ? updated : a));
      setEmployeeEditing(null);
    } catch { /* */ }
  };

  const handleRunShift = async (agentId: string) => {
    setShiftRunning(agentId);
    setShiftReport(null);
    try {
      const report = await runAgentShift(agentId);
      setShiftReport({ id: agentId, text: report || '(no report)', ok: true });
      // refresh last_run_at
      listAgents().then(setAgents).catch(() => {});
    } catch (e) {
      setShiftReport({ id: agentId, text: String(e), ok: false });
    } finally {
      setShiftRunning(null);
    }
  };
  const [capsForm, setCapsForm] = useState<AgentCapabilities>(emptyCapabilities());
  const [presets, setPresets] = useState<[string, AgentCapabilities][]>([]);
  const [capsInput, setCapsInput] = useState({ skill: '', keyword: '', pattern: '' });

  useEffect(() => {
    listAgents().then(setAgents).catch(() => {});
    listUniversalConnectors().then(setUniversalConnectors).catch(() => {});
    listCapabilityPresets().then(setPresets).catch(() => {});
  }, []);

  const handleProviderChange = (p: AgentProvider) => {
    const info = PROVIDER_INFO[p];
    setForm(f => ({ ...f, provider: p, endpoint: info.defaultEndpoint, model: info.defaultModel }));
  };

  const handleAdd = async () => {
    if (!form.name.trim()) { setError('Name is required'); return; }
    setLoading(true);
    setError('');
    try {
      const agent = await addAgent(form);
      setAgents([...agents, agent]);
      setView('list');
      setForm(emptyForm());
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to add agent');
    } finally {
      setLoading(false);
    }
  };

  const handleUpdate = async () => {
    if (!editingId || !form.name.trim()) { setError('Name is required'); return; }
    setLoading(true);
    setError('');
    try {
      const updated = await updateAgent(editingId, form);
      setAgents(agents.map(a => a.id === editingId ? updated : a));
      setView('list');
      setEditingId(null);
      setForm(emptyForm());
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to update agent');
    } finally {
      setLoading(false);
    }
  };

  const handleEdit = (agent: Agent) => {
    setEditingId(agent.id);
    setForm({
      name: agent.name,
      provider: agent.provider,
      model: agent.model,
      endpoint: agent.endpoint,
      api_key: agent.api_key,
      is_default: agent.is_default,
    });
    setError('');
    setView('edit');
  };

  const handleRemove = async (id: string) => {
    await removeAgent(id).catch(() => {});
    setAgents(agents.filter(a => a.id !== id));
  };

  const handleToggle = async (id: string) => {
    const updated = await toggleAgent(id).catch(() => null);
    if (updated) setAgents(agents.map(a => a.id === id ? updated : a));
  };

  const handleSetDefault = async (id: string) => {
    await setDefaultAgent(id).catch(() => {});
    setAgents(agents.map(a => ({ ...a, is_default: a.id === id })));
  };

  const handleToggleConnector = async (agentId: string, connectorId: string) => {
    const agent = agents.find(a => a.id === agentId);
    if (!agent) return;
    const current = agent.connector_ids || [];
    const updated = current.includes(connectorId)
      ? current.filter(id => id !== connectorId)
      : [...current, connectorId];
    try {
      await updateAgentConnectors(agentId, updated);
      setAgents(agents.map(a => a.id === agentId ? { ...a, connector_ids: updated } : a));
    } catch { /* */ }
  };

  const handleEditCaps = (agentId: string) => {
    if (editingCaps === agentId) {
      setEditingCaps(null);
      return;
    }
    const agent = agents.find(a => a.id === agentId);
    setCapsForm(agent?.capabilities || emptyCapabilities());
    setCapsInput({ skill: '', keyword: '', pattern: '' });
    setEditingCaps(agentId);
  };

  const handleSaveCaps = async () => {
    if (!editingCaps) return;
    try {
      const updated = await updateAgentCapabilities(editingCaps, capsForm);
      setAgents(agents.map(a => a.id === editingCaps ? updated : a));
      setEditingCaps(null);
    } catch { /* */ }
  };

  const handleApplyPreset = (preset: AgentCapabilities) => {
    setCapsForm(preset);
  };

  const addSkill = () => {
    const v = capsInput.skill.trim();
    if (!v || capsForm.skills.includes(v)) return;
    setCapsForm(f => ({ ...f, skills: [...f.skills, v] }));
    setCapsInput(p => ({ ...p, skill: '' }));
  };

  const addKeyword = () => {
    const v = capsInput.keyword.trim();
    if (!v || capsForm.keywords.includes(v)) return;
    setCapsForm(f => ({ ...f, keywords: [...f.keywords, v] }));
    setCapsInput(p => ({ ...p, keyword: '' }));
  };

  const addPattern = () => {
    const v = capsInput.pattern.trim();
    if (!v || capsForm.route_patterns.includes(v)) return;
    setCapsForm(f => ({ ...f, route_patterns: [...f.route_patterns, v] }));
    setCapsInput(p => ({ ...p, pattern: '' }));
  };

  // Shared form UI for both add and edit
  const renderForm = (isEdit: boolean) => (
    <div className="panel-form animate-fade-up">
      <button className="back-btn" onClick={() => { setView('list'); setEditingId(null); setError(''); }}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="15 18 9 12 15 6"/>
        </svg>
        Back
      </button>
      <h3 className="form-title">{isEdit ? 'Edit Agent' : 'Connect Agent'}</h3>

      <div className="form-field">
        <label>Name</label>
        <input
          className="form-input"
          placeholder="e.g. Claude Sonnet"
          value={form.name}
          onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
        />
      </div>

      <div className="form-field">
        <label>Provider</label>
        <div className="provider-grid">
          {(Object.keys(PROVIDER_INFO) as AgentProvider[]).map(p => (
            <button
              key={p}
              className={`provider-btn ${form.provider === p ? 'active' : ''}`}
              onClick={() => handleProviderChange(p)}
              style={{ '--provider-color': PROVIDER_COLORS[p] } as React.CSSProperties}
            >
              <div className="provider-dot" style={{ background: PROVIDER_COLORS[p] }} />
              {PROVIDER_INFO[p].label}
            </button>
          ))}
        </div>
      </div>

      {form.provider !== 'local' && (
        <>
          <div className="form-field">
            <label>Model</label>
            {form.provider === 'anthropic' ? (
              <div className="claude-model-list">
                {CLAUDE_MODELS.map(m => (
                  <button
                    key={m.id}
                    className={`claude-model-option ${form.model === m.id ? 'selected' : ''}`}
                    onClick={() => setForm(f => ({ ...f, model: m.id }))}
                  >
                    <div className="claude-model-radio">
                      <div className={`claude-model-dot ${form.model === m.id ? 'active' : ''}`} />
                    </div>
                    <div className="claude-model-info">
                      <div className="claude-model-name">{m.name}</div>
                      <div className="claude-model-desc">{m.desc}</div>
                    </div>
                  </button>
                ))}
              </div>
            ) : (
              <input
                className="form-input"
                placeholder="Model name"
                value={form.model}
                onChange={e => setForm(f => ({ ...f, model: e.target.value }))}
              />
            )}
          </div>

          <div className="form-field">
            <label>Endpoint URL</label>
            <input
              className="form-input"
              placeholder="https://api.anthropic.com"
              value={form.endpoint}
              onChange={e => setForm(f => ({ ...f, endpoint: e.target.value }))}
            />
          </div>

          <div className="form-field">
            <label>API Key <span className="label-sub">(stored locally)</span></label>
            <input
              className="form-input"
              type="password"
              placeholder={isEdit ? '(unchanged)' : 'sk-...'}
              value={form.api_key}
              onChange={e => setForm(f => ({ ...f, api_key: e.target.value }))}
            />
          </div>
        </>
      )}

      {form.provider === 'local' && (
        <div className="form-hint">
          This agent runs on your downloaded local model - no API key or endpoint needed.
        </div>
      )}

      <div className="form-check">
        <input
          type="checkbox"
          id="isDefault"
          checked={form.is_default}
          onChange={e => setForm(f => ({ ...f, is_default: e.target.checked }))}
        />
        <label htmlFor="isDefault">Set as default agent</label>
      </div>

      {error && <div className="form-error">{error}</div>}

      <button
        className="panel-btn accent full"
        onClick={isEdit ? handleUpdate : handleAdd}
        disabled={loading}
      >
        {loading ? (isEdit ? 'Saving...' : 'Connecting...') : (isEdit ? 'Save Changes' : 'Connect Agent')}
      </button>
    </div>
  );

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="8" r="4"/>
            <path d="M4 20c0-4 3.6-7 8-7s8 3 8 7"/>
          </svg>
          Agents
        </div>
        <div className="panel-header-actions">
          {view === 'list' && (
            <button className="panel-btn" onClick={() => openRunMonitor().catch(() => {})} title="Open the run monitor window">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="4" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="18" x2="12" y2="21"/>
              </svg>
              Monitor
            </button>
          )}
          {view === 'list' && (
            <button className="panel-btn accent" onClick={() => setView('add')}>
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              Add
            </button>
          )}
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body">
        {view === 'list' ? (
          <>
            {agents.length === 0 ? (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                    <circle cx="12" cy="8" r="4"/>
                    <path d="M4 20c0-4 3.6-7 8-7s8 3 8 7"/>
                  </svg>
                </div>
                <p>No agents connected</p>
                <p className="panel-empty-sub">Create a local agent or connect Claude, GPT-4, Ollama</p>
                <button className="panel-btn accent" onClick={() => setView('add')}>Connect Agent</button>
              </div>
            ) : (
              <div className="agent-list">
                {agents.map(agent => {
                  const caps = agent.capabilities;
                  const hasCapabilities = caps && (caps.skills.length > 0 || caps.keywords.length > 0 || caps.route_patterns.length > 0);

                  return (
                    <div key={agent.id} className={`agent-card ${agent.enabled ? '' : 'disabled'} ${agent.is_default ? 'default' : ''}`}>
                      <div className="agent-card-top">
                        <div className="agent-provider-dot" style={{ background: PROVIDER_COLORS[agent.provider] }} />
                        <div className="agent-info">
                          <div className="agent-name">
                            {agent.name}
                            {agent.is_default && <span className="default-badge">Default</span>}
                          </div>
                          <div className="agent-meta">
                            {PROVIDER_INFO[agent.provider].label} · {agent.model}
                          </div>
                        </div>
                        <div className="agent-actions">
                          <button
                            className={`toggle-switch ${agent.enabled ? 'on' : 'off'}`} role="switch" aria-checked={agent.enabled}
                            onClick={() => handleToggle(agent.id)}
                            title={agent.enabled ? 'Disable' : 'Enable'}
                          >
                            <div className="toggle-thumb" />
                          </button>
                        </div>
                      </div>

                      {/* Capability chips */}
                      {hasCapabilities && (
                        <div className="agent-caps-row">
                          {caps!.skills.slice(0, 4).map(s => (
                            <span key={s} className="agent-cap-chip skill">{s}</span>
                          ))}
                          {caps!.keywords.slice(0, 3).map(k => (
                            <span key={k} className="agent-cap-chip keyword">{k}</span>
                          ))}
                          {(caps!.skills.length + caps!.keywords.length > 7) && (
                            <span className="agent-cap-more">+{caps!.skills.length + caps!.keywords.length - 7}</span>
                          )}
                        </div>
                      )}

                      {employeeEditing === agent.id && (
                        <div className="agent-employee-editor" onClick={e => e.stopPropagation()}>
                          <label className="form-label">Charter - what is this agent's job?</label>
                          <textarea
                            className="form-input"
                            rows={3}
                            placeholder="Watch my calendar every morning and summarize the day. Flag conflicts and gaps longer than 2 hours."
                            value={employeeForm.charter}
                            onChange={e => setEmployeeForm(f => ({ ...f, charter: e.target.value }))}
                          />
                          <label className="form-label" style={{ marginTop: 8 }}>Granted tools ({employeeForm.granted_tools.length || 'safe defaults'})</label>
                          <div className="agent-grants">
                            {grantable.map(t => (
                              <label key={t.name} className="agent-grant" title={t.description}>
                                <input
                                  type="checkbox"
                                  checked={employeeForm.granted_tools.includes(t.name)}
                                  onChange={e => setEmployeeForm(f => ({
                                    ...f,
                                    granted_tools: e.target.checked
                                      ? [...f.granted_tools, t.name]
                                      : f.granted_tools.filter(n => n !== t.name),
                                  }))}
                                />
                                <span>{t.name}</span>
                              </label>
                            ))}
                          </div>
                          <div className="agent-employee-row">
                            <label className="form-label" style={{ margin: 0 }}>Wakes up</label>
                            <select
                              className="form-input form-select"
                              style={{ width: 'auto' }}
                              value={employeeForm.schedule_minutes ?? ''}
                              onChange={e => setEmployeeForm(f => ({ ...f, schedule_minutes: e.target.value ? Number(e.target.value) : null }))}
                            >
                              <option value="">Manually only</option>
                              <option value="15">Every 15 minutes</option>
                              <option value="60">Every hour</option>
                              <option value="240">Every 4 hours</option>
                              <option value="1440">Daily</option>
                            </select>
                            <span style={{ flex: 1 }} />
                            <button className="panel-btn" onClick={() => setEmployeeEditing(null)}>Cancel</button>
                            <button className="panel-btn accent" onClick={handleSaveEmployee}>Save</button>
                          </div>
                          <div className="agent-employee-row" style={{ marginTop: 6 }}>
                            <button
                              className="panel-btn full"
                              disabled={shiftRunning === agent.id || !(employeeForm.charter.trim() || (agent.charter ?? '').trim())}
                              onClick={() => handleRunShift(agent.id)}
                            >
                              {shiftRunning === agent.id ? 'On shift...' : 'Run a shift now'}
                            </button>
                          </div>
                          {agent.last_run_at && (
                            <span className="agent-employee-last">Last shift: {new Date(agent.last_run_at).toLocaleString()}</span>
                          )}
                          {shiftReport && shiftReport.id === agent.id && (
                            <div className={`agent-shift-report ${shiftReport.ok ? '' : 'err'}`}>{shiftReport.text}</div>
                          )}
                        </div>
                      )}

                      <div className="agent-card-bottom">
                        <span className="agent-endpoint">{agent.provider === 'local' ? 'Runs on local model' : agent.endpoint}</span>
                        <div className="agent-foot-actions">
                          <button className="link-btn" onClick={() => handleEdit(agent)}>
                            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                            </svg>
                            Edit
                          </button>
                          <button className="link-btn" onClick={() => handleEditEmployee(agent)}>
                            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <circle cx="12" cy="8" r="4"/><path d="M4 20c0-4 3.6-7 8-7s8 3 8 7"/>
                            </svg>
                            Employee{(agent.charter ?? '').trim() ? ' ✓' : ''}
                          </button>
                          <button className="link-btn" onClick={() => handleEditCaps(agent.id)}>
                            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10"/>
                            </svg>
                            Routing
                          </button>
                          {universalConnectors.length > 0 && (
                            <button className="link-btn" onClick={() => setBindingAgent(bindingAgent === agent.id ? null : agent.id)}>
                              Connectors{(agent.connector_ids?.length || 0) > 0 ? ` (${agent.connector_ids!.length})` : ''}
                            </button>
                          )}
                          {!agent.is_default && (
                            <button className="link-btn" onClick={() => handleSetDefault(agent.id)}>
                              Set Default
                            </button>
                          )}
                          <button className="link-btn danger" onClick={() => handleRemove(agent.id)}>
                            Remove
                          </button>
                        </div>
                      </div>

                      {/* Connector binding panel */}
                      {bindingAgent === agent.id && universalConnectors.length > 0 && (
                        <div className="agent-connector-bind">
                          <div className="agent-bind-label">Bind to connectors:</div>
                          {universalConnectors.map(uc => {
                            const bound = (agent.connector_ids || []).includes(uc.id);
                            return (
                              <button
                                key={uc.id}
                                className={`agent-bind-chip ${bound ? 'active' : ''}`}
                                onClick={() => handleToggleConnector(agent.id, uc.id)}
                              >
                                <span className="agent-bind-dot" style={{ background: bound ? '#34d399' : 'var(--text-dim)' }} />
                                {uc.name}
                              </button>
                            );
                          })}
                        </div>
                      )}

                      {/* Capabilities editor */}
                      {editingCaps === agent.id && (
                        <div className="agent-caps-editor">
                          {/* Preset buttons */}
                          {presets.length > 0 && (
                            <div className="caps-presets">
                              <span className="caps-preset-label">Presets:</span>
                              {presets.map(([role, preset]) => (
                                <button key={role} className="caps-preset-btn" onClick={() => handleApplyPreset(preset)}>
                                  {role}
                                </button>
                              ))}
                            </div>
                          )}

                          {/* Skills */}
                          <div className="caps-section">
                            <div className="caps-section-title">Skills</div>
                            <div className="caps-chip-list">
                              {capsForm.skills.map(s => (
                                <span key={s} className="caps-chip">
                                  {s}
                                  <button className="caps-chip-x" onClick={() => setCapsForm(f => ({ ...f, skills: f.skills.filter(x => x !== s) }))}>x</button>
                                </span>
                              ))}
                            </div>
                            <div className="caps-add-row">
                              <input
                                className="caps-input"
                                placeholder="Add skill..."
                                value={capsInput.skill}
                                onChange={e => setCapsInput(p => ({ ...p, skill: e.target.value }))}
                                onKeyDown={e => e.key === 'Enter' && addSkill()}
                              />
                              <button className="caps-add-btn" onClick={addSkill}>+</button>
                            </div>
                          </div>

                          {/* Keywords */}
                          <div className="caps-section">
                            <div className="caps-section-title">Keywords</div>
                            <div className="caps-chip-list">
                              {capsForm.keywords.map(k => (
                                <span key={k} className="caps-chip keyword">
                                  {k}
                                  <button className="caps-chip-x" onClick={() => setCapsForm(f => ({ ...f, keywords: f.keywords.filter(x => x !== k) }))}>x</button>
                                </span>
                              ))}
                            </div>
                            <div className="caps-add-row">
                              <input
                                className="caps-input"
                                placeholder="Add keyword..."
                                value={capsInput.keyword}
                                onChange={e => setCapsInput(p => ({ ...p, keyword: e.target.value }))}
                                onKeyDown={e => e.key === 'Enter' && addKeyword()}
                              />
                              <button className="caps-add-btn" onClick={addKeyword}>+</button>
                            </div>
                          </div>

                          {/* Route patterns */}
                          <div className="caps-section">
                            <div className="caps-section-title">Route Patterns</div>
                            <div className="caps-chip-list">
                              {capsForm.route_patterns.map(p => (
                                <span key={p} className="caps-chip pattern">
                                  {p}
                                  <button className="caps-chip-x" onClick={() => setCapsForm(f => ({ ...f, route_patterns: f.route_patterns.filter(x => x !== p) }))}>x</button>
                                </span>
                              ))}
                            </div>
                            <div className="caps-add-row">
                              <input
                                className="caps-input"
                                placeholder="/api/..."
                                value={capsInput.pattern}
                                onChange={e => setCapsInput(p => ({ ...p, pattern: e.target.value }))}
                                onKeyDown={e => e.key === 'Enter' && addPattern()}
                              />
                              <button className="caps-add-btn" onClick={addPattern}>+</button>
                            </div>
                          </div>

                          {/* Priority */}
                          <div className="caps-section">
                            <div className="caps-section-title">Priority</div>
                            <input
                              type="range"
                              min="-5"
                              max="10"
                              value={capsForm.priority}
                              onChange={e => setCapsForm(f => ({ ...f, priority: parseInt(e.target.value) }))}
                              className="caps-priority-slider"
                            />
                            <span className="caps-priority-val">{capsForm.priority}</span>
                          </div>

                          <div className="caps-actions">
                            <button className="panel-btn accent" onClick={handleSaveCaps}>Save Routing</button>
                            <button className="link-btn" onClick={() => setEditingCaps(null)}>Cancel</button>
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </>
        ) : (
          renderForm(view === 'edit')
        )}
      </div>
    </div>
  );
}
