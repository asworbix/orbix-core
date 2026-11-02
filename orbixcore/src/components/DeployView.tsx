import { useState, useEffect, useCallback } from 'react';
import type { Agent, Workflow } from '../types';
import { listWorkflows } from '../hooks/api';
import './DeployView.css';

interface Props {
  agents: Agent[];
  onOpenPanel: (panel: string, workflowId?: string) => void;
}

interface AgentWithMeta extends Agent {
  workflows: Workflow[];
  status: 'running' | 'stopped' | 'error';
}

export default function DeployView({ agents: agentsProp, onOpenPanel }: Props) {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [search, setSearch] = useState('');
  const [expanded, setExpanded] = useState<string | null>(null);

  const agents: AgentWithMeta[] = agentsProp.map(a => ({
    ...a,
    workflows: [],
    status: a.enabled ? 'running' as const : 'stopped' as const,
  }));

  useEffect(() => {
    listWorkflows().then(setWorkflows).catch(() => {});
  }, []);

  // Assign workflows to agents - first by explicit workflow_ids, then by keyword fallback
  const agentsWithWorkflows = agents.map(a => {
    // Explicit links via workflow_ids take priority
    const linkedIds = new Set(a.workflow_ids || []);
    let matched = workflows.filter(w => linkedIds.has(w.id));

    // Keyword fallback if no explicit links
    if (matched.length === 0) {
      const agentName = a.name.toLowerCase();
      matched = workflows.filter(w => {
        const wText = `${w.name} ${w.description}`.toLowerCase();
        if (agentName.includes('crm')) return wText.includes('crm') || wText.includes('lead') || wText.includes('pipeline') || wText.includes('contact') || wText.includes('deal');
        if (agentName.includes('finance')) return wText.includes('financ') || wText.includes('invoice') || wText.includes('expense') || wText.includes('revenue');
        if (agentName.includes('erp')) return wText.includes('erp') || wText.includes('inventory') || wText.includes('stock') || wText.includes('purchase order') || wText.includes('supplier');
        if (agentName.includes('ppm')) return wText.includes('project') || wText.includes('portfolio') || wText.includes('milestone') || wText.includes('task');
        if (agentName.includes('support')) return wText.includes('support') || wText.includes('ticket') || wText.includes('knowledge base') || wText.includes('sla') || wText.includes('triage');
        if (a.is_default) return wText.includes('health') || wText.includes('dashboard') || wText.includes('sync') || wText.includes('end-of-day');
        return false;
      });
    }
    return { ...a, workflows: matched };
  });

  const filtered = agentsWithWorkflows.filter(a =>
    a.name.toLowerCase().includes(search.toLowerCase()) ||
    a.model.toLowerCase().includes(search.toLowerCase()) ||
    a.provider.toLowerCase().includes(search.toLowerCase())
  );

  const toggleExpand = useCallback((id: string) => {
    setExpanded(prev => prev === id ? null : id);
  }, []);

  const statusColors: Record<string, string> = {
    running: '#34d399',
    stopped: '#4e5a6e',
    error: '#f87171',
  };

  return (
    <div className="deploy-view">
      <div className="deploy-header">
        <div className="deploy-title">
          <h2>Agent Deployment</h2>
          <span className="deploy-count">{agents.length} agents</span>
        </div>
        <div className="deploy-actions">
          <button className="deploy-add-btn" onClick={() => onOpenPanel('agents')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            Add Agent
          </button>
        </div>
      </div>

      <div className="deploy-search">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
        </svg>
        <input
          placeholder="Search agents..."
          value={search}
          onChange={e => setSearch(e.target.value)}
        />
      </div>

      <div className="deploy-agent-list">
        {filtered.length === 0 ? (
          <div className="deploy-empty">
            <p>No agents configured</p>
            <p className="deploy-empty-sub">Add an agent to start deploying workflows and automations.</p>
            <button className="deploy-add-btn" onClick={() => onOpenPanel('agents')}>
              Add your first agent
            </button>
          </div>
        ) : (
          filtered.map(agent => (
            <div
              key={agent.id}
              className={`deploy-agent-card ${expanded === agent.id ? 'expanded' : ''}`}
            >
              <div className="deploy-agent-header" onClick={() => toggleExpand(agent.id)}>
                <div className="deploy-agent-status">
                  <div className="deploy-status-dot" style={{ background: statusColors[agent.status] }} />
                  <div className="deploy-agent-info">
                    <span className="deploy-agent-name">{agent.name}</span>
                    <span className="deploy-agent-meta">
                      {agent.provider} / {agent.model}
                      {agent.is_default && <span className="deploy-default-badge">default</span>}
                    </span>
                  </div>
                </div>
                <div className="deploy-agent-controls">
                  <span className={`deploy-agent-status-label ${agent.status}`}>
                    {agent.status === 'running' ? 'Active' : agent.status === 'error' ? 'Error' : 'Inactive'}
                  </span>
                  <svg
                    className={`deploy-chevron ${expanded === agent.id ? 'open' : ''}`}
                    width="14" height="14" viewBox="0 0 24 24"
                    fill="none" stroke="currentColor" strokeWidth="2"
                  >
                    <polyline points="6 9 12 15 18 9"/>
                  </svg>
                </div>
              </div>

              {expanded === agent.id && (
                <div className="deploy-agent-body">
                  {/* Workflows */}
                  <div className="deploy-section">
                    <div className="deploy-section-header">
                      <span>Workflows</span>
                      <button className="deploy-section-add" onClick={() => onOpenPanel('teach')}>+ Add</button>
                    </div>
                    {agent.workflows.length === 0 ? (
                      <div className="deploy-section-empty">No workflows attached</div>
                    ) : (
                      agent.workflows.map(w => {
                        const trigger = typeof w.trigger === 'string' ? w.trigger
                          : 'command' in w.trigger ? w.trigger.command
                          : 'schedule' in w.trigger ? 'cron' : 'manual';
                        const triggerLabel = typeof w.trigger === 'string' ? 'manual'
                          : 'command' in w.trigger ? 'command'
                          : 'schedule';
                        return (
                          <div key={w.id} className="deploy-item deploy-item-clickable" onClick={() => onOpenPanel('teach', w.id)}>
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/>
                              <rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>
                            </svg>
                            <div className="deploy-item-info">
                              <span>{w.name}</span>
                              <span className="deploy-item-desc">{w.description}</span>
                            </div>
                            <span className="deploy-item-badge">{w.steps.length} steps</span>
                            <span className={`deploy-item-trigger ${triggerLabel}`}>{trigger}</span>
                          </div>
                        );
                      })
                    )}
                  </div>

                  {/* Quick Actions */}
                  <div className="deploy-section">
                    <div className="deploy-section-header">
                      <span>Automation</span>
                    </div>
                    <div className="deploy-quick-links">
                      <button onClick={() => onOpenPanel('scheduler')}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
                        </svg>
                        Schedules
                      </button>
                      <button onClick={() => onOpenPanel('triggers')}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M22 12h-4l-3 9L9 3l-3 9H2"/>
                        </svg>
                        Triggers
                      </button>
                      <button onClick={() => onOpenPanel('orchestrator')}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <circle cx="5" cy="12" r="3"/><circle cx="19" cy="6" r="3"/><circle cx="19" cy="18" r="3"/>
                          <path d="M8 12h4l3-6M12 12l3 6"/>
                        </svg>
                        Chains
                      </button>
                      <button onClick={() => onOpenPanel('automation')}>
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M13 10V3L4 14h7v7l9-11h-7z"/>
                        </svg>
                        Auto-Heal
                      </button>
                    </div>
                  </div>

                  {/* Error Display */}
                  {agent.status === 'error' && (
                    <div className="deploy-error-section">
                      <span>Connection failed. Check agent configuration.</span>
                      <button className="deploy-fix-btn" onClick={() => onOpenPanel('agents')}>
                        Fix Configuration
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
