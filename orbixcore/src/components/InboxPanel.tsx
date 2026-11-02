import { useState, useEffect, useCallback } from 'react';
import type { ProcessPipeline, PipelineItem, PipelineStage, ItemSource } from '../types';
import {
  listPipelines, listInboxItems, listPipelineItems,
  createInboxItem, movePipelineItem, assignItemToPipeline,
  updateItemStatus, bulkUpdateItems, deleteInboxItem,
  summarizeItem,
} from '../hooks/api';
import './InboxPanel.css';

type View = 'inbox' | 'pipeline';

interface Props {
  onClose: () => void;
}

export default function InboxPanel({ onClose }: Props) {
  const [view, setView] = useState<View>('inbox');
  const [pipelines, setPipelines] = useState<ProcessPipeline[]>([]);
  const [activePipelineId, setActivePipelineId] = useState<string | null>(null);
  const [inboxItems, setInboxItems] = useState<PipelineItem[]>([]);
  const [pipelineItems, setPipelineItems] = useState<PipelineItem[]>([]);
  const [selectedItems, setSelectedItems] = useState<Set<string>>(new Set());
  const [sourceFilter, setSourceFilter] = useState<ItemSource | ''>('');
  const [showAddForm, setShowAddForm] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [pls, items] = await Promise.all([
        listPipelines(),
        listInboxItems(sourceFilter || undefined),
      ]);
      setPipelines(pls);
      setInboxItems(items);
      if (pls.length > 0 && !activePipelineId) {
        setActivePipelineId(pls[0].id);
      }
    } catch (e) { console.error(e); }
  }, [sourceFilter, activePipelineId]);

  useEffect(() => { refresh(); }, [refresh]);

  // Load pipeline items when viewing a pipeline
  useEffect(() => {
    if (view === 'pipeline' && activePipelineId) {
      listPipelineItems(activePipelineId).then(setPipelineItems).catch(() => {});
    }
  }, [view, activePipelineId]);

  const activePipeline = pipelines.find(p => p.id === activePipelineId);

  const toggleSelect = (id: string) => {
    setSelectedItems(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleBulkAction = async (action: string) => {
    if (selectedItems.size === 0) return;
    const ids = Array.from(selectedItems);
    await bulkUpdateItems(ids, action, activePipelineId ?? undefined);
    setSelectedItems(new Set());
    refresh();
  };

  const handleAssign = async (itemId: string, pipelineId: string) => {
    await assignItemToPipeline(itemId, pipelineId);
    refresh();
    if (view === 'pipeline' && activePipelineId) {
      listPipelineItems(activePipelineId).then(setPipelineItems);
    }
  };

  const handleMoveStage = async (itemId: string, stageId: string) => {
    await movePipelineItem(itemId, stageId);
    if (activePipelineId) {
      listPipelineItems(activePipelineId).then(setPipelineItems);
    }
  };

  const handleDelete = async (itemId: string) => {
    await deleteInboxItem(itemId);
    refresh();
  };

  const handleMarkRead = async (itemId: string) => {
    await updateItemStatus(itemId, 'read');
    refresh();
  };

  const handleArchive = async (itemId: string) => {
    await updateItemStatus(itemId, 'archived');
    refresh();
  };

  const sourceIcon = (source: ItemSource) => {
    switch (source) {
      case 'email': return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <rect x="2" y="4" width="20" height="16" rx="2"/><path d="M22 7l-10 7L2 7"/>
        </svg>
      );
      case 'webhook': return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
        </svg>
      );
      case 'file_watch': return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        </svg>
      );
      default: return (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>
        </svg>
      );
    }
  };

  const timeAgo = (ts: string) => {
    const diff = Date.now() - new Date(ts).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'now';
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h`;
    const days = Math.floor(hrs / 24);
    return `${days}d`;
  };

  return (
    <div className="panel-overlay animate-slide-right">
      <div className="inbox-header">
        <div className="inbox-title-row">
          <h2>Inbox</h2>
          <button className="panel-close-btn" onClick={onClose}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>
        <div className="inbox-view-toggle">
          <button className={view === 'inbox' ? 'active' : ''} onClick={() => setView('inbox')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <path d="M22 12h-6l-2 3h-4l-2-3H2"/>
              <path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>
            </svg>
            Inbox
          </button>
          <button className={view === 'pipeline' ? 'active' : ''} onClick={() => setView('pipeline')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/>
              <rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>
            </svg>
            Pipeline
          </button>
        </div>
      </div>

      <div className="inbox-body">
        {view === 'inbox' ? (
          <InboxView
            items={inboxItems}
            pipelines={pipelines}
            selectedItems={selectedItems}
            sourceFilter={sourceFilter}
            showAddForm={showAddForm}
            onSetSourceFilter={setSourceFilter}
            onSetShowAddForm={setShowAddForm}
            onToggleSelect={toggleSelect}
            onBulkAction={handleBulkAction}
            onAssign={handleAssign}
            onMarkRead={handleMarkRead}
            onArchive={handleArchive}
            onDelete={handleDelete}
            onRefresh={refresh}
            sourceIcon={sourceIcon}
            timeAgo={timeAgo}
          />
        ) : (
          <PipelineView
            pipeline={activePipeline}
            pipelines={pipelines}
            items={pipelineItems}
            onSelectPipeline={setActivePipelineId}
            onMoveStage={handleMoveStage}
            onDelete={handleDelete}
            sourceIcon={sourceIcon}
            timeAgo={timeAgo}
          />
        )}
      </div>
    </div>
  );
}

// ── Inbox View ───────────────────────────────

function InboxView({
  items, pipelines, selectedItems, sourceFilter, showAddForm,
  onSetSourceFilter, onSetShowAddForm, onToggleSelect, onBulkAction,
  onAssign, onMarkRead, onArchive, onDelete, onRefresh, sourceIcon, timeAgo,
}: {
  items: PipelineItem[];
  pipelines: ProcessPipeline[];
  selectedItems: Set<string>;
  sourceFilter: ItemSource | '';
  showAddForm: boolean;
  onSetSourceFilter: (f: ItemSource | '') => void;
  onSetShowAddForm: (v: boolean) => void;
  onToggleSelect: (id: string) => void;
  onBulkAction: (action: string) => void;
  onAssign: (itemId: string, pipelineId: string) => void;
  onMarkRead: (id: string) => void;
  onArchive: (id: string) => void;
  onDelete: (id: string) => void;
  onRefresh: () => void;
  sourceIcon: (s: ItemSource) => React.ReactNode;
  timeAgo: (ts: string) => string;
}) {
  const [newTitle, setNewTitle] = useState('');
  const [newSnippet, setNewSnippet] = useState('');

  const handleAdd = async () => {
    if (!newTitle.trim()) return;
    await createInboxItem(newTitle, newSnippet, 'manual');
    setNewTitle(''); setNewSnippet('');
    onSetShowAddForm(false);
    onRefresh();
  };

  return (
    <div className="inbox-view">
      {/* Toolbar */}
      <div className="inbox-toolbar">
        <div className="inbox-filters">
          <select
            className="inbox-filter-select"
            value={sourceFilter}
            onChange={e => onSetSourceFilter(e.target.value as ItemSource | '')}
          >
            <option value="">All sources</option>
            <option value="email">Email</option>
            <option value="webhook">Webhook</option>
            <option value="file_watch">File Watch</option>
            <option value="manual">Manual</option>
            <option value="workflow">Workflow</option>
          </select>
        </div>
        <div className="inbox-actions-bar">
          {selectedItems.size > 0 && (
            <>
              <span className="inbox-selected-count">{selectedItems.size} selected</span>
              <button className="inbox-action-btn" onClick={() => onBulkAction('read')}>Mark Read</button>
              <button className="inbox-action-btn" onClick={() => onBulkAction('archive')}>Archive</button>
            </>
          )}
          <button className="inbox-add-btn" onClick={() => onSetShowAddForm(!showAddForm)}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
          </button>
        </div>
      </div>

      {/* Add form */}
      {showAddForm && (
        <div className="inbox-add-form">
          <input className="inbox-input" placeholder="Title..." value={newTitle} onChange={e => setNewTitle(e.target.value)} />
          <input className="inbox-input" placeholder="Description..." value={newSnippet} onChange={e => setNewSnippet(e.target.value)} />
          <div className="inbox-form-actions">
            <button className="inbox-btn secondary" onClick={() => onSetShowAddForm(false)}>Cancel</button>
            <button className="inbox-btn primary" onClick={handleAdd}>Add</button>
          </div>
        </div>
      )}

      {/* Items */}
      {items.length === 0 && (
        <div className="inbox-empty">
          <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" opacity="0.2">
            <path d="M22 12h-6l-2 3h-4l-2-3H2"/>
            <path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>
          </svg>
          <p>Your inbox is empty. Events from triggers will appear here.</p>
        </div>
      )}

      {items.map(item => (
        <div
          key={item.id}
          className={`inbox-item ${item.status === 'unread' ? 'unread' : ''} ${selectedItems.has(item.id) ? 'selected' : ''}`}
        >
          <button className="inbox-checkbox" onClick={() => onToggleSelect(item.id)}>
            {selectedItems.has(item.id) ? (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="var(--accent)" stroke="white" strokeWidth="2">
                <rect x="3" y="3" width="18" height="18" rx="4"/><path d="M9 12l2 2 4-4"/>
              </svg>
            ) : (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                <rect x="3" y="3" width="18" height="18" rx="4"/>
              </svg>
            )}
          </button>
          <div className="inbox-item-source">{sourceIcon(item.source)}</div>
          <div className="inbox-item-content" onClick={() => onMarkRead(item.id)}>
            <div className="inbox-item-title">{item.title}</div>
            <div className="inbox-item-snippet">{item.snippet}</div>
            {item.ai_summary && (
              <div className="inbox-item-summary">{item.ai_summary}</div>
            )}
          </div>
          <div className="inbox-item-meta">
            <span className="inbox-item-time">{timeAgo(item.created_at)}</span>
            {item.pipeline_id && item.stage_id && (
              <span className="inbox-item-stage">
                {pipelines.find(p => p.id === item.pipeline_id)
                  ?.stages.find(s => s.id === item.stage_id)?.name}
              </span>
            )}
          </div>
          <div className="inbox-item-actions">
            {!item.pipeline_id && pipelines.length > 0 && (
              <select
                className="inbox-assign-select"
                value=""
                onChange={e => { if (e.target.value) onAssign(item.id, e.target.value); }}
              >
                <option value="">Assign...</option>
                {pipelines.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            )}
            <button className="inbox-icon-btn" onClick={() => onArchive(item.id)} title="Archive">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/>
                <line x1="10" y1="12" x2="14" y2="12"/>
              </svg>
            </button>
            <button className="inbox-icon-btn danger" onClick={() => onDelete(item.id)} title="Delete">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12"/>
              </svg>
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Pipeline (Kanban) View ───────────────────

function PipelineView({
  pipeline, pipelines, items, onSelectPipeline, onMoveStage, onDelete, sourceIcon, timeAgo,
}: {
  pipeline: ProcessPipeline | undefined;
  pipelines: ProcessPipeline[];
  items: PipelineItem[];
  onSelectPipeline: (id: string) => void;
  onMoveStage: (itemId: string, stageId: string) => void;
  onDelete: (id: string) => void;
  sourceIcon: (s: ItemSource) => React.ReactNode;
  timeAgo: (ts: string) => string;
}) {
  const [summarizing, setSummarizing] = useState<string | null>(null);
  const [dragItem, setDragItem] = useState<string | null>(null);

  const handleSummarize = async (itemId: string) => {
    setSummarizing(itemId);
    try { await summarizeItem(itemId); } catch (e) { console.error(e); }
    setSummarizing(null);
  };

  if (!pipeline) {
    return (
      <div className="inbox-empty">
        <p>No pipelines configured for your role.</p>
      </div>
    );
  }

  const stageItems = (stage: PipelineStage) =>
    items.filter(i => i.stage_id === stage.id);

  return (
    <div className="pipeline-view">
      {/* Pipeline selector */}
      {pipelines.length > 1 && (
        <div className="pipeline-selector">
          {pipelines.map(p => (
            <button
              key={p.id}
              className={`pipeline-tab ${p.id === pipeline.id ? 'active' : ''}`}
              onClick={() => onSelectPipeline(p.id)}
            >
              {p.name}
            </button>
          ))}
        </div>
      )}

      {/* Kanban columns */}
      <div className="kanban-board">
        {pipeline.stages.map(stage => (
          <div
            key={stage.id}
            className="kanban-column"
            onDragOver={e => e.preventDefault()}
            onDrop={() => {
              if (dragItem) {
                onMoveStage(dragItem, stage.id);
                setDragItem(null);
              }
            }}
          >
            <div className="kanban-column-header">
              <span className="kanban-stage-dot" style={{ background: stage.color }}/>
              <span className="kanban-stage-name">{stage.name}</span>
              <span className="kanban-stage-count">{stageItems(stage).length}</span>
            </div>
            <div className="kanban-column-items">
              {stageItems(stage).map(item => (
                <div
                  key={item.id}
                  className="kanban-card"
                  draggable
                  onDragStart={() => setDragItem(item.id)}
                  onDragEnd={() => setDragItem(null)}
                >
                  <div className="kanban-card-header">
                    <span className="kanban-card-source">{sourceIcon(item.source)}</span>
                    <span className="kanban-card-time">{timeAgo(item.created_at)}</span>
                  </div>
                  <div className="kanban-card-title">{item.title}</div>
                  <div className="kanban-card-snippet">{item.snippet}</div>
                  {item.ai_summary && (
                    <div className="kanban-card-summary">{item.ai_summary}</div>
                  )}
                  <div className="kanban-card-footer">
                    <button
                      className="kanban-card-btn"
                      onClick={() => handleSummarize(item.id)}
                      disabled={summarizing === item.id}
                    >
                      {summarizing === item.id ? 'Summarizing...' : 'AI Summary'}
                    </button>
                    {/* Move to next stage button */}
                    {(() => {
                      const currentIdx = pipeline.stages.findIndex(s => s.id === stage.id);
                      const nextStage = pipeline.stages[currentIdx + 1];
                      if (!nextStage) return null;
                      return (
                        <button
                          className="kanban-card-btn advance"
                          onClick={() => onMoveStage(item.id, nextStage.id)}
                          title={`Move to ${nextStage.name}`}
                        >
                          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <path d="M5 12h14M12 5l7 7-7 7"/>
                          </svg>
                        </button>
                      );
                    })()}
                    <button className="kanban-card-btn danger" onClick={() => onDelete(item.id)}>
                      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <path d="M18 6L6 18M6 6l12 12"/>
                      </svg>
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
