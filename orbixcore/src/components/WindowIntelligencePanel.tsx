import { useState, useEffect, useCallback } from 'react';
import type { WindowProfile, WindowLibrary, WiRunningApp, AccessibilityStatus, WindowTwinEvent, WatchedWindow } from '../types';
import {
  wiCheckAccessibility, wiRequestAccessibility, wiListRunningApps, wiProfileWindow,
  wiRefreshProfile, wiGetLibrary, wiRemoveProfile,
  wiAddWatched, wiRemoveWatched, wiToggleWatched, wiGetEvents,
} from '../hooks/api';
import './WindowIntelligencePanel.css';

interface Props { onClose: () => void; }

type Tab = 'library' | 'profile' | 'events';

export default function WindowIntelligencePanel({ onClose }: Props) {
  const [tab, setTab] = useState<Tab>('library');
  const [axStatus, setAxStatus] = useState<AccessibilityStatus | null>(null);
  const [library, setLibrary] = useState<WindowLibrary | null>(null);
  const [apps, setApps] = useState<WiRunningApp[]>([]);
  const [showPicker, setShowPicker] = useState(false);
  const [profiling, setProfiling] = useState<number | null>(null);
  const [selected, setSelected] = useState<WindowProfile | null>(null);
  const [events, setEvents] = useState<WindowTwinEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [status, lib] = await Promise.all([
        wiCheckAccessibility(),
        wiGetLibrary(),
      ]);
      setAxStatus(status);
      setLibrary(lib);
    } catch (e: unknown) {
      setError(String(e));
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const loadApps = async () => {
    setShowPicker(true);
    try {
      const list = await wiListRunningApps();
      setApps(list);
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const profileApp = async (app: WiRunningApp) => {
    setProfiling(app.pid);
    setError(null);
    try {
      const profile = await wiProfileWindow(app.pid, app.name);
      setSelected(profile);
      setShowPicker(false);
      setTab('profile');
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setProfiling(null);
    }
  };

  const selectWindow = (w: WindowProfile) => {
    setSelected(w);
    setTab('profile');
  };

  const refreshProfile = async () => {
    if (!selected) return;
    try {
      const updated = await wiRefreshProfile(selected.window_id);
      setSelected(updated);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const removeProfile = async (windowId: string) => {
    try {
      await wiRemoveProfile(windowId);
      if (selected?.window_id === windowId) {
        setSelected(null);
        setTab('library');
      }
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const loadEvents = async () => {
    setTab('events');
    try {
      const ev = await wiGetEvents(selected?.window_id, 100);
      setEvents(ev);
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const addWatched = async (profile: WindowProfile) => {
    try {
      const w: WatchedWindow = {
        id: '',
        application: profile.application,
        url_pattern: profile.url,
        file_pattern: null,
        label: profile.title,
        update_frequency: 'on_change',
        active: true,
      };
      await wiAddWatched(w);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const removeWatched = async (id: string) => {
    try {
      await wiRemoveWatched(id);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const toggleWatched = async (id: string) => {
    try {
      await wiToggleWatched(id);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const fmtTime = (ts: string) => {
    try {
      const d = new Date(ts);
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    } catch { return ts; }
  };

  return (
    <div className="wi-panel">
      <div className="wi-header">
        <h2>Window Intelligence</h2>
        <button className="wi-close-btn" onClick={onClose}>&times;</button>
      </div>

      <div className="wi-tabs">
        <button className={`wi-tab ${tab === 'library' ? 'active' : ''}`} onClick={() => setTab('library')}>Library</button>
        <button className={`wi-tab ${tab === 'profile' ? 'active' : ''}`} onClick={() => selected && setTab('profile')} disabled={!selected}>Profile</button>
        <button className={`wi-tab ${tab === 'events' ? 'active' : ''}`} onClick={loadEvents}>Events</button>
      </div>

      <div className="wi-body">
        {error && (
          <div className="wi-status-banner error">{error}</div>
        )}

        {axStatus && !axStatus.trusted && (
          <div className="wi-status-banner warn">
            <div style={{ flex: 1 }}>{axStatus.message}</div>
            <div style={{ display: 'flex', gap: '0.4rem', flexShrink: 0 }}>
              <button className="wi-btn primary" style={{ fontSize: '0.75rem', padding: '0.3rem 0.6rem' }} onClick={async () => {
                const s = await wiRequestAccessibility();
                setAxStatus(s);
              }}>Request Access</button>
              <button className="wi-btn" style={{ fontSize: '0.75rem', padding: '0.3rem 0.6rem' }} onClick={refresh}>Check Again</button>
            </div>
          </div>
        )}

        {/* ─── Library Tab ─── */}
        {tab === 'library' && (
          <>
            {library && (
              <div className="wi-stats">
                <div className="wi-stat">
                  <div className="wi-stat-value">{library.active_windows.length}</div>
                  <div className="wi-stat-label">Windows</div>
                </div>
                <div className="wi-stat">
                  <div className="wi-stat-value">{library.total_fields_mapped}</div>
                  <div className="wi-stat-label">Fields Mapped</div>
                </div>
                <div className="wi-stat">
                  <div className="wi-stat-value">{library.total_actions_mapped}</div>
                  <div className="wi-stat-label">Actions Mapped</div>
                </div>
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.75rem' }}>
              <div className="wi-section-title" style={{ margin: 0 }}>Active Windows</div>
              <button className="wi-btn primary" onClick={loadApps}>+ Add Window</button>
            </div>

            {showPicker && (
              <div className="wi-app-picker">
                {apps.length === 0 && <div className="wi-empty">Loading applications...</div>}
                {apps.map(app => (
                  <div key={app.pid} className="wi-app-item" onClick={() => profileApp(app)}>
                    <span>{app.name}</span>
                    <span className="pid">
                      {profiling === app.pid ? 'Profiling...' : `PID ${app.pid}`}
                    </span>
                  </div>
                ))}
              </div>
            )}

            {library && library.active_windows.length > 0 ? (
              <div className="wi-window-list">
                {library.active_windows.map(w => (
                  <div
                    key={w.window_id}
                    className={`wi-window-card ${selected?.window_id === w.window_id ? 'selected' : ''}`}
                    onClick={() => selectWindow(w)}
                  >
                    <div className="wi-window-row">
                      <div>
                        <div className="wi-window-name">{w.title || w.application}</div>
                        <div className="wi-window-app">{w.application}</div>
                      </div>
                      <span className={`wi-badge ${w.status}`}>{w.status}</span>
                    </div>
                    <div className="wi-window-meta">
                      <span className="wi-badge fields">{w.field_count} fields</span>
                      <span className="wi-badge actions">{w.action_count} actions</span>
                    </div>
                  </div>
                ))}
              </div>
            ) : !showPicker ? (
              <div className="wi-empty">
                No windows profiled yet
                <p>Click "Add Window" to profile any running application</p>
              </div>
            ) : null}

            {library && library.watched.length > 0 && (
              <>
                <div className="wi-section-title" style={{ marginTop: '1.25rem' }}>Watched</div>
                <div className="wi-window-list">
                  {library.watched.map(w => (
                    <div key={w.id} className="wi-watched-card">
                      <div>
                        <div className="wi-watched-label">{w.label}</div>
                        <div className="wi-watched-pattern">
                          {w.url_pattern || w.file_pattern || w.application}
                        </div>
                      </div>
                      <div className="wi-watched-controls">
                        <button
                          className={`wi-btn ${w.active ? '' : 'primary'}`}
                          onClick={() => toggleWatched(w.id)}
                          style={{ fontSize: '0.72rem', padding: '0.3rem 0.6rem' }}
                        >
                          {w.active ? 'Pause' : 'Resume'}
                        </button>
                        <button
                          className="wi-btn danger"
                          onClick={() => removeWatched(w.id)}
                          style={{ fontSize: '0.72rem', padding: '0.3rem 0.6rem' }}
                        >
                          Remove
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              </>
            )}
          </>
        )}

        {/* ─── Profile Tab ─── */}
        {tab === 'profile' && selected && (
          <div className="wi-profile-detail">
            <div className="wi-window-row" style={{ marginBottom: '0.75rem' }}>
              <div>
                <div className="wi-window-name">{selected.title}</div>
                <div className="wi-window-app">{selected.application} (PID {selected.pid})</div>
              </div>
              <span className={`wi-badge ${selected.status}`}>{selected.status}</span>
            </div>

            <div className="wi-actions-row">
              <button className="wi-btn primary" onClick={refreshProfile}>Refresh</button>
              <button className="wi-btn" onClick={() => addWatched(selected)}>Watch</button>
              <button className="wi-btn danger" onClick={() => removeProfile(selected.window_id)}>Remove</button>
            </div>

            {/* Inputs */}
            {selected.profile.inputs.length > 0 && (
              <div className="wi-detail-section" style={{ marginTop: '1rem' }}>
                <div className="wi-section-title">Input Fields ({selected.profile.inputs.length})</div>
                <table className="wi-field-table">
                  <thead>
                    <tr>
                      <th>Label</th>
                      <th>Type</th>
                      <th>Value</th>
                      <th>ID</th>
                    </tr>
                  </thead>
                  <tbody>
                    {selected.profile.inputs.map(inp => (
                      <tr key={inp.id}>
                        <td>{inp.label || '(unlabeled)'}</td>
                        <td>{inp.input_type}</td>
                        <td style={{ maxWidth: 120, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {inp.current_value ?? '-'}
                        </td>
                        <td style={{ fontSize: '0.72rem', color: 'rgba(255,255,255,0.3)' }}>{inp.id}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {/* Data Elements */}
            {selected.profile.data_elements.length > 0 && (
              <div className="wi-detail-section">
                <div className="wi-section-title">Data Elements ({selected.profile.data_elements.length})</div>
                <table className="wi-field-table">
                  <thead>
                    <tr>
                      <th>Label</th>
                      <th>Type</th>
                      <th>Value</th>
                    </tr>
                  </thead>
                  <tbody>
                    {selected.profile.data_elements.map(el => (
                      <tr key={el.id}>
                        <td>{el.label || '(unlabeled)'}</td>
                        <td>{el.element_type}</td>
                        <td style={{ maxWidth: 150, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {el.current_value ?? '-'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {/* Actions */}
            {selected.profile.actions.length > 0 && (
              <div className="wi-detail-section">
                <div className="wi-section-title">Actions ({selected.profile.actions.length})</div>
                <table className="wi-field-table">
                  <thead>
                    <tr>
                      <th>Label</th>
                      <th>Type</th>
                      <th>Enabled</th>
                    </tr>
                  </thead>
                  <tbody>
                    {selected.profile.actions.map(act => (
                      <tr key={act.id}>
                        <td>{act.label}</td>
                        <td>{act.action_type}</td>
                        <td>{act.enabled ? 'Yes' : 'No'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {/* Navigation */}
            {selected.profile.navigation.sections.length > 0 && (
              <div className="wi-detail-section">
                <div className="wi-section-title">Navigation</div>
                <div style={{ display: 'flex', gap: '0.4rem', flexWrap: 'wrap' }}>
                  {selected.profile.navigation.sections.map((s, i) => (
                    <span key={i} className="wi-badge fields">{s}</span>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        {tab === 'profile' && !selected && (
          <div className="wi-empty">
            Select a window from the Library tab
          </div>
        )}

        {/* ─── Events Tab ─── */}
        {tab === 'events' && (
          <>
            <div className="wi-section-title">
              {selected ? `Events for ${selected.title}` : 'All Events'}
            </div>
            {events.length > 0 ? (
              <div className="wi-event-list">
                {events.map(ev => (
                  <div key={ev.id} className="wi-event">
                    <span className={`wi-event-dot ${ev.event_type}`} />
                    <span className="wi-event-text">
                      <strong>{ev.event_type.replace(/_/g, ' ')}</strong>
                      {ev.field_id && <> on <em>{ev.field_id}</em></>}
                      {ev.new_value && <> = {ev.new_value}</>}
                    </span>
                    <span className="wi-event-time">{fmtTime(ev.timestamp)}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="wi-empty">
                No events recorded yet
                <p>Events appear as agents interact with profiled windows</p>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
