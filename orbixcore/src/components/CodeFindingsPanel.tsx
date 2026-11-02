import { useState, useEffect } from 'react';
import { codeListFindings, codeListAgents, onCodeFinding, codeOpenInEditor } from '../hooks/api';
import type { CodeFinding, CodeAgent } from '../types';
import './CodeFindingsPanel.css';

interface Props {
  onClose: () => void;
}

type SeverityFilter = 'all' | 'critical' | 'warning' | 'suggestion' | 'info';

export default function CodeFindingsPanel({ onClose }: Props) {
  const [findings, setFindings] = useState<CodeFinding[]>([]);
  const [agents, setAgents] = useState<CodeAgent[]>([]);
  const [filter, setFilter] = useState<SeverityFilter>('all');
  const [expandedId, setExpandedId] = useState<string | null>(null);

  useEffect(() => {
    codeListFindings().then(setFindings).catch(() => {});
    codeListAgents().then(setAgents).catch(() => {});
  }, []);

  useEffect(() => {
    const unsub = onCodeFinding((finding) => {
      setFindings(prev => [...prev, finding]);
    });
    return () => { unsub.then(fn => fn()); };
  }, []);

  const filtered = filter === 'all' ? findings : findings.filter(f => f.severity === filter);

  const counts = {
    all: findings.length,
    critical: findings.filter(f => f.severity === 'critical').length,
    warning: findings.filter(f => f.severity === 'warning').length,
    suggestion: findings.filter(f => f.severity === 'suggestion').length,
    info: findings.filter(f => f.severity === 'info').length,
  };

  const agentName = (id: string) => agents.find(a => a.id === id)?.name || 'Agent';

  return (
    <div className="panel code-findings-panel">
      <div className="panel-header">
        <h2 className="panel-title">Findings</h2>
        <button className="panel-close-btn" onClick={onClose}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <div className="cf-filter-bar">
        {(['all', 'critical', 'warning', 'suggestion', 'info'] as SeverityFilter[]).map(s => (
          <button
            key={s}
            className={`cf-filter-btn ${filter === s ? 'active' : ''} ${s}`}
            onClick={() => setFilter(s)}
          >
            {s === 'all' ? 'All' : s.charAt(0).toUpperCase() + s.slice(1)}
            <span className="cf-filter-count">{counts[s]}</span>
          </button>
        ))}
      </div>

      <div className="panel-body">
        {filtered.length === 0 && (
          <div className="panel-empty">
            {findings.length === 0
              ? 'No findings yet. Run an analysis from the Code tab.'
              : `No ${filter} findings.`}
          </div>
        )}
        {filtered.map(f => (
          <div
            key={f.id}
            className={`cf-card ${expandedId === f.id ? 'expanded' : ''}`}
            onClick={() => setExpandedId(prev => prev === f.id ? null : f.id)}
          >
            <div className="cf-card-top">
              <span className={`cf-severity ${f.severity}`}>{f.severity}</span>
              <span className="cf-category">{f.category}</span>
              <span className="cf-agent">{agentName(f.agent_id)}</span>
            </div>
            <div className="cf-title">{f.title}</div>
            {expandedId === f.id && (
              <div className="cf-expanded">
                <div className="cf-desc">{f.description}</div>
                {f.suggestion && (
                  <div className="cf-suggestion">
                    <div className="cf-suggestion-head">
                      <span className="cf-suggestion-label">Suggestion</span>
                      <button
                        className="cf-copy-btn"
                        onClick={() => navigator.clipboard.writeText(f.suggestion || '')}
                        title="Copy suggestion"
                      >Copy</button>
                    </div>
                    <div className="cf-suggestion-text">{f.suggestion}</div>
                  </div>
                )}
                {f.file_path && (
                  <div className="cf-file">
                    <span className="cf-file-path">
                      {f.file_path}{f.line_start != null && `:${f.line_start}`}
                    </span>
                    <button
                      className="cf-open-btn"
                      onClick={() => codeOpenInEditor(f.file_path as string, f.line_start ?? undefined).catch(() => {})}
                      title="Open in editor"
                    >Open in editor</button>
                  </div>
                )}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
