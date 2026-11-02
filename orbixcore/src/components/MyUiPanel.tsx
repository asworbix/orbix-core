import { useState, useEffect, useCallback } from 'react';
import type { DynamicPanel as PanelSpec, DynamicPanelBlock } from '../types';
import {
  listDynamicPanels,
  deleteDynamicPanel,
  onDataChanged,
  runWorkflow,
} from '../hooks/api';
import DynamicPanel from './DynamicPanel';
import './Panel.css';
import './MyUiPanel.css';

interface Props {
  onClose: () => void;
}

export default function MyUiPanel({ onClose }: Props) {
  const [panels, setPanels] = useState<PanelSpec[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listDynamicPanels()
      .then((rows) => {
        rows.sort(
          (a, b) =>
            new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
        );
        setPanels(rows);
        setActiveId((cur) => cur ?? rows[0]?.id ?? null);
      })
      .catch(() => setPanels([]));
  }, []);

  useEffect(() => {
    refresh();
    const unsub = onDataChanged(() => refresh());
    return () => {
      unsub.then((fn) => fn());
    };
  }, [refresh]);

  const handleDelete = useCallback(
    async (id: string) => {
      if (!confirm('Delete this panel?')) return;
      try {
        await deleteDynamicPanel(id);
        if (id === activeId) setActiveId(null);
        refresh();
      } catch {
        /* ignore */
      }
    },
    [activeId, refresh],
  );

  const handleAction = useCallback(async (block: DynamicPanelBlock) => {
    const target = typeof block.target === 'string' ? block.target : '';
    const kind = typeof block.action === 'string' ? block.action : 'workflow';
    if (!target) return;

    if (kind === 'workflow') {
      try {
        await runWorkflow(target);
      } catch {
        /* surfaced elsewhere */
      }
    }
    // Future: 'panel' (open another panel), 'send' (post a chat message), etc.
  }, []);

  const active = panels.find((p) => p.id === activeId) ?? null;

  return (
    <div className="panel my-ui-panel">
      <div className="panel-header">
        <div className="panel-title">
          <span>My UI</span>
          <span className="my-ui-count">{panels.length}</span>
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      <div className="my-ui-body">
        <div className="my-ui-list">
          {panels.length === 0 && (
            <div className="my-ui-empty">
              <div className="my-ui-empty-title">No panels yet</div>
              <div className="my-ui-empty-sub">
                Ask Bob: <em>"build me a panel that shows…"</em>
              </div>
            </div>
          )}
          {panels.map((p) => (
            <button
              key={p.id}
              className={`my-ui-item ${p.id === activeId ? 'active' : ''}`}
              onClick={() => setActiveId(p.id)}
            >
              <div className="my-ui-item-title">{p.title}</div>
              <div className="my-ui-item-meta">
                <span>{p.created_by}</span>
                <span>·</span>
                <span>{p.blocks.length} blocks</span>
              </div>
              <span
                className="my-ui-item-del"
                role="button"
                aria-label="Delete"
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(p.id);
                }}
              >
                ×
              </span>
            </button>
          ))}
        </div>

        <div className="my-ui-stage">
          {active ? (
            <DynamicPanel panel={active} onAction={handleAction} />
          ) : (
            <div className="my-ui-stage-empty">
              {panels.length === 0
                ? 'Bob will build panels here when you ask him to.'
                : 'Pick a panel from the list.'}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

