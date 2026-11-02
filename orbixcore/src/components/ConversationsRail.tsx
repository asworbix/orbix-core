import { useState, useEffect, useCallback, useMemo } from 'react';
import type { ConversationSummary, Message } from '../types';
import {
  listConversations,
  loadConversation,
  deleteConversation,
  renameConversation,
  saveCurrentConversation,
  clearConversation,
  onDataChanged,
  onMenuAction,
} from '../hooks/api';
import './ConversationsRail.css';

interface Props {
  activeId: string | null;
  onSelect: (id: string | null, messages: Message[]) => void;
  onNew: () => void;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}

export default function ConversationsRail({
  activeId,
  onSelect,
  onNew,
  collapsed,
  onToggleCollapsed,
}: Props) {
  const [items, setItems] = useState<ConversationSummary[]>([]);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');

  const refresh = useCallback(() => {
    listConversations()
      .then((rows) => {
        rows.sort(
          (a, b) =>
            new Date(b.last_message_at).getTime() -
            new Date(a.last_message_at).getTime(),
        );
        setItems(rows);
      })
      .catch(() => setItems([]));
  }, []);

  useEffect(() => {
    refresh();
    const unsub = onDataChanged(() => refresh());
    return () => {
      unsub.then((fn) => fn());
    };
  }, [refresh]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((c) => c.title.toLowerCase().includes(q));
  }, [items, query]);

  const handleNew = useCallback(async () => {
    setLoading(true);
    try {
      // Persist current thread before clearing so it shows up in the rail
      await saveCurrentConversation().catch(() => {});
      await clearConversation();
      onNew();
      refresh();
    } finally {
      setLoading(false);
    }
  }, [onNew, refresh]);

  // File > New Chat in the native menu bar routes here - the rail owns the
  // save-then-clear sequence and its own refresh.
  useEffect(() => {
    const un = onMenuAction((action) => { if (action === 'new-chat') handleNew(); });
    return () => { un.then(fn => fn()); };
  }, [handleNew]);

  const handleSelect = useCallback(
    async (id: string) => {
      if (id === activeId) return;
      setLoading(true);
      try {
        // Save what's currently in memory before swapping
        await saveCurrentConversation().catch(() => {});
        const messages = await loadConversation(id);
        onSelect(id, messages);
        refresh();
      } catch {
        /* ignore */
      } finally {
        setLoading(false);
      }
    },
    [activeId, onSelect, refresh],
  );

  const handleDelete = useCallback(
    async (e: React.MouseEvent, id: string) => {
      e.stopPropagation();
      if (!confirm('Delete this conversation?')) return;
      try {
        await deleteConversation(id);
        if (id === activeId) {
          await clearConversation().catch(() => {});
          onSelect(null, []);
        }
        refresh();
      } catch {
        /* ignore */
      }
    },
    [activeId, onSelect, refresh],
  );

  const startRename = useCallback((c: ConversationSummary) => {
    setRenamingId(c.id);
    setRenameDraft(c.title);
  }, []);

  const cancelRename = useCallback(() => {
    setRenamingId(null);
    setRenameDraft('');
  }, []);

  const commitRename = useCallback(async () => {
    if (!renamingId) return;
    const draft = renameDraft.trim();
    if (!draft) {
      cancelRename();
      return;
    }
    try {
      await renameConversation(renamingId, draft);
      refresh();
    } catch {
      /* ignore */
    } finally {
      cancelRename();
    }
  }, [renamingId, renameDraft, cancelRename, refresh]);

  const formatWhen = (iso: string): string => {
    try {
      const d = new Date(iso);
      const now = Date.now();
      const ms = now - d.getTime();
      const min = Math.floor(ms / 60_000);
      if (min < 1) return 'now';
      if (min < 60) return `${min}m`;
      const hr = Math.floor(min / 60);
      if (hr < 24) return `${hr}h`;
      const day = Math.floor(hr / 24);
      if (day < 7) return `${day}d`;
      return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
    } catch {
      return '';
    }
  };

  if (collapsed) {
    return (
      <aside className="conv-rail collapsed" aria-label="Conversations">
        <button
          className="conv-rail-expand"
          title="Show conversations" aria-label="Show conversations"
          onClick={onToggleCollapsed}
        >
          ☰
        </button>
        <button className="conv-rail-newmini" title="New conversation" aria-label="New conversation" onClick={handleNew}>
          ＋
        </button>
      </aside>
    );
  }

  return (
    <aside className="conv-rail" aria-label="Conversations">
      <div className="conv-rail-head">
        <button className="conv-rail-new" onClick={handleNew} disabled={loading}>
          <span className="conv-rail-new-icon">＋</span>
          <span>New chat</span>
        </button>
        {onToggleCollapsed && (
          <button
            className="conv-rail-collapse"
            title="Hide conversations" aria-label="Hide conversations"
            onClick={onToggleCollapsed}
          >
            ‹
          </button>
        )}
      </div>

      <div className="conv-rail-search">
        <input
          type="text"
          placeholder="Search conversations…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      <div className="conv-rail-list">
        {filtered.length === 0 && (
          <div className="conv-rail-empty">
            {query ? 'No matches' : 'No conversations yet'}
          </div>
        )}
        {filtered.map((c) => {
          const isRenaming = renamingId === c.id;
          return (
            <div
              key={c.id}
              className={`conv-rail-item ${c.id === activeId ? 'active' : ''}`}
              onClick={() => !isRenaming && handleSelect(c.id)}
              onDoubleClick={(e) => {
                e.stopPropagation();
                startRename(c);
              }}
              title={isRenaming ? 'Renaming…' : `${c.title} - double-click to rename`}
              role="button"
              tabIndex={0}
            >
              {isRenaming ? (
                <input
                  className="conv-rail-rename"
                  value={renameDraft}
                  autoFocus
                  onChange={(e) => setRenameDraft(e.target.value)}
                  onClick={(e) => e.stopPropagation()}
                  onBlur={commitRename}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      commitRename();
                    } else if (e.key === 'Escape') {
                      e.preventDefault();
                      cancelRename();
                    }
                  }}
                />
              ) : (
                <div className="conv-rail-item-title">{c.title || 'Untitled'}</div>
              )}
              <div className="conv-rail-item-meta">
                <span>{formatWhen(c.last_message_at)}</span>
                <span className="conv-rail-item-count">{c.message_count}</span>
              </div>
              <span
                className="conv-rail-item-del"
                onClick={(e) => handleDelete(e, c.id)}
                role="button"
                aria-label="Delete conversation"
                title="Delete"
              >
                ×
              </span>
            </div>
          );
        })}
      </div>
    </aside>
  );
}
