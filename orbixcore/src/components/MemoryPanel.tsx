import { useState, useEffect } from 'react';
import type { MemoryEntry, MemoryCategory, ConversationSummary, Message } from '../types';
import {
  listMemories, addMemory, removeMemory, updateMemory,
  listConversations, loadConversation, deleteConversation,
  clearAllMemories, saveCompanyContext, getCompanyContext,
  exportConversation
} from '../hooks/api';
import './Panel.css';
import './MemoryPanel.css';

interface Props {
  onClose: () => void;
  onLoadConversation: (messages: Message[]) => void;
}

type Tab = 'memories' | 'history';

const CATEGORY_LABELS: Record<MemoryCategory, string> = {
  fact: 'Fact',
  preference: 'Preference',
  context: 'Context',
  skill: 'Skill',
};

const CATEGORY_COLORS: Record<MemoryCategory, string> = {
  fact: '#6c8fff',
  preference: '#a855f7',
  context: '#34d399',
  skill: '#fbbf24',
};

export default function MemoryPanel({ onClose, onLoadConversation }: Props) {
  const [tab, setTab] = useState<Tab>('memories');
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('');
  const [newCategory, setNewCategory] = useState<MemoryCategory>('fact');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  const [showCompanyCtx, setShowCompanyCtx] = useState(false);
  const [companyCtx, setCompanyCtx] = useState<Record<string, string>>({
    name: '', products: '', competencies: '', target_market: '', ideal_customer_profile: '',
  });
  const [companySaved, setCompanySaved] = useState(false);

  useEffect(() => {
    if (tab === 'memories') {
      listMemories().then(setMemories).catch(() => {});
      getCompanyContext().then(ctx => {
        if (Object.keys(ctx).length > 0) {
          setCompanyCtx(prev => ({ ...prev, ...ctx }));
          setCompanySaved(true);
        }
      }).catch(() => {});
    } else {
      listConversations().then(setConversations).catch(() => {});
    }
  }, [tab]);

  const handleAdd = async () => {
    if (!newKey.trim() || !newValue.trim()) return;
    try {
      const entry = await addMemory(newKey.trim(), newValue.trim(), newCategory);
      setMemories(prev => [...prev, entry]);
      setNewKey('');
      setNewValue('');
      setShowAddForm(false);
    } catch {}
  };

  const handleRemove = async (id: string) => {
    try {
      await removeMemory(id);
      setMemories(prev => prev.filter(m => m.id !== id));
    } catch {}
  };

  const handleUpdate = async (id: string) => {
    if (!editValue.trim()) return;
    try {
      const updated = await updateMemory(id, editValue.trim());
      setMemories(prev => prev.map(m => m.id === id ? updated : m));
      setEditingId(null);
    } catch {}
  };

  const handleSaveCompanyCtx = async () => {
    try {
      await saveCompanyContext(companyCtx);
      setCompanySaved(true);
      // Refresh memories list since new entries were added
      listMemories().then(setMemories).catch(() => {});
    } catch {}
  };

  const handleClearAll = async () => {
    try {
      await clearAllMemories();
      setMemories([]);
    } catch {}
  };

  const handleLoadConv = async (id: string) => {
    try {
      const messages = await loadConversation(id);
      onLoadConversation(messages);
    } catch {}
  };

  const handleDeleteConv = async (id: string) => {
    try {
      await deleteConversation(id);
      setConversations(prev => prev.filter(c => c.id !== id));
    } catch {}
  };

  const formatDate = (iso: string) => {
    try {
      const d = new Date(iso);
      const now = new Date();
      const diff = now.getTime() - d.getTime();
      if (diff < 60000) return 'Just now';
      if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
      if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
      return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
    } catch { return ''; }
  };

  return (
    <div className="panel memory-panel">
      <div className="panel-header">
        <h2 className="panel-title">Memory</h2>
        <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <div className="memory-tabs">
        <button
          className={`memory-tab ${tab === 'memories' ? 'active' : ''}`}
          onClick={() => setTab('memories')}
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <path d="M12 2L2 7l10 5 10-5-10-5z"/>
            <path d="M2 17l10 5 10-5"/>
            <path d="M2 12l10 5 10-5"/>
          </svg>
          Memories
        </button>
        <button
          className={`memory-tab ${tab === 'history' ? 'active' : ''}`}
          onClick={() => setTab('history')}
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <circle cx="12" cy="12" r="10"/>
            <polyline points="12 6 12 12 16 14"/>
          </svg>
          History
        </button>
      </div>

      <div className="panel-body">
        {tab === 'memories' && (
          <>
            {/* Company Context Card */}
            <div className="company-ctx-card">
              <div className="company-ctx-header" onClick={() => setShowCompanyCtx(!showCompanyCtx)}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                  <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
                  <polyline points="9 22 9 12 15 12 15 22"/>
                </svg>
                <span>Company Context</span>
                {companySaved && <span className="ctx-saved-badge">Saved</span>}
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
                  style={{ marginLeft: 'auto', transform: showCompanyCtx ? 'rotate(180deg)' : 'none', transition: 'transform 0.2s' }}>
                  <polyline points="6 9 12 15 18 9"/>
                </svg>
              </div>
              {showCompanyCtx && (
                <div className="company-ctx-body">
                  <p className="company-ctx-hint">
                    Tell your agent about your company so workflows like Lead Scout can run with real context.
                  </p>
                  {[
                    { key: 'name', label: 'Company Name', placeholder: 'e.g. Acme Corp' },
                    { key: 'products', label: 'Products / Services', placeholder: 'e.g. CRM software, consulting services' },
                    { key: 'competencies', label: 'Core Competencies', placeholder: 'e.g. AI automation, data analytics' },
                    { key: 'target_market', label: 'Target Market', placeholder: 'e.g. SMBs in Scandinavia, B2B SaaS' },
                    { key: 'ideal_customer_profile', label: 'Ideal Customer Profile', placeholder: 'e.g. 50-500 employees, tech industry, decision maker is CTO' },
                  ].map(f => (
                    <div key={f.key} className="company-ctx-field">
                      <label>{f.label}</label>
                      <input
                        className="form-input"
                        placeholder={f.placeholder}
                        value={companyCtx[f.key] || ''}
                        onChange={e => setCompanyCtx(prev => ({ ...prev, [f.key]: e.target.value }))}
                      />
                    </div>
                  ))}
                  <button className="memory-save-btn" onClick={handleSaveCompanyCtx}>
                    {companySaved ? 'Update Context' : 'Save Context'}
                  </button>
                </div>
              )}
            </div>

            <div className="memory-actions-row">
              <button
                className="memory-add-btn"
                onClick={() => setShowAddForm(!showAddForm)}
              >
                {showAddForm ? 'Cancel' : '+ Add Memory'}
              </button>
              {memories.length > 0 && (
                <button className="memory-clear-btn" onClick={handleClearAll}>
                  Clear All
                </button>
              )}
            </div>

            {showAddForm && (
              <div className="memory-add-form">
                <input
                  className="form-input"
                  placeholder="Key (e.g. 'name', 'preferred language')"
                  value={newKey}
                  onChange={e => setNewKey(e.target.value)}
                />
                <input
                  className="form-input"
                  placeholder="Value"
                  value={newValue}
                  onChange={e => setNewValue(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && handleAdd()}
                />
                <div className="memory-category-row">
                  {(Object.keys(CATEGORY_LABELS) as MemoryCategory[]).map(cat => (
                    <button
                      key={cat}
                      className={`memory-cat-btn ${newCategory === cat ? 'active' : ''}`}
                      style={{ '--cat-color': CATEGORY_COLORS[cat] } as React.CSSProperties}
                      onClick={() => setNewCategory(cat)}
                    >
                      {CATEGORY_LABELS[cat]}
                    </button>
                  ))}
                </div>
                <button className="memory-save-btn" onClick={handleAdd}>Save</button>
              </div>
            )}

            {memories.length === 0 && !showAddForm && (
              <div className="memory-empty">
                <p>No memories yet.</p>
                <p className="memory-empty-sub">
                  Memories help Orbix Core remember your preferences, context, and important facts across conversations.
                </p>
              </div>
            )}

            <div className="memory-list">
              {memories.map(m => (
                <div key={m.id} className="memory-card">
                  <div className="memory-card-header">
                    <span
                      className="memory-cat-dot"
                      style={{ background: CATEGORY_COLORS[m.category] }}
                      title={CATEGORY_LABELS[m.category]}
                    />
                    <span className="memory-key">{m.key}</span>
                    <span className="memory-time">{formatDate(m.updated_at)}</span>
                    <button
                      className="memory-action-btn"
                      onClick={() => { setEditingId(m.id); setEditValue(m.value); }}
                      title="Edit"
                    >
                      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                        <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                      </svg>
                    </button>
                    <button
                      className="memory-action-btn danger"
                      onClick={() => handleRemove(m.id)}
                      title="Delete"
                    >
                      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <line x1="18" y1="6" x2="6" y2="18"/>
                        <line x1="6" y1="6" x2="18" y2="18"/>
                      </svg>
                    </button>
                  </div>
                  {editingId === m.id ? (
                    <div className="memory-edit-row">
                      <input
                        className="form-input"
                        value={editValue}
                        onChange={e => setEditValue(e.target.value)}
                        onKeyDown={e => e.key === 'Enter' && handleUpdate(m.id)}
                        autoFocus
                      />
                      <button className="memory-save-btn small" onClick={() => handleUpdate(m.id)}>Save</button>
                    </div>
                  ) : (
                    <p className="memory-value">{m.value}</p>
                  )}
                </div>
              ))}
            </div>
          </>
        )}

        {tab === 'history' && (
          <>
            <button
              className="conv-export-current"
              onClick={() => exportConversation()}
              title="Export current conversation as PDF with sensitive data masked"
            >
              Export current conversation
            </button>

            {conversations.length === 0 && (
              <div className="memory-empty">
                <p>No saved conversations.</p>
                <p className="memory-empty-sub">
                  Conversations are auto-saved after each reply.
                </p>
              </div>
            )}

            <div className="memory-list">
              {conversations.map(c => (
                <div key={c.id} className="memory-card conv-card" onClick={() => handleLoadConv(c.id)}>
                  <div className="memory-card-header">
                    <span className="conv-title">{c.title}</span>
                    <div className="conv-actions">
                      <button
                        className="memory-action-btn export"
                        onClick={e => { e.stopPropagation(); exportConversation(c.id); }}
                        title="Export as PDF"
                      >
                        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                          <polyline points="14 2 14 8 20 8"/>
                          <line x1="12" y1="18" x2="12" y2="12"/>
                          <polyline points="9 15 12 18 15 15"/>
                        </svg>
                      </button>
                      <button
                        className="memory-action-btn danger"
                        onClick={e => { e.stopPropagation(); handleDeleteConv(c.id); }}
                        title="Delete"
                      >
                        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <line x1="18" y1="6" x2="6" y2="18"/>
                          <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                      </button>
                    </div>
                  </div>
                  <div className="conv-meta">
                    <span>{c.message_count} messages</span>
                    <span>{formatDate(c.last_message_at)}</span>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
