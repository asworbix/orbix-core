import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Agent, Workflow, WorkflowStep, WorkflowRunState, ScheduleSpec, WhenInfo, Deliver, Peer } from '../types';
import {
  listWorkflows, deleteWorkflow, updateWorkflow, updateWorkflowSteps,
  runWorkflow, stopWorkflow, testWorkflowStep, draftWorkflow, reviseWorkflow,
  listWorkflowRuns, onWorkflowStatusChange, onRunLog, onDataChanged,
  getWorkflowWhen, setWorkflowSchedule, setWorkflowDeliver, listPeers,
  type WorkflowRunSummary,
} from '../hooks/api';
import TemplateGallery from './TemplateGallery';
import './BuildView.css';

/*
 * Build: workflows as a numbered list of plain sentences.
 *
 * You say what should happen; Bob drafts the steps; you adjust by telling
 * him what to change, or by dragging steps into another order. Tool names
 * and arguments stay under a Details toggle. Running lights the cards up
 * in order with plain progress, and a failure says what happened. The
 * old node canvas only ever reordered a list, so this is the honest shape
 * of a workflow, not a simplification of it.
 */

interface Props {
  agents: Agent[];
  onOpenPanel: (panel: string, workflowId?: string) => void;
  focusWorkflowId?: string | null;
}

const EXAMPLES = [
  'Every weekday morning, read my calendar and unread mail and write me a short brief',
  'Ask ChatGPT to summarise what is in Notes today and keep the summary in a new note',
  'Once a week, list the files on my Desktop older than 30 days and tell me what to archive',
];

/* ── Plain-language helpers ── */

type Cfg = Record<string, unknown>;

function stepSubject(step: WorkflowStep): string {
  const c = step.config as Cfg;
  const args = (c.tool_args ?? {}) as Cfg;
  if (step.step_type === 'prompt') return 'Bob';
  if (step.step_type === 'wait') return 'Clock';
  if (step.step_type === 'condition') return 'Check';
  if (step.step_type === 'loop') return 'Repeat';
  if (step.step_type === 'relay') return 'Private web';
  const tool = String(c.tool ?? '');
  if (tool === 'app_bridge' || tool.startsWith('peer_')) return String(args.app ?? 'App');
  if (tool === 'mcp_call') return String(args.server ?? 'MCP');
  if (tool === 'api_call') return args.connector ? String(args.connector) : 'Web API';
  if (tool === 'web_search' || tool === 'web_fetch') return 'Web';
  if (tool.startsWith('file_') || c.path) return 'Files';
  if (tool === 'shell_exec' || c.command) return 'Terminal';
  if (c.connector_id) return String(c.connector_id);
  if (c.destination) return 'Database';
  return 'Action';
}

function humanCron(c: string): string {
  const map: Record<string, string> = {
    '* * * * *': 'every minute',
    '*/5 * * * *': 'every 5 minutes',
    '*/15 * * * *': 'every 15 minutes',
    '0 * * * *': 'every hour',
    '0 9 * * *': 'every day at 09:00',
    '0 9 * * 1-5': 'every weekday at 09:00',
    '0 9 * * 1': 'every Monday at 09:00',
    '0 9 1 * *': 'on the 1st of each month at 09:00',
  };
  if (map[c]) return map[c];
  const p = c.trim().split(/\s+/);
  if (p.length === 5 && /^\d+$/.test(p[0]) && /^\d+$/.test(p[1])) {
    const at = `${p[1].padStart(2, '0')}:${p[0].padStart(2, '0')}`;
    const when = p[4] === '1-5' ? 'every weekday' : p[4] === '*' ? 'every day' : p[4] === '0,6' || p[4] === '6,0' ? 'at weekends' : `on days ${p[4]}`;
    return `${when} at ${at}`;
  }
  return `on a schedule (${c})`;
}

const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const pad = (n: number) => String(n).padStart(2, '0');

function specText(sp: ScheduleSpec): string {
  const at = `${pad(sp.hour)}:${pad(sp.minute)}`;
  switch (sp.kind) {
    case 'daily': return `every day at ${at}`;
    case 'weekdays': return `every weekday at ${at}`;
    case 'weekly': return `every ${WEEKDAYS[sp.weekday] ?? 'Monday'} at ${at}`;
    case 'monthly': return `on day ${sp.day} of each month at ${at}`;
    case 'hourly': return 'every hour';
    case 'minutes': return `every ${sp.every} minutes`;
  }
}

