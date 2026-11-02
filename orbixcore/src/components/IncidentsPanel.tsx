import { useState, useEffect } from 'react';
import type { Incident, IncidentSeverity, IncidentStatus, IncidentMetrics } from '../types';
import {
  listActiveIncidents, listResolvedIncidents, createIncident,
  updateIncidentStatus, resolveIncident, closeIncident, deleteIncident,
  getIncidentMetrics,
} from '../hooks/api';
import './IncidentsPanel.css';

interface Props { onClose: () => void }

type Tab = 'active' | 'history' | 'metrics';

const SEVERITY_COLORS: Record<IncidentSeverity, string> = {
  Critical: '#ef4444', High: '#f97316', Medium: '#fbbf24', Low: '#6366f1',
};

const STATUS_ORDER: IncidentStatus[] = ['Open', 'Investigating', 'Mitigating', 'Resolved', 'Closed'];

function timeAgo(ts: string): string {
  const diff = Date.now() - new Date(ts).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

export default function IncidentsPanel({ onClose }: Props) {
  const [tab, setTab] = useState<Tab>('active');
  const [active, setActive] = useState<Incident[]>([]);
  const [history, setHistory] = useState<Incident[]>([]);
  const [metrics, setMetrics] = useState<IncidentMetrics | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [newDesc, setNewDesc] = useState('');
  const [newSeverity, setNewSeverity] = useState<IncidentSeverity>('Medium');
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [updateMsg, setUpdateMsg] = useState('');

  const loadActive = () => listActiveIncidents().then(setActive).catch(() => {});
  const loadHistory = () => listResolvedIncidents(50).then(setHistory).catch(() => {});

  useEffect(() => { loadActive(); }, []);

  useEffect(() => {
    if (tab === 'history') loadHistory();
    if (tab === 'metrics') getIncidentMetrics().then(setMetrics).catch(() => {});
  }, [tab]);

  const handleCreate = async () => {
    if (!newTitle.trim()) return;
    await createIncident({
      id: '', title: newTitle.trim(), description: newDesc.trim(),
      severity: newSeverity, status: 'Open',
      assigned_device_id: null, assigned_group_id: null,
      source: 'manual', source_id: null,
      affected_device_ids: [], tags: [], updates: [],
      created_at: '', updated_at: '', resolved_at: null, resolution_notes: null,
    });
    setNewTitle(''); setNewDesc(''); setShowCreate(false);
    loadActive();
  };

  const handleStatusChange = async (id: string, status: IncidentStatus) => {
    if (status === 'Resolved') {
      await resolveIncident(id, updateMsg || 'Resolved');
    } else if (status === 'Closed') {
      await closeIncident(id);
    } else {
      await updateIncidentStatus(id, status, updateMsg || `Status changed to ${status}`, 'user');
    }
    setUpdateMsg('');
    setExpandedId(null);
    loadActive();
    if (tab === 'history') loadHistory();
  };

  const handleDelete = async (id: string) => {
    await deleteIncident(id);
    loadActive();
    loadHistory();
  };

  const renderIncident = (inc: Incident, showActions = true) => (
    <div key={inc.id} className="incident-card">
      <div className="incident-severity-bar" style={{ background: SEVERITY_COLORS[inc.severity] }} />
      <div className="incident-body">
        <div className="incident-header" onClick={() => setExpandedId(expandedId === inc.id ? null : inc.id)}>
          <div className="incident-title-row">
            <span className="incident-title">{inc.title}</span>
            <span className="incident-time">{timeAgo(inc.created_at)}</span>
          </div>
          <div className="incident-meta">
            <span className={`incident-severity-badge ${inc.severity.toLowerCase()}`}>{inc.severity}</span>
            <span className={`incident-status-badge ${inc.status.toLowerCase()}`}>{inc.status}</span>
            {inc.affected_device_ids.length > 0 && (
              <span className="incident-affected">{inc.affected_device_ids.length} affected</span>
            )}
            {inc.source !== 'manual' && (
              <span className="incident-source">{inc.source}</span>
            )}
          </div>
        </div>

        {expandedId === inc.id && (
          <div className="incident-details">
            <div className="incident-desc">{inc.description}</div>

            {inc.tags.length > 0 && (
              <div className="incident-tags">
                {inc.tags.map(t => <span key={t} className="incident-tag">{t}</span>)}
              </div>
            )}

            {inc.updates.length > 0 && (
              <div className="incident-updates">
                <div className="incident-updates-title">Updates</div>
                {inc.updates.map(u => (
                  <div key={u.id} className="incident-update-item">
                    <span className={`incident-status-badge ${u.status.toLowerCase()}`}>{u.status}</span>
                    <span className="incident-update-msg">{u.message}</span>
                    <span className="incident-update-time">{timeAgo(u.timestamp)}</span>
                  </div>
                ))}
              </div>
            )}

            {inc.resolution_notes && (
              <div className="incident-resolution">
                <strong>Resolution:</strong> {inc.resolution_notes}
              </div>
            )}

            {showActions && !['Resolved', 'Closed'].includes(inc.status) && (
              <div className="incident-actions">
                <input
                  placeholder="Update message..."
                  value={updateMsg}
                  onChange={e => setUpdateMsg(e.target.value)}
                  className="incident-update-input"
                />
                <div className="incident-action-btns">
                  {STATUS_ORDER.filter(s => s !== inc.status && STATUS_ORDER.indexOf(s) > STATUS_ORDER.indexOf(inc.status)).map(s => (
                    <button key={s} className={`incident-action-btn ${s.toLowerCase()}`}
                      onClick={() => handleStatusChange(inc.id, s)}>{s}</button>
                  ))}
                </div>
              </div>
            )}

            <button className="incident-delete-btn" onClick={() => handleDelete(inc.id)}>Delete</button>
          </div>
        )}
      </div>
    </div>
  );

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
            <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
          </svg>
          Incidents
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="incidents-tabs">
        <button className={tab === 'active' ? 'active' : ''} onClick={() => setTab('active')}>
          Active {active.length > 0 && <span className="incidents-tab-badge">{active.length}</span>}
        </button>
        <button className={tab === 'history' ? 'active' : ''} onClick={() => setTab('history')}>History</button>
        <button className={tab === 'metrics' ? 'active' : ''} onClick={() => setTab('metrics')}>Metrics</button>
      </div>

      <div className="panel-body">
        {tab === 'active' && (
          <>
            <div className="incidents-toolbar">
              <button className="incidents-create-btn" onClick={() => setShowCreate(!showCreate)}>
                {showCreate ? 'Cancel' : '+ New Incident'}
              </button>
            </div>

            {showCreate && (
              <div className="incidents-create-form">
                <input placeholder="Incident title" value={newTitle} onChange={e => setNewTitle(e.target.value)} />
                <textarea placeholder="Description..." value={newDesc} onChange={e => setNewDesc(e.target.value)} rows={3} />
                <div className="incidents-severity-select">
                  <span className="incidents-label">Severity</span>
                  {(['Critical', 'High', 'Medium', 'Low'] as IncidentSeverity[]).map(s => (
                    <button key={s} className={`incidents-sev-btn ${newSeverity === s ? 'active' : ''}`}
                      style={{ borderColor: newSeverity === s ? SEVERITY_COLORS[s] : undefined,
                               color: newSeverity === s ? SEVERITY_COLORS[s] : undefined }}
                      onClick={() => setNewSeverity(s)}>{s}</button>
                  ))}
                </div>
                <button className="incidents-submit-btn" onClick={handleCreate}>Create Incident</button>
              </div>
            )}

            {active.length === 0 ? (
              <div className="panel-empty">
                <p>No active incidents</p>
                <p className="panel-empty-sub">All clear! Create an incident to start tracking issues.</p>
              </div>
            ) : (
              active.map(inc => renderIncident(inc))
            )}
          </>
        )}

        {tab === 'history' && (
          <>
            {history.length === 0 ? (
              <div className="panel-empty">
                <p>No resolved incidents</p>
                <p className="panel-empty-sub">Resolved and closed incidents will appear here.</p>
              </div>
            ) : (
              history.map(inc => renderIncident(inc, false))
            )}
          </>
        )}

        {tab === 'metrics' && metrics && (
          <>
            <div className="incidents-metrics-grid">
              <div className="incidents-metric-card">
                <div className="incidents-metric-value">{metrics.total_incidents}</div>
                <div className="incidents-metric-label">Total</div>
              </div>
              <div className="incidents-metric-card">
                <div className="incidents-metric-value" style={{ color: '#ef4444' }}>{metrics.open_incidents}</div>
                <div className="incidents-metric-label">Open</div>
              </div>
              <div className="incidents-metric-card">
                <div className="incidents-metric-value" style={{ color: '#fbbf24' }}>{metrics.investigating}</div>
                <div className="incidents-metric-label">Investigating</div>
              </div>
              <div className="incidents-metric-card">
                <div className="incidents-metric-value" style={{ color: '#f97316' }}>{metrics.mitigating}</div>
                <div className="incidents-metric-label">Mitigating</div>
              </div>
              <div className="incidents-metric-card">
                <div className="incidents-metric-value" style={{ color: '#10b981' }}>{metrics.resolved_today}</div>
                <div className="incidents-metric-label">Resolved Today</div>
              </div>
              <div className="incidents-metric-card">
                <div className="incidents-metric-value">{metrics.mttr_mins.toFixed(0)}m</div>
                <div className="incidents-metric-label">MTTR</div>
              </div>
            </div>

            <div className="incidents-severity-breakdown">
              <div className="incidents-breakdown-title">By Severity</div>
              {metrics.by_severity.map(([sev, count]) => (
                <div key={sev} className="incidents-breakdown-row">
                  <span className="incidents-breakdown-label">{sev}</span>
                  <div className="incidents-breakdown-bar">
                    <div className="incidents-breakdown-fill"
                      style={{
                        width: `${metrics.total_incidents > 0 ? (count / metrics.total_incidents) * 100 : 0}%`,
                        background: SEVERITY_COLORS[sev as IncidentSeverity] || '#6366f1',
                      }} />
                  </div>
                  <span className="incidents-breakdown-count">{count}</span>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
