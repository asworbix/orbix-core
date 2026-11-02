import { useState, useEffect } from 'react';
import type { KBArticle, KBSearchResult } from '../types';
import {
  listKBArticles, createKBArticle, updateKBArticle, deleteKBArticle,
  searchKB, listKBCategories,
} from '../hooks/api';
import './KnowledgePanel.css';

interface Props { onClose: () => void }

export default function KnowledgePanel({ onClose }: Props) {
  const [tab, setTab] = useState<'browse' | 'search' | 'create'>('browse');
  const [articles, setArticles] = useState<KBArticle[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [catFilter, setCatFilter] = useState('');
  const [selected, setSelected] = useState<KBArticle | null>(null);
  const [editing, setEditing] = useState(false);

  // Search state
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<KBSearchResult[]>([]);

  // Create/edit form
  const [formTitle, setFormTitle] = useState('');
  const [formContent, setFormContent] = useState('');
  const [formCategory, setFormCategory] = useState('General');
  const [formTags, setFormTags] = useState('');

  useEffect(() => {
    listKBArticles(catFilter || undefined).then(setArticles).catch(() => {});
    listKBCategories().then(setCategories).catch(() => {});
  }, [catFilter]);

  const handleSearch = async () => {
    if (!query.trim()) return;
    const r = await searchKB(query).catch(() => []);
    setResults(r);
  };

  const handleCreate = async () => {
    if (!formTitle.trim()) return;
    const tags = formTags.split(',').map(t => t.trim()).filter(Boolean);
    const article = await createKBArticle(formTitle, formContent, formCategory, tags).catch(() => null);
    if (article) {
      setArticles(prev => [article, ...prev]);
      setFormTitle('');
      setFormContent('');
      setFormTags('');
      setTab('browse');
    }
  };

  const handleUpdate = async () => {
    if (!selected) return;
    const tags = formTags.split(',').map(t => t.trim()).filter(Boolean);
    const updated = await updateKBArticle(selected.id, formTitle, formContent, formCategory, tags).catch(() => null);
    if (updated) {
      setArticles(prev => prev.map(a => a.id === updated.id ? updated : a));
      setSelected(updated);
      setEditing(false);
    }
  };

  const handleDelete = async (id: string) => {
    await deleteKBArticle(id).catch(() => {});
    setArticles(prev => prev.filter(a => a.id !== id));
    if (selected?.id === id) setSelected(null);
  };

  const openArticle = (article: KBArticle) => {
    setSelected(article);
    setEditing(false);
  };

  const startEdit = () => {
    if (!selected) return;
    setFormTitle(selected.title);
    setFormContent(selected.content);
    setFormCategory(selected.category);
    setFormTags(selected.tags.join(', '));
    setEditing(true);
  };

  const formatDate = (iso: string) =>
    new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });

  // Article detail view
  if (selected && !editing) {
    return (
      <div className="panel">
        <div className="panel-header">
          <h2 className="panel-title">
            <button className="kb-back-btn" onClick={() => setSelected(null)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="15 18 9 12 15 6"/>
              </svg>
            </button>
            {selected.title}
          </h2>
          <div className="panel-header-actions">
            <button className="link-btn" onClick={startEdit}>Edit</button>
            <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        </div>
        <div className="panel-body">
          <div className="kb-article-meta">
            <span className="kb-cat-badge">{selected.category}</span>
            {selected.tags.map(t => <span key={t} className="kb-tag">{t}</span>)}
            <span className="kb-meta-date">{formatDate(selected.updated_at)}</span>
            <span className="kb-meta-words">{selected.word_count} words</span>
          </div>
          <div className="kb-article-content">{selected.content}</div>
        </div>
      </div>
    );
  }

  // Edit view
  if (selected && editing) {
    return (
      <div className="panel">
        <div className="panel-header">
          <h2 className="panel-title">Edit Article</h2>
          <div className="panel-header-actions">
            <button className="link-btn" onClick={() => setEditing(false)}>Cancel</button>
            <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        </div>
        <div className="panel-body">
          <div className="kb-form">
            <input
              className="panel-input"
              placeholder="Title"
              value={formTitle}
              onChange={e => setFormTitle(e.target.value)}
            />
            <input
              className="panel-input"
              placeholder="Category"
              value={formCategory}
              onChange={e => setFormCategory(e.target.value)}
            />
            <input
              className="panel-input"
              placeholder="Tags (comma separated)"
              value={formTags}
              onChange={e => setFormTags(e.target.value)}
            />
            <textarea
              className="panel-textarea"
              placeholder="Content (markdown supported)"
              value={formContent}
              onChange={e => setFormContent(e.target.value)}
              rows={12}
            />
            <div className="kb-form-actions">
              <button className="panel-btn accent" onClick={handleUpdate}>Save Changes</button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Main list/search/create view
  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/>
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>
          </svg>
          Knowledge Base
          {articles.length > 0 && <span className="sugg-badge">{articles.length}</span>}
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="kb-tabs">
        <button className={tab === 'browse' ? 'active' : ''} onClick={() => setTab('browse')}>
          Browse
        </button>
        <button className={tab === 'search' ? 'active' : ''} onClick={() => setTab('search')}>
          Search
        </button>
        <button className={tab === 'create' ? 'active' : ''} onClick={() => setTab('create')}>
          + New
        </button>
      </div>

      <div className="panel-body">
        {tab === 'browse' && (
          <>
            {/* Category filter */}
            {categories.length > 0 && (
              <div className="kb-cat-filter">
                <button
                  className={!catFilter ? 'active' : ''}
                  onClick={() => setCatFilter('')}
                >
                  All
                </button>
                {categories.map(c => (
                  <button
                    key={c}
                    className={catFilter === c ? 'active' : ''}
                    onClick={() => setCatFilter(c)}
                  >
                    {c}
                  </button>
                ))}
              </div>
            )}

            {articles.length === 0 ? (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                    <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/>
                    <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>
                  </svg>
                </div>
                <p>No articles yet</p>
                <p className="panel-empty-sub">Add documents, FAQs, and SOPs for the AI to reference.</p>
              </div>
            ) : (
              articles.map(a => (
                <div key={a.id} className="kb-item" onClick={() => openArticle(a)}>
                  <div className="kb-item-top">
                    <span className="kb-item-title">{a.title}</span>
                    <button
                      className="kb-delete-btn"
                      onClick={e => { e.stopPropagation(); handleDelete(a.id); }}
                      title="Delete"
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                      </svg>
                    </button>
                  </div>
                  <div className="kb-item-meta">
                    <span className="kb-cat-badge">{a.category}</span>
                    <span className="kb-meta-words">{a.word_count} words</span>
                    <span className="kb-meta-date">{formatDate(a.updated_at)}</span>
                  </div>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'search' && (
          <>
            <div className="kb-search-bar">
              <input
                className="panel-input"
                placeholder="Search knowledge base..."
                value={query}
                onChange={e => setQuery(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSearch()}
              />
              <button className="panel-btn accent" onClick={handleSearch}>Search</button>
            </div>

            {results.length === 0 ? (
              <div className="panel-empty">
                <p>{query ? 'No results found' : 'Enter a search query'}</p>
              </div>
            ) : (
              results.map(r => (
                <div
                  key={r.article_id}
                  className="kb-search-result"
                  onClick={() => {
                    const article = articles.find(a => a.id === r.article_id);
                    if (article) openArticle(article);
                  }}
                >
                  <div className="kb-result-title">{r.title}</div>
                  <div className="kb-result-snippet">{r.snippet}</div>
                  <div className="kb-result-meta">
                    <span className="kb-cat-badge">{r.category}</span>
                    <span className="kb-result-score">{(r.score * 100).toFixed(0)}% match</span>
                  </div>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'create' && (
          <div className="kb-form">
            <input
              className="panel-input"
              placeholder="Title"
              value={formTitle}
              onChange={e => setFormTitle(e.target.value)}
            />
            <input
              className="panel-input"
              placeholder="Category"
              value={formCategory}
              onChange={e => setFormCategory(e.target.value)}
            />
            <input
              className="panel-input"
              placeholder="Tags (comma separated)"
              value={formTags}
              onChange={e => setFormTags(e.target.value)}
            />
            <textarea
              className="panel-textarea"
              placeholder="Content (markdown supported)"
              value={formContent}
              onChange={e => setFormContent(e.target.value)}
              rows={12}
            />
            <div className="kb-form-actions">
              <button className="panel-btn accent" onClick={handleCreate} disabled={!formTitle.trim()}>
                Create Article
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
