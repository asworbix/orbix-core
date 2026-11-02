import { useState, useEffect } from 'react';
import type { Suggestion, SuggestionPreferences, SuggestionCategory } from '../types';
import {
  listSuggestions, getSuggestionHistory, acceptSuggestion,
  dismissSuggestion, dismissAllSuggestions,
  getSuggestionPreferences, updateSuggestionPreferences,
} from '../hooks/api';
import './SuggestionsPanel.css';

const CATEGORY_ICONS: Record<SuggestionCategory, string> = {
  pipeline: 'P',
  workflow: 'W',
  follow_up: 'F',
  optimization: 'O',
  scheduling: 'S',
};

const CATEGORY_LABELS: Record<SuggestionCategory, string> = {
  pipeline: 'Pipeline',
  workflow: 'Workflow',
  follow_up: 'Follow-up',
  optimization: 'Optimization',
  scheduling: 'Scheduling',
};

const PRIORITY_COLORS: Record<string, string> = {
  high: 'var(--error)',
  medium: 'var(--warning)',
  low: 'var(--text-muted)',
};

interface Props { onClose: () => void }

export default function SuggestionsPanel({ onClose }: Props) {
  const [active, setActive] = useState<Suggestion[]>([]);
  const [history, setHistory] = useState<Suggestion[]>([]);
  const [prefs, setPrefs] = useState<SuggestionPreferences | null>(null);
  const [tab, setTab] = useState<'active' | 'history' | 'settings'>('active');

  useEffect(() => {
    listSuggestions().then(setActive).catch(() => {});
    getSuggestionHistory().then(setHistory).catch(() => {});
    getSuggestionPreferences().then(setPrefs).catch(() => {});
  }, []);

  const handleAccept = async (id: string) => {
    const updated = await acceptSuggestion(id).catch(() => null);
    if (updated) {
      setActive(prev => prev.filter(s => s.id !== id));
      setHistory(prev => [updated, ...prev]);
    }
  };

  const handleDismiss = async (id: string) => {
    const updated = await dismissSuggestion(id).catch(() => null);
    if (updated) {
      setActive(prev => prev.filter(s => s.id !== id));
      setHistory(prev => [updated, ...prev]);
    }
  };

  const handleDismissAll = async () => {
    await dismissAllSuggestions().catch(() => {});
    setHistory(prev => [...active.map(s => ({ ...s, status: 'dismissed' as const })), ...prev]);
    setActive([]);
  };

  const handleToggleCategory = async (key: keyof SuggestionPreferences['categories']) => {
    if (!prefs) return;
    const updated = {
      ...prefs,
      categories: { ...prefs.categories, [key]: !prefs.categories[key] },
    };
    setPrefs(updated);
    await updateSuggestionPreferences(updated).catch(() => {});
  };

  const handleToggleEnabled = async () => {
    if (!prefs) return;
    const updated = { ...prefs, enabled: !prefs.enabled };
    setPrefs(updated);
    await updateSuggestionPreferences(updated).catch(() => {});
  };

  const formatTime = (iso: string) => {
    return new Date(iso).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 1 1 7.072 0l-.548.547A3.374 3.374 0 0 0 14 18.469V19a2 2 0 1 1-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
          </svg>
          Suggestions
          {active.length > 0 && <span className="sugg-badge">{active.length}</span>}
        </h2>
        <div className="panel-header-actions">
          {tab === 'active' && active.length > 0 && (
            <button className="link-btn" onClick={handleDismissAll}>Dismiss all</button>
          )}
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="sugg-tabs">
        <button className={tab === 'active' ? 'active' : ''} onClick={() => setTab('active')}>
          Active {active.length > 0 && <span className="sugg-tab-count">{active.length}</span>}
        </button>
        <button className={tab === 'history' ? 'active' : ''} onClick={() => setTab('history')}>
          History
        </button>
        <button className={tab === 'settings' ? 'active' : ''} onClick={() => setTab('settings')}>
          Settings
        </button>
      </div>

      <div className="panel-body">
        {/* ── Active ── */}
        {tab === 'active' && (
          <>
            {active.length === 0 ? (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                    <path d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 1 1 7.072 0l-.548.547A3.374 3.374 0 0 0 14 18.469V19a2 2 0 1 1-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
                  </svg>
                </div>
                <p>No suggestions right now</p>
                <p className="panel-empty-sub">Smart suggestions will appear when patterns are detected in your activity.</p>
              </div>
            ) : (
              active.map(s => (
                <div key={s.id} className="sugg-card animate-fade-up">
                  <div className="sugg-card-header">
                    <span className="sugg-category">
                      {CATEGORY_ICONS[s.category]} {CATEGORY_LABELS[s.category]}
                    </span>
                    <span className="sugg-priority" style={{ color: PRIORITY_COLORS[s.priority] }}>
                      {s.priority}
                    </span>
                  </div>
                  <h3 className="sugg-title">{s.title}</h3>
                  <p className="sugg-desc">{s.description}</p>
                  <div className="sugg-actions">
                    <button className="panel-btn accent" onClick={() => handleAccept(s.id)}>
                      {s.action_label}
                    </button>
                    <button className="link-btn" onClick={() => handleDismiss(s.id)}>Dismiss</button>
                  </div>
                </div>
              ))
            )}
          </>
        )}

        {/* ── History ── */}
        {tab === 'history' && (
          <>
            {history.length === 0 ? (
              <div className="panel-empty">
                <p>No suggestion history</p>
              </div>
            ) : (
              history.map(s => (
                <div key={s.id} className="sugg-history-item">
                  <span className="sugg-category-mini">{CATEGORY_ICONS[s.category]}</span>
                  <div className="sugg-history-info">
                    <span className="sugg-history-title">{s.title}</span>
                    <span className="sugg-history-time">{formatTime(s.created_at)}</span>
                  </div>
                  <span className={`sugg-status-badge ${s.status}`}>{s.status}</span>
                </div>
              ))
            )}
          </>
        )}

        {/* ── Settings ── */}
        {tab === 'settings' && prefs && (
          <div className="sugg-settings">
            <div className="sugg-setting-row">
              <div>
                <span className="sugg-setting-label">Enable Suggestions</span>
                <span className="sugg-setting-sub">Proactively recommend actions</span>
              </div>
              <button className={`toggle-switch ${prefs.enabled ? 'on' : ''}`} role="switch" aria-checked={prefs.enabled} onClick={handleToggleEnabled}>
                <div className="toggle-thumb" />
              </button>
            </div>

            <div className="panel-section-header">Categories</div>

            {(Object.keys(prefs.categories) as (keyof typeof prefs.categories)[]).map(key => (
              <div key={key} className="sugg-setting-row">
                <span className="sugg-setting-label">
                  {CATEGORY_ICONS[key as SuggestionCategory]} {CATEGORY_LABELS[key as SuggestionCategory]}
                </span>
                <button
                  className={`toggle-switch ${prefs.categories[key] ? 'on' : ''}`} role="switch" aria-checked={prefs.categories[key]}
                  onClick={() => handleToggleCategory(key)}
                >
                  <div className="toggle-thumb" />
                </button>
              </div>
            ))}

            <div className="panel-section-header">Thresholds</div>

            <div className="sugg-setting-row">
              <span className="sugg-setting-label">Stale item threshold</span>
              <span className="sugg-setting-value">{prefs.stale_threshold_hours}h</span>
            </div>
            <div className="sugg-setting-row">
              <span className="sugg-setting-label">Follow-up threshold</span>
              <span className="sugg-setting-value">{prefs.followup_threshold_hours}h</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
