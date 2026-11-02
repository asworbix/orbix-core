import { useState, useEffect } from 'react';
import type { ScheduleEntry, SchedulePreset, ScheduleRunLog, Workflow } from '../types';
import {
  listSchedules, createSchedule, deleteSchedule, toggleSchedule,
  getScheduleHistory, previewSchedule, listWorkflows,
} from '../hooks/api';
import './SchedulerPanel.css';

const PRESETS: { value: SchedulePreset; label: string }[] = [
  { value: 'every_5_minutes', label: 'Every 5 minutes' },
  { value: 'every_15_minutes', label: 'Every 15 minutes' },
  { value: 'every_hour', label: 'Every hour' },
  { value: 'daily', label: 'Daily at 9 AM' },
  { value: 'weekday_mornings', label: 'Weekdays at 9 AM' },
  { value: 'weekly', label: 'Weekly (Mon 9 AM)' },
  { value: 'monthly', label: 'Monthly (1st at 9 AM)' },
  { value: 'custom', label: 'Custom cron...' },
];

interface Props { onClose: () => void }

export default function SchedulerPanel({ onClose }: Props) {
  const [schedules, setSchedules] = useState<ScheduleEntry[]>([]);
  const [history, setHistory] = useState<ScheduleRunLog[]>([]);
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [tab, setTab] = useState<'schedules' | 'history'>('schedules');
  const [creating, setCreating] = useState(false);

  // Create form
  const [selWorkflow, setSelWorkflow] = useState('');
  const [selPreset, setSelPreset] = useState<SchedulePreset>('daily');
  const [customCron, setCustomCron] = useState('');
  const [preview, setPreview] = useState<string[]>([]);

  useEffect(() => {
    listSchedules().then(setSchedules).catch(() => {});
    getScheduleHistory().then(setHistory).catch(() => {});
    listWorkflows().then(setWorkflows).catch(() => {});
  }, []);

  const handleCreate = async () => {
    if (!selWorkflow) return;
    try {
      const entry = await createSchedule(
        selWorkflow,
        selPreset,
        selPreset === 'custom' ? customCron : undefined,
      );
      setSchedules(prev => [entry, ...prev]);
      setCreating(false);
      setSelWorkflow('');
      setCustomCron('');
    } catch (err) {
      console.error(err);
    }
  };

  const handleToggle = async (id: string) => {
    try {
      const updated = await toggleSchedule(id);
      setSchedules(prev => prev.map(s => s.id === id ? updated : s));
    } catch {}
  };

  const handleDelete = async (id: string) => {
    await deleteSchedule(id).catch(() => {});
    setSchedules(prev => prev.filter(s => s.id !== id));
  };

  const handlePreview = async () => {
    const cronMap: Record<string, string> = {
      'every_minute': '* * * * *',
      'every_5_minutes': '*/5 * * * *',
      'every_15_minutes': '*/15 * * * *',
      'every_hour': '0 * * * *',
      'daily': '0 9 * * *',
      'weekday_mornings': '0 9 * * 1-5',
      'weekly': '0 9 * * 1',
      'monthly': '0 9 1 * *',
    };
    const cron = selPreset === 'custom' ? customCron : cronMap[selPreset] || '0 9 * * *';
    if (!cron) return;
    try {
      const times = await previewSchedule(cron, 5);
      setPreview(times);
    } catch { setPreview([]); }
  };

  const workflowName = (id: string) => workflows.find(w => w.id === id)?.name || 'Unknown';

  const formatTime = (iso: string | null) => {
    if (!iso) return '-';
    const d = new Date(iso);
    return d.toLocaleString(undefined, {
      month: 'short', day: 'numeric',
      hour: '2-digit', minute: '2-digit',
    });
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="12" cy="12" r="10"/>
            <polyline points="12 6 12 12 16 14"/>
          </svg>
          Scheduler
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      <div className="sched-tabs">
        <button className={tab === 'schedules' ? 'active' : ''} onClick={() => setTab('schedules')}>
          Schedules
          {schedules.length > 0 && <span className="sched-tab-count">{schedules.length}</span>}
        </button>
        <button className={tab === 'history' ? 'active' : ''} onClick={() => setTab('history')}>
          Run History
          {history.length > 0 && <span className="sched-tab-count">{history.length}</span>}
        </button>
      </div>

      <div className="panel-body">
        {tab === 'schedules' && (
          <>
            {!creating && (
              <button className="panel-btn accent full" onClick={() => setCreating(true)} style={{ marginBottom: 12 }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                </svg>
                New Schedule
              </button>
            )}

            {creating && (
              <div className="sched-create-form animate-fade-up">
                <div className="form-field">
                  <label>Workflow</label>
                  <select className="form-input" value={selWorkflow} onChange={e => setSelWorkflow(e.target.value)}>
                    <option value="">Select a workflow...</option>
                    {workflows.map(w => (
                      <option key={w.id} value={w.id}>{w.name}</option>
                    ))}
                  </select>
                </div>

                <div className="form-field">
                  <label>Schedule</label>
                  <select className="form-input" value={selPreset} onChange={e => setSelPreset(e.target.value as SchedulePreset)}>
                    {PRESETS.map(p => (
                      <option key={p.value} value={p.value}>{p.label}</option>
                    ))}
                  </select>
                </div>

                {selPreset === 'custom' && (
                  <div className="form-field">
                    <label>Cron Expression <span className="label-sub">min hour dom month dow</span></label>
                    <input
                      className="form-input"
                      placeholder="0 9 * * 1-5"
                      value={customCron}
                      onChange={e => setCustomCron(e.target.value)}
                    />
                  </div>
                )}

                <button className="link-btn" onClick={handlePreview} style={{ alignSelf: 'flex-start' }}>
                  Preview next runs →
                </button>

                {preview.length > 0 && (
                  <div className="sched-preview">
                    {preview.map((t, i) => (
                      <div key={i} className="sched-preview-item">{formatTime(t)}</div>
                    ))}
                  </div>
                )}

                <div className="sched-form-actions">
                  <button className="panel-btn" onClick={() => { setCreating(false); setPreview([]); }}>Cancel</button>
                  <button
                    className="panel-btn accent"
                    onClick={handleCreate}
                    disabled={!selWorkflow || (selPreset === 'custom' && !customCron)}
                  >
                    Create Schedule
                  </button>
                </div>
              </div>
            )}

            {schedules.length === 0 && !creating && (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                    <circle cx="12" cy="12" r="10"/>
                    <polyline points="12 6 12 12 16 14"/>
                  </svg>
                </div>
                <p>No schedules yet</p>
                <p className="panel-empty-sub">Create a schedule to run workflows automatically on a timer.</p>
              </div>
            )}

            {schedules.map(s => (
              <div key={s.id} className={`sched-card ${!s.enabled ? 'disabled' : ''}`}>
                <div className="sched-card-top">
                  <div className="sched-card-info">
                    <span className="sched-card-name">{workflowName(s.workflow_id)}</span>
                    <span className="sched-card-cron">{s.cron_expr}</span>
                  </div>
                  <div className="sched-card-actions">
                    <button
                      className={`toggle-switch ${s.enabled ? 'on' : ''}`} role="switch" aria-checked={s.enabled}
                      onClick={() => handleToggle(s.id)}
                      title={s.enabled ? 'Pause' : 'Resume'}
                    >
                      <div className="toggle-thumb" />
                    </button>
                    <button className="link-btn danger" onClick={() => handleDelete(s.id)} title="Delete">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/>
                      </svg>
                    </button>
                  </div>
                </div>
                <div className="sched-card-meta">
                  <span>Next: {formatTime(s.next_run)}</span>
                  <span>Last: {formatTime(s.last_run)}</span>
                  <span>Runs: {s.run_count}{s.fail_count > 0 ? ` (${s.fail_count} failed)` : ''}</span>
                </div>
              </div>
            ))}
          </>
        )}

        {tab === 'history' && (
          <>
            {history.length === 0 ? (
              <div className="panel-empty">
                <p>No run history yet</p>
                <p className="panel-empty-sub">Schedule runs will appear here.</p>
              </div>
            ) : (
              history.map(r => (
                <div key={r.id} className="sched-run-item">
                  <div className={`sched-run-dot ${r.result}`} />
                  <div className="sched-run-info">
                    <span className="sched-run-name">{r.workflow_name}</span>
                    <span className="sched-run-time">{formatTime(r.started_at)} · {r.duration_ms}ms</span>
                  </div>
                  <span className={`sched-run-badge ${r.result}`}>{r.result}</span>
                </div>
              ))
            )}
          </>
        )}
      </div>
    </div>
  );
}