function deliverText(d: Deliver | undefined): string {
  if (!d || d === 'chat') return 'show it in chat';
  if (d === 'document') return 'save it as a document';
  if (d === 'panel') return 'put it in a panel in My UI';
  if (d === 'notify') return 'notify me and show it in chat';
  return `send it to ${d.app}`;
}

function whenText(wf: Workflow): string {
  const t = wf.trigger;
  if (t === 'manual') return 'Runs when you press Run';
  if (typeof t === 'object' && 'schedule' in t) return `Runs ${humanCron(t.schedule)}`;
  if (typeof t === 'object' && 'command' in t) return `Runs when you type /${t.command}`;
  return 'Runs when you press Run';
}

function relTime(iso: string): string {
  const d = new Date(iso).getTime();
  if (Number.isNaN(d)) return '';
  const m = Math.round((Date.now() - d) / 60000);
  if (m < 1) return 'just now';
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  return `${Math.round(h / 24)} d ago`;
}

/* Plain words for the errors the engine produces. */
function plainError(e: string | null | undefined): string {
  if (!e) return 'A step did not work.';
  return e
    .replace(/^\*+Failed:\*?\s*/i, '')
    .replace(/\[ERR\]\s*/g, '')
    .replace(/\*Connector error:\*\s*/g, '')
    .replace(/Step '([^']+)' failed after \d+ attempts:\s*/i, '"$1": ')
    .trim();
}

type StepStatus = 'idle' | 'running' | 'done' | 'failed';

/* ── Component ── */

