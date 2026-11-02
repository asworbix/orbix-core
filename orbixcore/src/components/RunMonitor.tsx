import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import {
  runLogRecent, onRunLog, onMessageChunk,
  bobCancel, imagineStop, imagineNudge, stopAgentShift, sendToAgentShift, stopWorkflow,
  type RunLogEntry,
} from '../hooks/api';
import './RunMonitor.css';

/* The Run monitor: a small square window, a glance face like a watch.
   The face shows what is running, the step it is on, the live narration,
   the last few events, and Stop. "Log" expands the window into the full
   timeline (filters, every entry with its detail, steer) and collapses
   back. Every kind of run lands here (a chat turn, an Imagine run, an
   agent shift, a workflow run) through the backend's run log (run-log
   events, backfilled from run_log_recent); the live narration rides the
   message-chunk stream. */

type Status = 'running' | 'done' | 'failed' | 'stopped';
interface RunView { id: string; kind: string; title: string; status: Status; startedAt: string; endedAt?: string }

const FACE: [number, number] = [380, 380];
const FULL: [number, number] = [460, 680];

const FILTERS: { id: string; label: string; match: (e: RunLogEntry) => boolean }[] = [
  { id: 'all', label: 'All', match: () => true },
  { id: 'steps', label: 'Steps', match: (e) => e.kind === 'step' || e.kind === 'run' || e.kind === 'user' },
  { id: 'tools', label: 'Tools', match: (e) => e.kind === 'tool' },
  { id: 'model', label: 'Model', match: (e) => e.kind === 'model' },
  { id: 'egress', label: 'Egress', match: (e) => e.kind === 'egress' },
  { id: 'errors', label: 'Errors', match: (e) => e.kind === 'error' || e.level === 'error' },
];

const KIND_TAG: Record<RunLogEntry['kind'], string> = {
  run: 'RUN', step: 'STEP', tool: 'TOOL', model: 'MODEL', egress: 'EGRESS', error: 'ERR', user: 'YOU',
};
const RUN_KIND_LABEL: Record<string, string> = { chat: 'Bob', imagine: 'Imagine', shift: 'Shift', workflow: 'Workflow' };
const STATUS_LABEL: Record<Status, string> = { running: 'Running', done: 'Done', failed: 'Failed', stopped: 'Stopped' };

/* The stream carries the model's raw turn text; hide think blocks and
   action tag bodies so the live line reads as narration. */
function stripMachinery(s: string): string {
  return s
    .replace(/<think>[\s\S]*?(<\/think>|$)/g, '')
    .replace(/<action>[\s\S]*?(<\/action>|$)/g, '')
    .trim();
}

function clock(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return '';
  }
}

