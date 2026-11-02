import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import type { Agent, MemoryEntry, ToolCall } from '../types';
import {
  getActivityLog,
  clearActivityLog,
  onAgentActivity,
  getPendingToolCalls,
  onToolCallPending,
  listMemories,
  onDataChanged,
} from '../hooks/api';
import type { AgentActivity } from '../hooks/api';
import './DevDrawer.css';

type TabId = 'actions' | 'agents' | 'tools' | 'memory';

interface Props {
  open: boolean;
  onToggle: () => void;
  agents: Agent[];
  height: number;
  onHeightChange: (h: number) => void;
}

const TABS: { id: TabId; label: string; hint: string }[] = [
  { id: 'actions', label: 'Action Log', hint: '⌘1' },
  { id: 'agents', label: 'Agents', hint: '⌘2' },
  { id: 'tools', label: 'Tools', hint: '⌘3' },
  { id: 'memory', label: 'Memory', hint: '⌘4' },
];

const MIN_H = 140;
const MAX_H = 600;

function fmtTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  } catch {
    return '';
  }
}

export default function DevDrawer({
  open,
  onToggle,
  agents,
  height,
  onHeightChange,
}: Props) {
  const [tab, setTab] = useState<TabId>('actions');
  const [activity, setActivity] = useState<AgentActivity[]>([]);
  const [tools, setTools] = useState<ToolCall[]>([]);
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [filter, setFilter] = useState('');
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const dragRef = useRef<{ startY: number; startH: number } | null>(null);

  // Subscribe to live feeds when the drawer is open.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const refresh = () => {
      getActivityLog(undefined, 200).then((rows) => {
        if (!cancelled) setActivity(rows);
      }).catch(() => {});
      getPendingToolCalls().then((rows) => {
        if (!cancelled) setTools(rows);
      }).catch(() => {});
      listMemories().then((rows) => {
        if (!cancelled) setMemories(rows);
      }).catch(() => {});
    };
    refresh();
    const unsubA = onAgentActivity((a) => {
      setActivity((prev) => [a, ...prev].slice(0, 200));
    });
    const unsubT = onToolCallPending((c) => {
      setTools((prev) => [c, ...prev].slice(0, 100));
    });
    const unsubD = onDataChanged(() => refresh());
    return () => {
      cancelled = true;
      unsubA.then((fn) => fn());
      unsubT.then((fn) => fn());
      unsubD.then((fn) => fn());
    };
  }, [open]);

  // Keyboard shortcuts - Cmd-1..4 switch tabs while drawer is open.
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;
      const target = e.target as HTMLElement | null;
      const inField =
        target &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.isContentEditable);
      if (inField) return;
      if (e.key === '1') { e.preventDefault(); setTab('actions'); }
      else if (e.key === '2') { e.preventDefault(); setTab('agents'); }
      else if (e.key === '3') { e.preventDefault(); setTab('tools'); }
      else if (e.key === '4') { e.preventDefault(); setTab('memory'); }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open]);

  // Resize via top edge drag.
  const onDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = { startY: e.clientY, startH: height };
      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return;
        const dy = dragRef.current.startY - ev.clientY;
        const next = Math.max(MIN_H, Math.min(MAX_H, dragRef.current.startH + dy));
        onHeightChange(next);
      };
      const onUp = () => {
        dragRef.current = null;
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    },
    [height, onHeightChange],
  );

  const filteredActivity = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return activity;
    return activity.filter((a) =>
      `${a.agent_name} ${a.action} ${a.detail ?? ''} ${a.workflow_name ?? ''}`.toLowerCase().includes(q),
    );
  }, [activity, filter]);

  const selectedAgent = agents.find((a) => a.id === selectedAgentId) ?? null;
  const agentRecentActivity = useMemo(() => {
    if (!selectedAgent) return [];
    return activity
      .filter((a) => a.agent_name === selectedAgent.name)
      .slice(0, 50);
  }, [selectedAgent, activity]);

  if (!open) {
    return (
      <button
        className="dev-drawer-handle-collapsed"
        onClick={onToggle}
        title="Open dev drawer (⌘J)" aria-label="Open dev drawer (⌘J)"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="18 15 12 9 6 15" />
        </svg>
        <span>Console</span>
      </button>
    );
  }

  return (
    <div className="dev-drawer" style={{ height }} role="region" aria-label="Developer console">
      <div className="dev-drawer-resizer" onMouseDown={onDragStart} />
      <div className="dev-drawer-tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`dev-drawer-tab ${tab === t.id ? 'active' : ''}`}
            onClick={() => setTab(t.id)}
          >
            <span>{t.label}</span>
            <span className="dev-drawer-tab-hint">{t.hint}</span>
          </button>
        ))}
        <div className="dev-drawer-spacer" />
        <input
          className="dev-drawer-filter"
          placeholder="Filter…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <button
          className="dev-drawer-close"
          onClick={onToggle}
          title="Close (⌘J)"
          aria-label="Close console"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </button>
      </div>

      <div className="dev-drawer-body">
        {tab === 'actions' && (
          <div className="dev-list">
            <div className="dev-list-toolbar">
              <span>{filteredActivity.length} entries</span>
              <button
                className="dev-list-action"
                onClick={() => clearActivityLog().then(() => setActivity([])).catch(() => {})}
              >
                Clear
              </button>
            </div>
            {filteredActivity.length === 0 ? (
              <div className="dev-empty">No activity yet.</div>
            ) : (
              filteredActivity.map((a) => (
                <div key={a.id} className="dev-row">
                  <span className="dev-row-time">{fmtTime(a.timestamp)}</span>
                  <span className="dev-row-tag">{a.agent_name}</span>
                  <span className="dev-row-kind">{a.action}</span>
                  <span className="dev-row-summary">{a.detail || ''}</span>
                </div>
              ))
            )}
          </div>
        )}

        {tab === 'agents' && (
          <div className="dev-split">
            <div className="dev-split-left">
              {agents.length === 0 && <div className="dev-empty">No agents.</div>}
              {agents.map((a) => (
                <button
                  key={a.id}
                  className={`dev-pill ${selectedAgentId === a.id ? 'active' : ''}`}
                  onClick={() => setSelectedAgentId(a.id)}
                >
                  <span className={`dev-dot ${a.enabled ? 'ok' : 'off'}`} />
                  <span className="dev-pill-name">{a.name}</span>
                  <span className="dev-pill-meta">{a.model}</span>
                </button>
              ))}
            </div>
            <div className="dev-split-right">
              {selectedAgent ? (
                <>
                  <div className="dev-detail-head">
                    <h4>{selectedAgent.name}</h4>
                    <span>{selectedAgent.provider} · {selectedAgent.model}</span>
                  </div>
                  <div className="dev-list">
                    {agentRecentActivity.length === 0 ? (
                      <div className="dev-empty">No recent activity for this agent.</div>
                    ) : (
                      agentRecentActivity.map((a) => (
                        <div key={a.id} className="dev-row">
                          <span className="dev-row-time">{fmtTime(a.timestamp)}</span>
                          <span className="dev-row-kind">{a.action}</span>
                          <span className="dev-row-summary">{a.detail || ''}</span>
                        </div>
                      ))
                    )}
                  </div>
                </>
              ) : (
                <div className="dev-empty">Pick an agent to inspect.</div>
              )}
            </div>
          </div>
        )}

        {tab === 'tools' && (
          <div className="dev-list">
            {tools.length === 0 ? (
              <div className="dev-empty">No tool calls yet.</div>
            ) : (
              tools.map((t) => (
                <div key={t.id} className="dev-row dev-row-block">
                  <div className="dev-row-line">
                    <span className="dev-row-time">{fmtTime(t.created_at)}</span>
                    <span className={`dev-row-status ${t.status}`}>{t.status}</span>
                    <span className="dev-row-kind">{t.tool_name}</span>
                  </div>
                  <pre className="dev-pre">{JSON.stringify(t.arguments, null, 2)}</pre>
                  {t.result && (
                    <pre className="dev-pre subtle">{
                      typeof t.result === 'string' ? t.result : JSON.stringify(t.result, null, 2)
                    }</pre>
                  )}
                </div>
              ))
            )}
          </div>
        )}

        {tab === 'memory' && (
          <div className="dev-list">
            {memories.length === 0 ? (
              <div className="dev-empty">Memory is empty.</div>
            ) : (
              memories.map((m) => (
                <div key={m.id} className="dev-row dev-row-block">
                  <div className="dev-row-line">
                    <span className="dev-row-tag">{m.category}</span>
                    <span className="dev-row-kind">{m.key}</span>
                    <span className="dev-row-time">{fmtTime(m.updated_at)}</span>
                  </div>
                  <div className="dev-row-summary wrap">{m.value}</div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}