export default function BuildView({ onOpenPanel, focusWorkflowId }: Props) {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [composing, setComposing] = useState(false);
  const [showTemplates, setShowTemplates] = useState(false);

  // Drafting from a sentence
  const [ask, setAsk] = useState('');
  const [drafting, setDrafting] = useState(false);
  const [draftError, setDraftError] = useState<string | null>(null);

  // Revising with Bob
  const [reviseOpen, setReviseOpen] = useState(false);
  const [reviseText, setReviseText] = useState('');
  const [revising, setRevising] = useState(false);
  const [reviseError, setReviseError] = useState<string | null>(null);
  const reviseRef = useRef<HTMLTextAreaElement>(null);

  // Adding a step
  const [addText, setAddText] = useState('');

  // Running
  const [runState, setRunState] = useState<WorkflowRunState | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [runs, setRuns] = useState<WorkflowRunSummary[]>([]);

  // Per-step UI
  const [openDetails, setOpenDetails] = useState<Set<string>>(new Set());
  const [openOutput, setOpenOutput] = useState<Set<string>>(new Set());
  const [testResult, setTestResult] = useState<Record<string, { ok: boolean; text: string }>>({});
  const [testing, setTesting] = useState<string | null>(null);

  // When it runs / where it goes
  const [when, setWhen] = useState<WhenInfo | null>(null);
  const [whenOpen, setWhenOpen] = useState(false);
  const [spec, setSpec] = useState<ScheduleSpec>({ kind: 'weekdays', hour: 9, minute: 0, weekday: 1, day: 1, every: 15 });
  const [whenBusy, setWhenBusy] = useState(false);
  const [deliverOpen, setDeliverOpen] = useState(false);
  const [peers, setPeers] = useState<Peer[]>([]);

  // Header editing
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Drag reorder
  const dragFrom = useRef<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  const selected = useMemo(() => workflows.find((w) => w.id === selectedId) ?? null, [workflows, selectedId]);
  const isRunning = !!runState && runState.workflow_id === selectedId && runState.status === 'running';

  const reload = useCallback(async () => {
    try {
      const list = await listWorkflows();
      setWorkflows(list);
      return list;
    } catch {
      return [] as Workflow[];
    }
  }, []);

  useEffect(() => {
    reload().then((list) => {
      if (focusWorkflowId && list.some((w) => w.id === focusWorkflowId)) setSelectedId(focusWorkflowId);
      else if (!selectedId && list.length > 0) setSelectedId(list[0].id);
    });
    const unsubData = onDataChanged(() => { reload(); });
    return () => { unsubData.then((fn) => fn()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusWorkflowId]);

  useEffect(() => {
    const unsubStatus = onWorkflowStatusChange((st) => {
      setRunState(st);
      if (st.status !== 'running') setProgress(null);
    });
    const unsubLog = onRunLog((e) => {
      if (e.kind === 'step' && e.level !== 'error') setProgress(e.text);
      if (e.kind === 'tool') setProgress(e.text);
    });
    return () => {
      unsubStatus.then((fn) => fn());
      unsubLog.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    setConfirmDelete(false);
    setEditingName(false);
    setReviseOpen(false);
    setReviseError(null);
    setTestResult({});
    setOpenDetails(new Set());
    setOpenOutput(new Set());
    setWhenOpen(false);
    setDeliverOpen(false);
    if (!selectedId) { setRuns([]); setWhen(null); return; }
    listWorkflowRuns(selectedId, 5).then(setRuns).catch(() => setRuns([]));
    getWorkflowWhen(selectedId).then((w) => {
      setWhen(w);
      if (w.schedule) setSpec(w.schedule);
    }).catch(() => setWhen(null));
  }, [selectedId, runState?.status]);

  useEffect(() => {
    if (deliverOpen) listPeers().then(setPeers).catch(() => setPeers([]));
  }, [deliverOpen]);

  useEffect(() => {
    if (reviseOpen) requestAnimationFrame(() => reviseRef.current?.focus());
  }, [reviseOpen]);

  /* ── Actions ── */

  const draft = async () => {
    const text = ask.trim();
    if (!text || drafting) return;
    setDrafting(true);
    setDraftError(null);
    try {
      const wf = await draftWorkflow(text);
      const list = await reload();
      setSelectedId(list.some((w) => w.id === wf.id) ? wf.id : null);
      setComposing(false);
      setAsk('');
    } catch (e) {
      setDraftError(String(e));
    } finally {
      setDrafting(false);
    }
  };

  const revise = async (instruction: string) => {
    if (!selected || !instruction.trim() || revising) return;
    setRevising(true);
    setReviseError(null);
    try {
      const wf = await reviseWorkflow(selected.id, instruction.trim());
      setWorkflows((prev) => prev.map((w) => (w.id === wf.id ? wf : w)));
      setReviseOpen(false);
      setReviseText('');
      setAddText('');
    } catch (e) {
      setReviseError(String(e));
    } finally {
      setRevising(false);
    }
  };

  const saveSteps = async (steps: WorkflowStep[]) => {
    if (!selected) return;
    const ordered = steps.map((s, i) => ({ ...s, order: i }));
    try {
      const wf = await updateWorkflowSteps(selected.id, ordered);
      setWorkflows((prev) => prev.map((w) => (w.id === wf.id ? wf : w)));
    } catch { /* the list reloads on data-changed */ }
  };

  const removeStep = (id: string) => {
    if (!selected) return;
    void saveSteps(selected.steps.filter((s) => s.id !== id));
  };

  const run = async () => {
    if (!selected || isRunning) return;
    if (selected.steps.length === 0) return;
    setProgress('Starting…');
    setRunState({ workflow_id: selected.id, status: 'running', current_step: 0, total_steps: selected.steps.length, output: [] });
    try {
      const st = await runWorkflow(selected.id);
      setRunState(st);
    } catch (e) {
      setRunState({ workflow_id: selected.id, status: 'failed', current_step: 0, total_steps: selected.steps.length, output: [], error: String(e) });
    } finally {
      setProgress(null);
    }
  };

  const stop = () => {
    if (!selected) return;
    stopWorkflow(selected.id).catch(() => {});
  };

  const test = async (step: WorkflowStep) => {
    if (!selected || testing) return;
    setTesting(step.id);
    try {
      const r = await testWorkflowStep(selected.id, step.id);
      setTestResult((prev) => ({ ...prev, [step.id]: { ok: r.success, text: r.output } }));
      setOpenOutput((prev) => new Set(prev).add(step.id));
    } catch (e) {
      setTestResult((prev) => ({ ...prev, [step.id]: { ok: false, text: String(e) } }));
      setOpenOutput((prev) => new Set(prev).add(step.id));
    } finally {
      setTesting(null);
    }
  };

  const saveWhen = async (next: ScheduleSpec | null) => {
    if (!selected || whenBusy) return;
    setWhenBusy(true);
    try {
      const w = await setWorkflowSchedule(selected.id, next);
      setWhen(w);
      setWhenOpen(false);
      await reload();
    } catch { /* leave the picker open */ } finally {
      setWhenBusy(false);
    }
  };

  const saveDeliver = async (d: Deliver) => {
    if (!selected) return;
    try {
      const wf = await setWorkflowDeliver(selected.id, d);
      setWorkflows((prev) => prev.map((w) => (w.id === wf.id ? wf : w)));
    } catch { /* ignore */ } finally {
      setDeliverOpen(false);
    }
  };

  const commitName = async () => {
    setEditingName(false);
    if (!selected) return;
    const name = nameDraft.trim();
    if (!name || name === selected.name) return;
    try {
      const wf = await updateWorkflow(selected.id, name, undefined);
      setWorkflows((prev) => prev.map((w) => (w.id === wf.id ? wf : w)));
    } catch { /* ignore */ }
  };

  const remove = async () => {
    if (!selected) return;
    if (!confirmDelete) { setConfirmDelete(true); return; }
    try {
      await deleteWorkflow(selected.id);
      const list = await reload();
      setSelectedId(list[0]?.id ?? null);
    } catch { /* ignore */ }
  };

  const toggle = (set: React.Dispatch<React.SetStateAction<Set<string>>>, id: string) =>
    set((prev) => { const n = new Set(prev); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const stepStatus = (i: number): StepStatus => {
    if (!runState || !selected || runState.workflow_id !== selected.id) return 'idle';
    if (runState.status === 'running') return i < runState.current_step ? 'done' : i === runState.current_step ? 'running' : 'idle';
    if (runState.status === 'completed') return 'done';
    if (runState.status === 'failed') return i < runState.current_step ? 'done' : i === runState.current_step ? 'failed' : 'idle';
    if (runState.status === 'stopped') return i < runState.current_step ? 'done' : 'idle';
    return 'idle';
  };

  const filtered = workflows.filter((w) =>
    w.name.toLowerCase().includes(search.toLowerCase()) || (w.description || '').toLowerCase().includes(search.toLowerCase()),
  );

  const showHero = composing || (!selected && workflows.length === 0) || (!selected && !composing && workflows.length > 0 && false);

  /* ── Render ── */

  return (
    <div className="build">
      {/* Sidebar: the list */}
      <aside className="build-side">
        <div className="build-side-head">
          <span className="build-side-title">Workflows</span>
          <button type="button" className="build-btn primary small" onClick={() => { setComposing(true); setSelectedId(null); setDraftError(null); }}>
            New
          </button>
        </div>
        {workflows.length > 4 && (
          <input className="build-search" placeholder="Find a workflow" value={search} onChange={(e) => setSearch(e.target.value)} />
        )}
        <div className="build-side-list">
          {filtered.map((w) => {
            const live = runState && runState.workflow_id === w.id && runState.status === 'running';
            return (
              <button
                type="button"
                key={w.id}
                className={`build-side-item ${w.id === selectedId && !composing ? 'active' : ''}`}
                onClick={() => { setComposing(false); setSelectedId(w.id); }}
              >
                <span className={`build-dot ${live ? 'live' : ''}`} />
                <span className="build-side-name">{w.name}</span>
                <span className="build-side-when">{w.steps.length} step{w.steps.length === 1 ? '' : 's'}</span>
              </button>
            );
          })}
          {workflows.length === 0 && <div className="build-side-empty">Nothing here yet.</div>}
        </div>
        <button type="button" className="build-link" onClick={() => setShowTemplates(true)}>Start from a template</button>
      </aside>

      {/* Main */}
      <section className="build-main">
        {(showHero || !selected) ? (
          <div className="build-hero">
            <div className="build-hero-mark">◉</div>
            <h1 className="build-hero-title">What should happen?</h1>
            <p className="build-hero-sub">Say it the way you would to a colleague. Bob turns it into steps you can run, change, or schedule.</p>
            <textarea
              className="build-ask"
              rows={3}
              placeholder="Every weekday morning, read my calendar and mail and write me a short brief"
              value={ask}
              disabled={drafting}
              onChange={(e) => setAsk(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void draft(); } }}
              autoFocus
            />
            <div className="build-hero-row">
              <button type="button" className="build-btn primary" disabled={drafting || !ask.trim()} onClick={() => void draft()}>
                {drafting ? 'Bob is drafting…' : 'Draft it'}
              </button>
              <span className="build-hint">⌘↩</span>
              {workflows.length > 0 && composing && (
                <button type="button" className="build-btn ghost" onClick={() => { setComposing(false); setSelectedId(workflows[0].id); }}>Cancel</button>
              )}
            </div>
            {draftError && <div className="build-error">{plainError(draftError)}</div>}
            <div className="build-examples">
              {EXAMPLES.map((ex) => (
                <button type="button" key={ex} className="build-chip" onClick={() => setAsk(ex)}>{ex}</button>
              ))}
            </div>
          </div>
        ) : (
          <div className="build-flow">
            {/* Header */}
            <header className="build-head">
              <div className="build-head-text">
                {editingName ? (
                  <input
                    className="build-name-input"
                    value={nameDraft}
                    autoFocus
                    onChange={(e) => setNameDraft(e.target.value)}
                    onBlur={() => void commitName()}
                    onKeyDown={(e) => { if (e.key === 'Enter') void commitName(); if (e.key === 'Escape') setEditingName(false); }}
                  />
                ) : (
                  <h1 className="build-name" title="Click to rename" onClick={() => { setNameDraft(selected.name); setEditingName(true); }}>{selected.name}</h1>
                )}
                {selected.description && <p className="build-desc">{selected.description}</p>}
                <div className="build-when">
                  <span>
                    {when?.schedule ? `Runs ${specText(when.schedule)}` : whenText(selected)}
                    {when?.next_run && when.schedule ? ` · next ${when.next_run}` : ''}
                    {when?.triggers.length ? `, and ${when.triggers.join(', ')}` : ''}
                  </span>
                  <button type="button" className="build-link inline" onClick={() => setWhenOpen((v) => !v)}>{whenOpen ? 'close' : 'change'}</button>
                </div>
                <div className="build-when">
                  <span>Then {deliverText(selected.deliver)}</span>
                  <button type="button" className="build-link inline" onClick={() => setDeliverOpen((v) => !v)}>{deliverOpen ? 'close' : 'change'}</button>
                </div>
              </div>
              <div className="build-head-actions">
                {isRunning ? (
                  <button type="button" className="build-btn danger" onClick={stop}>Stop</button>
                ) : (
                  <button type="button" className="build-btn primary" disabled={selected.steps.length === 0} onClick={() => void run()}>Run</button>
                )}
                <button type="button" className={`build-btn ghost ${reviseOpen ? 'active' : ''}`} onClick={() => { setReviseOpen((v) => !v); setReviseText(''); }}>Change with Bob</button>
                <button type="button" className={`build-btn ghost ${confirmDelete ? 'danger' : ''}`} onClick={() => void remove()} onBlur={() => setConfirmDelete(false)}>
                  {confirmDelete ? 'Delete?' : 'Delete'}
                </button>
              </div>
            </header>

            {/* When it runs */}
            {whenOpen && (
              <div className="build-picker">
                <div className="build-picker-title">When should it run?</div>
                <div className="build-picker-row">
                  <select className="build-select" value={spec.kind} onChange={(e) => setSpec({ ...spec, kind: e.target.value as ScheduleSpec['kind'] })}>
                    <option value="daily">Every day</option>
                    <option value="weekdays">Every weekday</option>
                    <option value="weekly">Every week</option>
                    <option value="monthly">Every month</option>
                    <option value="hourly">Every hour</option>
                    <option value="minutes">Every few minutes</option>
                  </select>
                  {spec.kind === 'weekly' && (
                    <select className="build-select" value={spec.weekday} onChange={(e) => setSpec({ ...spec, weekday: Number(e.target.value) })}>
                      {WEEKDAYS.map((d, i) => <option key={d} value={i}>{d}</option>)}
                    </select>
                  )}
                  {spec.kind === 'monthly' && (
                    <label className="build-inline-label">on day
                      <input className="build-number" type="number" min={1} max={28} value={spec.day} onChange={(e) => setSpec({ ...spec, day: Math.min(28, Math.max(1, Number(e.target.value) || 1)) })} />
                    </label>
                  )}
                  {spec.kind === 'minutes' && (
                    <label className="build-inline-label">every
                      <input className="build-number" type="number" min={1} max={59} value={spec.every} onChange={(e) => setSpec({ ...spec, every: Math.min(59, Math.max(1, Number(e.target.value) || 1)) })} />
                      minutes
                    </label>
                  )}
                  {(spec.kind === 'daily' || spec.kind === 'weekdays' || spec.kind === 'weekly' || spec.kind === 'monthly') && (
                    <label className="build-inline-label">at
                      <input
                        className="build-time"
                        type="time"
                        value={`${pad(spec.hour)}:${pad(spec.minute)}`}
                        onChange={(e) => { const [h, m] = e.target.value.split(':').map(Number); if (!Number.isNaN(h)) setSpec({ ...spec, hour: h, minute: Number.isNaN(m) ? 0 : m }); }}
                      />
                    </label>
                  )}
                </div>
                <div className="build-picker-row">
                  <button type="button" className="build-btn primary small" disabled={whenBusy} onClick={() => void saveWhen(spec)}>{whenBusy ? 'Saving…' : `Run ${specText(spec)}`}</button>
                  <button type="button" className="build-btn ghost small" disabled={whenBusy} onClick={() => void saveWhen(null)}>Only when I press Run</button>
                  <button type="button" className="build-link" onClick={() => onOpenPanel('triggers', selected.id)}>or when something happens…</button>
                </div>
                <div className="build-picker-note">Times are this computer's local time.</div>
              </div>
            )}

            {/* Where the result goes */}
            {deliverOpen && (
              <div className="build-picker">
                <div className="build-picker-title">Where should the result go?</div>
                <div className="build-picker-row wrap">
                  {([['chat', 'Show it in chat'], ['document', 'Save it as a document'], ['panel', 'Put it in a panel'], ['notify', 'Notify me']] as [Deliver, string][]).map(([d, label]) => (
                    <button type="button" key={String(d)} className={`build-chip ${selected.deliver === d || (!selected.deliver && d === 'chat') ? 'on' : ''}`} onClick={() => void saveDeliver(d)}>{label}</button>
                  ))}
                  {peers.filter((p) => p.capabilities.includes('send')).map((p) => (
                    <button type="button" key={p.id} className={`build-chip ${typeof selected.deliver === 'object' && selected.deliver.app === p.app ? 'on' : ''}`} onClick={() => void saveDeliver({ app: p.app })}>Send it to {p.app}</button>
                  ))}
                </div>
                <div className="build-picker-note">Bob also says a line in chat whichever you pick, so a scheduled run never finishes in silence.</div>
              </div>
            )}

            {/* Revise box */}
            {reviseOpen && (
              <div className="build-revise">
                <textarea
                  ref={reviseRef}
                  rows={2}
                  placeholder='Tell Bob what to change: "make step 2 post to #sales instead", "only weekdays", "add a step that saves it to Notes"'
                  value={reviseText}
                  disabled={revising}
                  onChange={(e) => setReviseText(e.target.value)}
                  onKeyDown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void revise(reviseText); } if (e.key === 'Escape') setReviseOpen(false); }}
                />
                <div className="build-revise-row">
                  <button type="button" className="build-btn primary small" disabled={revising || !reviseText.trim()} onClick={() => void revise(reviseText)}>
                    {revising ? 'Bob is changing it…' : 'Apply'}
                  </button>
                  <button type="button" className="build-btn ghost small" onClick={() => setReviseOpen(false)}>Cancel</button>
                  {reviseError && <span className="build-error inline">{plainError(reviseError)}</span>}
                </div>
              </div>
            )}

            {/* Result strip */}
            {runState && runState.workflow_id === selected.id && runState.status !== 'running' && runState.status !== 'idle' && (
              <div className={`build-result ${runState.status}`}>
                {runState.status === 'completed' && <span>Done. {runState.total_steps} step{runState.total_steps === 1 ? '' : 's'} ran; the result is in chat and below.</span>}
                {runState.status === 'failed' && (
                  <span>Stopped at step {Math.min(runState.current_step + 1, runState.total_steps)}: {plainError(runState.error)}</span>
                )}
                {runState.status === 'stopped' && <span>Stopped after step {runState.current_step}.</span>}
                <button type="button" className="build-link inline" onClick={() => setRunState(null)}>dismiss</button>
              </div>
            )}

            {/* Steps */}
            <ol className="build-steps">
              {selected.steps.map((step, i) => {
                const status = stepStatus(i);
                const output = runState && runState.workflow_id === selected.id ? runState.output?.[i] : undefined;
                const t = testResult[step.id];
                const shown = t ? t.text : output;
                const showOutput = openOutput.has(step.id) && !!shown;
                return (
                  <li
                    key={step.id}
                    className={`build-step ${status} ${dragOver === i ? 'over' : ''}`}
                    draggable={!isRunning}
                    onDragStart={() => { dragFrom.current = i; }}
                    onDragOver={(e) => { e.preventDefault(); setDragOver(i); }}
                    onDragLeave={() => setDragOver(null)}
                    onDrop={(e) => {
                      e.preventDefault();
                      const from = dragFrom.current;
                      setDragOver(null);
                      dragFrom.current = null;
                      if (from === null || from === i) return;
                      const next = [...selected.steps];
                      const [moved] = next.splice(from, 1);
                      next.splice(i, 0, moved);
                      void saveSteps(next);
                    }}
                  >
                    <div className="build-step-num">
                      {status === 'running' ? <span className="build-spin" /> : status === 'done' ? '✓' : status === 'failed' ? '✕' : i + 1}
                    </div>
                    <div className="build-step-body">
                      <div className="build-step-line">
                        <span className="build-step-label">{step.label}</span>
                        <span className="build-step-subject">{stepSubject(step)}</span>
                      </div>
                      {status === 'running' && progress && <div className="build-step-progress">{progress}</div>}
                      {status === 'failed' && runState?.error && <div className="build-step-fail">{plainError(runState.error)}</div>}
                      {shown && (
                        <button type="button" className="build-link inline" onClick={() => toggle(setOpenOutput, step.id)}>
                          {showOutput ? 'hide result' : t ? (t.ok ? 'show test result' : 'show what went wrong') : 'show result'}
                        </button>
                      )}
                      {showOutput && <pre className={`build-step-output ${t && !t.ok ? 'bad' : ''}`}>{shown}</pre>}
                      {openDetails.has(step.id) && (
                        <pre className="build-step-details">{JSON.stringify({ type: step.step_type, ...step.config }, null, 2)}</pre>
                      )}
                      <div className="build-step-actions">
                        <button type="button" className="build-link" onClick={() => { setReviseOpen(true); setReviseText(`Step ${i + 1} ("${step.label}"): `); }}>Change</button>
                        <button type="button" className="build-link" disabled={!!testing || isRunning} onClick={() => void test(step)}>{testing === step.id ? 'Testing…' : 'Test'}</button>
                        <button type="button" className="build-link" onClick={() => toggle(setOpenDetails, step.id)}>{openDetails.has(step.id) ? 'Hide details' : 'Details'}</button>
                        <button type="button" className="build-link quiet" disabled={isRunning} onClick={() => removeStep(step.id)}>Remove</button>
                      </div>
                    </div>
                    <div className="build-step-grip" title="Drag to reorder">⋮⋮</div>
                  </li>
                );
              })}
            </ol>

            {selected.steps.length === 0 && (
              <div className="build-empty-steps">No steps yet. Tell Bob what should happen.</div>
            )}

            {/* Add a step */}
            <div className="build-add">
              <input
                className="build-add-input"
                placeholder={selected.steps.length === 0 ? 'What should happen first?' : 'What should happen next?'}
                value={addText}
                disabled={revising}
                onChange={(e) => setAddText(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter' && addText.trim()) { e.preventDefault(); void revise(`Add a new step at the end: ${addText.trim()}`); } }}
              />
              <button type="button" className="build-btn ghost small" disabled={revising || !addText.trim()} onClick={() => void revise(`Add a new step at the end: ${addText.trim()}`)}>
                {revising ? 'Adding…' : 'Add step'}
              </button>
            </div>

            {/* Recent runs */}
            {runs.length > 0 && (
              <div className="build-runs">
                <div className="build-runs-title">Recent runs</div>
                {runs.map((r) => (
                  <div key={r.id} className={`build-run ${r.status}`}>
                    <span className={`build-dot ${r.status}`} />
                    <span className="build-run-when">{relTime(r.created_at)}</span>
                    <span className="build-run-text">
                      {r.status === 'completed' ? 'ran to the end' : r.status === 'failed' ? `stopped at step ${Math.min(r.current_step + 1, r.total_steps)}: ${plainError(r.error)}` : r.status}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </section>

      {showTemplates && (
        <div className="build-modal" onClick={() => setShowTemplates(false)}>
          <div className="build-modal-card" onClick={(e) => e.stopPropagation()}>
            <TemplateGallery
              onCreated={(wf) => { setShowTemplates(false); reload().then(() => { setComposing(false); setSelectedId(wf.id); }); }}
              onClose={() => setShowTemplates(false)}
            />
          </div>
        </div>
      )}
    </div>
  );
}
