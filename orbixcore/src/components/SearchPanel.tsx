import { useState, useEffect } from 'react';
import type { ConversationSearchResult, Message } from '../types';
import { searchConversations, getSearchStats, loadConversation } from '../hooks/api';
import './SearchPanel.css';

interface Props {
  onClose: () => void;
  onLoadConversation?: (msgs: Message[]) => void;
}

export default function SearchPanel({ onClose, onLoadConversation }: Props) {
  const [query, setQuery] = useState('');
  const [roleFilter, setRoleFilter] = useState('');
  const [results, setResults] = useState<ConversationSearchResult[]>([]);
  const [searched, setSearched] = useState(false);
  const [stats, setStats] = useState<[number, number]>([0, 0]);

  useEffect(() => {
    getSearchStats().then(setStats).catch(() => {});
  }, []);

  const handleSearch = async () => {
    if (!query.trim()) return;
    setSearched(true);
    const r = await searchConversations(
      query,
      roleFilter || undefined,
    ).catch(() => []);
    setResults(r);
  };

  const handleLoadConversation = async (conversationId: string) => {
    if (!onLoadConversation) return;
    try {
      const msgs = await loadConversation(conversationId);
      onLoadConversation(msgs);
      onClose();
    } catch {
      // ignore
    }
  };

  const formatTime = (iso: string) =>
    new Date(iso).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="11" cy="11" r="8"/>
            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          Search Conversations
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body">
        {/* Search bar */}
        <div className="search-bar">
          <div className="search-input-wrap">
            <svg className="search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8"/>
              <line x1="21" y1="21" x2="16.65" y2="16.65"/>
            </svg>
            <input
              className="search-input"
              placeholder="Search all conversations..."
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSearch()}
              autoFocus
            />
          </div>
          <button className="panel-btn accent" onClick={handleSearch}>Search</button>
        </div>

        {/* Filters */}
        <div className="search-filters">
          <button
            className={!roleFilter ? 'active' : ''}
            onClick={() => setRoleFilter('')}
          >
            All
          </button>
          <button
            className={roleFilter === 'user' ? 'active' : ''}
            onClick={() => setRoleFilter('user')}
          >
            User
          </button>
          <button
            className={roleFilter === 'assistant' ? 'active' : ''}
            onClick={() => setRoleFilter('assistant')}
          >
            Assistant
          </button>
        </div>

        {/* Stats */}
        <div className="search-stats">
          {stats[0]} conversations indexed &middot; {stats[1]} terms
        </div>

        {/* Results */}
        {!searched ? (
          <div className="panel-empty">
            <div className="panel-empty-icon">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                <circle cx="11" cy="11" r="8"/>
                <line x1="21" y1="21" x2="16.65" y2="16.65"/>
              </svg>
            </div>
            <p>Search across your chat history</p>
            <p className="panel-empty-sub">Find previous answers, solutions, and context from past conversations.</p>
          </div>
        ) : results.length === 0 ? (
          <div className="panel-empty">
            <p>No results for &quot;{query}&quot;</p>
          </div>
        ) : (
          results.map((r, i) => (
            <div
              key={`${r.message_id}-${i}`}
              className="search-result animate-fade-up"
              onClick={() => handleLoadConversation(r.conversation_id)}
            >
              <div className="search-result-header">
                <span className="search-result-conv">{r.conversation_title}</span>
                <span className="search-result-time">{formatTime(r.timestamp)}</span>
              </div>
              <div className="search-result-role">
                <span className={`search-role-badge ${r.role}`}>{r.role}</span>
              </div>
              {r.content_snippet && (
                <div className="search-result-snippet">{r.content_snippet}</div>
              )}
              <div className="search-result-score">
                relevance: {(r.score * 100).toFixed(0)}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
