import { useState, useEffect } from 'react';
import type { AgentChain, AgentChainNode, ChainMode, ChainRunState, ChainTemplate, Agent } from '../types';
import {
  listChains, createChain, deleteChain, listChainTemplates,
  runChain, getChainHistory, listAgents,
} from '../hooks/api';
import './OrchestratorPanel.css';

const MODE_LABELS: Record<ChainMode, string> = {
  sequential: 'Sequential',
  fan_out: 'Fan-out / Fan-in',
  fallback: 'Fallback',
  specialist: 'Specialist Router',
};

const MODE_ICONS: Record<ChainMode, string> = {
  sequential: '→',
  fan_out: '⇉',
  fallback: '↻',
  specialist: '◎',
};

interface Props { onClose: () => void }

export default function OrchestratorPanel({ onClose }: Props) {
  const [chains, setChains] = useState<AgentChain[]>([]);
  const [templates, setTemplates] = useState<ChainTemplate[]>([]);
  const [history, setHistory] = useState<ChainRunState[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [tab, setTab] = useState<'chains' | 'templates' | 'history'>('chains');
  const [creating, setCreating] = useState(false);
  const [runInput, setRunInput] = useState<{ chainId: string; text: string } | null>(null);
  const [running, setRunning] = useState(false);

  // Create form
  const [newName, setNewName] = useState('');
  const [newDesc, setNewDesc] = useState('');
  const [newMode, setNewMode] = useState<ChainMode>('sequential');
  const [newNodes, setNewNodes] = useState<Partial<AgentChainNode>[]>([]);

  useEffect(() => {
    listChains().then(setChains).catch(() => {});
    listChainTemplates().then(setTemplates).catch(() => {});
    getChainHistory().then(setHistory).catch(() => {});
    listAgents().then(setAgents).catch(() => {});
  }, []);

  const enabledAgents = agents.filter(a => a.enabled);

  const handleCreateFromTemplate = (tpl: ChainTemplate) => {
    setNewName(tpl.name);
    setNewDesc(tpl.description);
    setNewMode(tpl.mode);
    setNewNodes(tpl.node_hints.map((h, i) => ({
      id: crypto.randomUUID(),
      agent_id: '',
      agent_name: '',
      order: i,
      role_hint: h.role_hint,
      route_keywords: h.route_keywords,
      timeout_secs: 60,
      retry_on_fail: false,
    })));
    setCreating(true);
    setTab('chains');
  };

  const handleCreate = async () => {
    if (!newName || newNodes.length === 0) return;
    // Fill in agent names
    const filledNodes: AgentChainNode[] = newNodes.map((n, i) => ({
      id: n.id || crypto.randomUUID(),
      agent_id: n.agent_id || '',
      agent_name: n.agent_name || enabledAgents.find(a => a.id === n.agent_id)?.name || 'Unassigned',
      order: i,
      role_hint: n.role_hint || `agent_${i}`,
      route_keywords: n.route_keywords || [],
      timeout_secs: n.timeout_secs || 60,
      retry_on_fail: n.retry_on_fail || false,
    }));

    try {
      const chain = await createChain(newName, newDesc, newMode, filledNodes);
      setChains(prev => [chain, ...prev]);
      setCreating(false);
      resetForm();
    } catch (err) { console.error(err); }
  };

  const resetForm = () => {
    setNewName(''); setNewDesc(''); setNewMode('sequential'); setNewNodes([]);
  };

  const handleDelete = async (id: string) => {
    await deleteChain(id).catch(() => {});
    setChains(prev => prev.filter(c => c.id !== id));
  };

  const handleRun = async () => {
    if (!runInput) return;
    setRunning(true);
    try {
      const result = await runChain(runInput.chainId, runInput.text);
      setHistory(prev => [result, ...prev]);
    } catch (err) { console.error(err); }
    setRunning(false);
    setRunInput(null);
  };

  const addNode = () => {
    setNewNodes(prev => [...prev, {
      id: crypto.randomUUID(),
      agent_id: '',
      agent_name: '',
      order: prev.length,
      role_hint: '',
      route_keywords: [],
      timeout_secs: 60,
      retry_on_fail: false,
    }]);
  };

  const updateNode = (idx: number, field: string, value: string) => {
    setNewNodes(prev => prev.map((n, i) => {
      if (i !== idx) return n;
      if (field === 'agent_id') {
        const agent = enabledAgents.find(a => a.id === value);
        return { ...n, agent_id: value, agent_name: agent?.name || '' };
      }
      return { ...n, [field]: value };
    }));
  };

  const removeNode = (idx: number) => {
    setNewNodes(prev => prev.filter((_, i) => i !== idx));
  };

  const formatTime = (iso: string | null) => {
    if (!iso) return '-';
    return new Date(iso).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="5" cy="12" r="3"/><circle cx="19" cy="6" r="3"/><circle cx="19" cy="18" r="3"/>
            <path d="M8 12h4l3-6M12 12l3 6"/>
          </svg>
          Orchestrator
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="orch-tabs">
        {(['chains', 'templates', 'history'] as const).map(t => (
          <button key={t} className={tab === t ? 'active' : ''} onClick={() => setTab(t)}>
            {t === 'chains' ? 'Chains' : t === 'templates' ? 'Templates' : 'History'}
            {t === 'chains' && chains.length > 0 && <span className="orch-tab-count">{chains.length}</span>}
          </button>
        ))}
      </div>

      <div className="panel-body">
        {/* ── Chains Tab ── */}
        {tab === 'chains' && (
          <>
            {!creating && !runInput && (
              <button className="panel-btn accent full" onClick={() => setCreating(true)} style={{ marginBottom: 12 }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                </svg>
                New Chain
              </button>
            )}

            {/* Run input */}
            {runInput && (
              <div className="orch-run-form animate-fade-up">
                <div className="form-field">
                  <label>Input for chain</label>
                  <textarea
                    className="form-input"
                    rows={3}
                    placeholder="Describe the task..."
                    value={runInput.text}
                    onChange={e => setRunInput({ ...runInput, text: e.target.value })}
                  />
                </div>
                <div className="sched-form-actions">
                  <button className="panel-btn" onClick={() => setRunInput(null)}>Cancel</button>
                  <button
                    className="panel-btn accent"
                    onClick={handleRun}
                    disabled={!runInput.text.trim() || running}
                  >
                    {running ? 'Running...' : 'Run Chain'}
                  </button>
                </div>
              </div>
            )}

            {/* Create form */}
            {creating && (
              <div className="orch-create-form animate-fade-up">
                <div className="form-field">
                  <label>Chain Name</label>
                  <input className="form-input" placeholder="My agent chain" value={newName} onChange={e => setNewName(e.target.value)} />
                </div>
                <div className="form-field">
                  <label>Description</label>
                  <input className="form-input" placeholder="What this chain does" value={newDesc} onChange={e => setNewDesc(e.target.value)} />
                </div>
                <div className="form-field">
                  <label>Mode</label>
                  <select className="form-input" value={newMode} onChange={e => setNewMode(e.target.value as ChainMode)}>
                    <option value="sequential">Sequential (A → B → C)</option>
                    <option value="fan_out">Fan-out / Fan-in (parallel + merge)</option>
                    <option value="fallback">Fallback (try until success)</option>
                    <option value="specialist">Specialist Router (keyword match)</option>
                  </select>
                </div>

                <div className="orch-nodes-header">
                  <label>Agent Nodes</label>
                  <button className="link-btn" onClick={addNode}>+ Add node</button>
                </div>

                {newNodes.map((node, idx) => (
                  <div key={node.id || idx} className="orch-node-row">
                    <span className="orch-node-order">{idx + 1}</span>
                    <select
                      className="form-input"
                      value={node.agent_id || ''}
                      onChange={e => updateNode(idx, 'agent_id', e.target.value)}
                      style={{ flex: 1 }}
                    >
                      <option value="">Select agent...</option>
                      {enabledAgents.map(a => (
                        <option key={a.id} value={a.id}>{a.name} ({a.model})</option>
                      ))}
                    </select>
                    <input
                      className="form-input"
                      placeholder="Role hint"
                      value={node.role_hint || ''}
                      onChange={e => updateNode(idx, 'role_hint', e.target.value)}
                      style={{ width: 100 }}
                    />
                    <button className="link-btn danger" onClick={() => removeNode(idx)}>×</button>
                  </div>
                ))}

                <div className="sched-form-actions">
                  <button className="panel-btn" onClick={() => { setCreating(false); resetForm(); }}>Cancel</button>
                  <button className="panel-btn accent" onClick={handleCreate} disabled={!newName || newNodes.length === 0}>
                    Create Chain
                  </button>
                </div>
              </div>
            )}

            {chains.length === 0 && !creating && (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                    <circle cx="5" cy="12" r="3"/><circle cx="19" cy="6" r="3"/><circle cx="19" cy="18" r="3"/>
                    <path d="M8 12h4l3-6M12 12l3 6"/>
                  </svg>
                </div>
                <p>No agent chains yet</p>
                <p className="panel-empty-sub">Create a chain to orchestrate multiple agents on complex tasks.</p>
              </div>
            )}

            {chains.map(c => (
              <div key={c.id} className="orch-chain-card">
                <div className="orch-chain-top">
                  <div className="orch-chain-info">
                    <span className="orch-chain-name">{c.name}</span>
                    <span className="orch-chain-mode">
                      <span className="orch-mode-icon">{MODE_ICONS[c.mode]}</span>
                      {MODE_LABELS[c.mode]} · {c.nodes.length} agents
                    </span>
                  </div>
                  <div className="orch-chain-actions">
                    <button className="panel-btn" onClick={() => setRunInput({ chainId: c.id, text: '' })}>
                      Run
                    </button>
                    <button className="link-btn danger" onClick={() => handleDelete(c.id)} title="Delete">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/>
                      </svg>
                    </button>
                  </div>
                </div>
                {c.description && <p className="orch-chain-desc">{c.description}</p>}
                <div className="orch-chain-nodes">
                  {c.nodes.map((n, i) => (
                    <span key={n.id} className="orch-node-pill">
                      {n.agent_name || n.role_hint}
                      {i < c.nodes.length - 1 && <span className="orch-arrow">→</span>}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </>
        )}

        {/* ── Templates Tab ── */}
        {tab === 'templates' && (
          <div className="orch-templates-grid">
            {templates.map(tpl => (
              <div key={tpl.id} className="orch-tpl-card" onClick={() => handleCreateFromTemplate(tpl)}>
                <span className="orch-mode-badge">{MODE_ICONS[tpl.mode]} {MODE_LABELS[tpl.mode]}</span>
                <span className="orch-tpl-name">{tpl.name}</span>
                <span className="orch-tpl-desc">{tpl.description}</span>
                <div className="orch-tpl-hints">
                  {tpl.node_hints.map((h, i) => (
                    <span key={i} className="orch-hint-chip">{h.role_hint}</span>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* ── History Tab ── */}
        {tab === 'history' && (
          <>
            {history.length === 0 ? (
              <div className="panel-empty">
                <p>No run history yet</p>
                <p className="panel-empty-sub">Chain executions will appear here.</p>
              </div>
            ) : (
              history.map(r => (
                <div key={r.started_at} className="orch-history-card">
                  <div className="orch-history-top">
                    <span className="orch-history-name">{r.chain_name}</span>
                    <span className={`sched-run-badge ${r.status}`}>{r.status}</span>
                  </div>
                  <span className="orch-history-time">
                    {formatTime(r.started_at)} · {r.total_tokens} tokens · {r.node_results.length} agents
                  </span>
                  <div className="orch-history-timeline">
                    {r.node_results.map(nr => (
                      <div key={nr.node_id} className={`orch-timeline-step ${nr.status}`}>
                        <span className="orch-timeline-name">{nr.agent_name}</span>
                        <span className="orch-timeline-dur">{nr.duration_ms}ms</span>
                      </div>
                    ))}
                  </div>
                </div>
              ))
            )}
          </>
        )}
      </div>
    </div>
  );
}