function elapsed(fromIso: string, toIso?: string): string {
  const ms = (toIso ? new Date(toIso).getTime() : Date.now()) - new Date(fromIso).getTime();
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/* Vite dev only: ?demo=1 seeds a running Imagine so the face can be
   designed and screenshotted without the app. */
const DEMO = import.meta.env.DEV && new URLSearchParams(window.location.search).get('demo') === '1';
function demoEntries(): RunLogEntry[] {
  const t0 = Date.now() - 42_000;
  const at = (s: number) => new Date(t0 + s * 1000).toISOString();
  let seq = 0;
  const e = (kind: RunLogEntry['kind'], level: RunLogEntry['level'], text: string, s: number, extra: Partial<RunLogEntry> = {}): RunLogEntry =>
    ({ seq: ++seq, ts: at(s), run_id: 'imagine-demo', kind, level, text, ...extra });
  return [
    e('run', 'info', 'Free rein', 0, { run_kind: 'imagine', status: 'started' }),
    e('step', 'info', 'Looking at what is open…', 1),
    e('step', 'info', 'Loading model…', 2),
    e('model', 'info', 'Model turn: 412 chars, 2 tool calls, 6.1s', 9),
    e('tool', 'ok', 'Read Calendar', 10, { detail: 'events: 09:00 Standup, 11:30 Design review, 14:00 Board call' }),
    e('tool', 'ok', "Searched the web for 'Q3 board pack template'", 12, { detail: '5 results' }),
    e('egress', 'ok', 'web_search allowed to lite.duckduckgo.com: 1 redaction(s) applied before egress', 12),
    e('model', 'info', 'Model turn: 1.2k chars, 1 tool call, 8.4s', 21),
    e('step', 'info', 'Creating slides…', 22),
  ];
}

export default function RunMonitor() {
  const [entries, setEntries] = useState<RunLogEntry[]>(() => (DEMO ? demoEntries() : []));
  const [full, setFull] = useState(false);
  const [filter, setFilter] = useState('all');
  const [scope, setScope] = useState<'current' | 'all'>('current');
  const [live, setLive] = useState<{ runId: string; text: string } | null>(
    DEMO ? { runId: 'imagine-demo', text: 'Board call at two: I will build the deck from the calendar and the last quarter numbers.' } : null,
  );
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());
  const [draft, setDraft] = useState('');
  const [pinned, setPinned] = useState(true);
  const [, tick] = useState(0);
  const seen = useRef<Set<number>>(new Set());
  const listRef = useRef<HTMLDivElement>(null);

  const push = useCallback((batch: RunLogEntry[]) => {
    const fresh = batch.filter((e) => !seen.current.has(e.seq));
    if (!fresh.length) return;
    fresh.forEach((e) => seen.current.add(e.seq));
    setEntries((prev) => {
      const next = [...prev, ...fresh];
      return next.length > 3000 ? next.slice(-2400) : next;
    });
  }, []);

  useEffect(() => {
    if (DEMO) return;
    runLogRecent(600).then(push).catch(() => {});
    const subs = [
      onRunLog((e) => push([e])),
      onMessageChunk((e) => setLive({ runId: e.message_id, text: stripMachinery(e.content ?? '') })),
    ];
    return () => { subs.forEach((p) => p.then((un) => un()).catch(() => {})); };
  }, [push]);

  // The elapsed clock on the face ticks once a second while something runs.
  const runs = useMemo(() => {
    const map = new Map<string, RunView>();
    for (const e of entries) {
      if (e.kind !== 'run') continue;
      if (e.status === 'started') {
        map.set(e.run_id, { id: e.run_id, kind: e.run_kind ?? 'run', title: e.text, status: 'running', startedAt: e.ts });
      } else if (e.status) {
        const r = map.get(e.run_id);
        if (r) { r.status = e.status; r.endedAt = e.ts; }
        else map.set(e.run_id, { id: e.run_id, kind: e.run_kind ?? 'run', title: e.text, status: e.status, startedAt: e.ts, endedAt: e.ts });
      }
    }
    return [...map.values()];
  }, [entries]);

  const active: RunView | null = useMemo(() => {
    const running = [...runs].reverse().find((r) => r.status === 'running');
    return running ?? runs[runs.length - 1] ?? null;
  }, [runs]);
  const running = !!active && active.status === 'running';

  useEffect(() => {
    if (!running) return;
    const id = window.setInterval(() => tick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [running]);

  const currentStep = useMemo(() => {
    if (!active) return null;
    for (let i = entries.length - 1; i >= 0; i--) {
      const e = entries[i];
      if (e.run_id === active.id && e.kind === 'step') return e.text;
    }
    return null;
  }, [entries, active]);

  const activeEntries = useMemo(
    () => (active ? entries.filter((e) => e.run_id === active.id) : []),
    [entries, active],
  );

  const visible = useMemo(() => {
    const f = FILTERS.find((x) => x.id === filter) ?? FILTERS[0];
    return entries.filter((e) => f.match(e) && (scope === 'all' || !active || e.run_id === active.id));
  }, [entries, filter, scope, active]);

  useEffect(() => {
    const el = listRef.current;
    if (el && pinned) el.scrollTop = el.scrollHeight;
  }, [visible.length, live, pinned, full]);

  const onScroll = () => {
    const el = listRef.current;
    if (!el) return;
    setPinned(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
  };

  const canSteer = running && (active!.kind === 'shift' || active!.kind === 'imagine');
  const liveText = running && live && live.runId === active!.id ? live.text : '';

  const resize = (to: [number, number]) => {
    if (DEMO) return;
    try { getCurrentWindow().setSize(new LogicalSize(to[0], to[1])).catch(() => {}); } catch { /* not in Tauri */ }
  };
  const toggleFull = () => {
    const next = !full;
    setFull(next);
    resize(next ? FULL : FACE);
  };

  const stop = () => {
    if (!active) return;
    const p = active.kind === 'chat' ? bobCancel()
      : active.kind === 'imagine' ? imagineStop()
      : active.kind === 'shift' ? stopAgentShift(active.id)
      : active.kind === 'workflow' ? stopWorkflow()
      : Promise.resolve();
    p.catch(() => {});
  };

  const steer = async () => {
    const text = draft.trim();
    if (!text || !active || !canSteer) return;
    setDraft('');
    try {
      if (active.kind === 'shift') await sendToAgentShift(active.id, text);
      else await imagineNudge(text);
    } catch { /* the run just ended */ }
  };

  const copyLog = () => {
    const text = visible
      .map((e) => `${clock(e.ts)}  ${KIND_TAG[e.kind].padEnd(6)} ${e.text}${e.detail ? `\n${e.detail.split('\n').map((l) => `        ${l}`).join('\n')}` : ''}`)
      .join('\n');
    navigator.clipboard.writeText(text).catch(() => {});
  };

  const toggle = (seq: number) => setExpanded((prev) => {
    const next = new Set(prev);
    if (next.has(seq)) next.delete(seq); else next.add(seq);
    return next;
  });

  const kindLabel = active ? (RUN_KIND_LABEL[active.kind] ?? active.kind) : null;
  const headline = !active
    ? 'Nothing running'
    : running
      ? (currentStep ?? 'Working…')
      : STATUS_LABEL[active.status];
  const recent = activeEntries.filter((e) => e.kind !== 'run').slice(-3);

  // ── The face ──
  if (!full) {
    return (
      <div className={`run-monitor face ${active?.status ?? 'idle'}`}>
        <header className="rm-face-top" data-tauri-drag-region>
          <span className={`rm-dot ${active?.status ?? 'idle'}`} />
          <span className="rm-face-kind mono" data-tauri-drag-region>
            {kindLabel ? kindLabel.toUpperCase() : 'RUN MONITOR'}
          </span>
          {active && (
            <span className="rm-face-clock mono">{elapsed(active.startedAt, running ? undefined : active.endedAt)}</span>
          )}
          {running && <button className="rm-face-stop" onClick={stop} title="Stop this run">Stop</button>}
        </header>

        <div className="rm-face-center">
          {active && <div className="rm-face-title" title={active.title}>{active.title}</div>}
          <div className="rm-face-headline">{headline}</div>
          {liveText && <div className="rm-face-live">{liveText}</div>}
          {running && !liveText && <div className="rm-thinking" aria-hidden="true"><span /><span /><span /></div>}
          {!active && <div className="rm-face-hint">Bob, Imagine, agent shifts and workflows show up here as they run.</div>}
        </div>

        <div className="rm-face-recent">
          {recent.map((e) => (
            <div key={e.seq} className={`rm-face-row level-${e.level}`}>
              <span className={`rm-tag mono tag-${e.kind}`}>{KIND_TAG[e.kind]}</span>
              <span className="rm-face-row-text">{e.text}</span>
            </div>
          ))}
        </div>

        <footer className="rm-face-foot">
          <button className="rm-chip" onClick={toggleFull}>Log</button>
          {canSteer && (
            <input
              className="rm-face-steer"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); steer(); } }}
              placeholder="Steer…"
            />
          )}
        </footer>
      </div>
    );
  }

  // ── The full log ──
  return (
    <div className="run-monitor">
      <header className="rm-header" data-tauri-drag-region>
        <span className={`rm-dot ${active?.status ?? 'idle'}`} />
        <div className="rm-title" data-tauri-drag-region>
          <strong title={active ? active.title : ''}>{active ? `${kindLabel}: ${active.title}` : 'No run yet'}</strong>
          <span className="rm-sub">{active ? (running ? (currentStep ?? 'Working…') : STATUS_LABEL[active.status]) : 'Runs appear here as they start'}</span>
        </div>
        {running && <button className="rm-stop" onClick={stop}>Stop</button>}
        <button className="rm-chip" onClick={toggleFull} title="Back to the face">Face</button>
      </header>

      <div className="rm-bar">
        <div className="rm-filters">
          {FILTERS.map((f) => (
            <button key={f.id} className={`rm-chip ${filter === f.id ? 'active' : ''}`} onClick={() => setFilter(f.id)}>
              {f.label}
            </button>
          ))}
        </div>
        <div className="rm-bar-right">
          <button
            className={`rm-chip ${scope === 'all' ? 'active' : ''}`}
            onClick={() => setScope((s) => (s === 'all' ? 'current' : 'all'))}
            title={scope === 'all' ? 'Showing every run' : 'Showing the current run'}
          >
            {scope === 'all' ? 'All runs' : 'This run'}
          </button>
          <button className="rm-chip" onClick={copyLog} title="Copy the visible log">Copy</button>
        </div>
      </div>

      <div className="rm-list" ref={listRef} onScroll={onScroll}>
        {visible.length === 0 && (
          <div className="rm-empty">
            {entries.length === 0
              ? 'When Bob, an agent, a workflow, or Imagine runs, every step, tool call, model turn, and egress decision shows up here.'
              : 'Nothing matches this filter.'}
          </div>
        )}
        {visible.map((e) => {
          const open = expanded.has(e.seq);
          return (
            <div key={e.seq} className={`rm-row kind-${e.kind} level-${e.level}${e.detail ? ' has-detail' : ''}`}>
              <span className="rm-time mono">{clock(e.ts)}</span>
              <span className={`rm-tag mono tag-${e.kind}`}>{KIND_TAG[e.kind]}</span>
              <span className="rm-text" onClick={() => e.detail && toggle(e.seq)}>
                {e.text}
                {e.detail && <span className="rm-more">{open ? ' ▾' : ' ▸'}</span>}
              </span>
              {e.detail && open && <pre className="rm-detail">{e.detail}</pre>}
            </div>
          );
        })}
        {liveText && <div className="rm-live">{liveText}</div>}
        {running && !liveText && <div className="rm-thinking" aria-hidden="true"><span /><span /><span /></div>}
      </div>

      <div className="rm-composer">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); steer(); } }}
          placeholder={canSteer ? `Steer ${active!.kind === 'shift' ? active!.title : 'Bob'}…` : running ? 'This run cannot be steered' : 'No run to steer'}
          disabled={!canSteer}
        />
        <button onClick={steer} disabled={!canSteer || !draft.trim()}>Send</button>
      </div>
    </div>
  );
}
