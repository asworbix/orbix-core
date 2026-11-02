import { useState, useEffect, useCallback } from 'react';
import type { Workflow } from '../types';
import { listWorkflows, onDataChanged } from '../hooks/api';
import './QuickActions.css';

// Icon + color mapping based on workflow name keywords
const ACTION_STYLES: { keywords: string[]; color: string; icon: React.ReactNode }[] = [
  {
    keywords: ['health', 'status', 'check', 'monitor', 'dashboard'],
    color: '#34D399',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>,
  },
  {
    keywords: ['pipeline', 'crm', 'lead', 'onboarding', 'deal', 'contact'],
    color: '#6C8FFF',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>,
  },
  {
    keywords: ['invoice', 'finance', 'payment', 'budget', 'money', 'financial', 'expense', 'revenue'],
    color: '#F59E0B',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="3" width="20" height="18" rx="2"/><path d="M7 8h10M7 12h6"/></svg>,
  },
  {
    keywords: ['inventory', 'stock', 'erp', 'warehouse', 'supply', 'purchase order'],
    color: '#F97316',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/></svg>,
  },
  {
    keywords: ['project', 'portfolio', 'milestone', 'task', 'ppm'],
    color: '#A78BFA',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/></svg>,
  },
  {
    keywords: ['ticket', 'support', 'triage', 'help', 'sla'],
    color: '#F87171',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>,
  },
  {
    keywords: ['sync', 'end-of-day', 'daily', 'wrap', 'summary', 'report'],
    color: '#06B6D4',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>,
  },
  {
    keywords: ['email', 'mail', 'send', 'message'],
    color: '#6C8FFF',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="M22 4L12 13 2 4"/></svg>,
  },
  {
    keywords: ['deploy', 'build', 'code', 'dev', 'git', 'push'],
    color: '#8B5CF6',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>,
  },
];

const DEFAULT_STYLE = {
  color: '#7B93FF',
  icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10"/></svg>,
};

function getActionStyle(name: string) {
  const lower = name.toLowerCase();
  for (const style of ACTION_STYLES) {
    if (style.keywords.some(kw => lower.includes(kw))) {
      return style;
    }
  }
  return DEFAULT_STYLE;
}

interface Props {
  onRunWorkflow: (wf: Workflow) => void;
}

export default function QuickActions({ onRunWorkflow }: Props) {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [runningId, setRunningId] = useState<string | null>(null);

  useEffect(() => {
    listWorkflows().then(setWorkflows).catch(() => {});
    const unsub = onDataChanged(() => {
      listWorkflows().then(setWorkflows).catch(() => {});
    });
    return () => { unsub.then(fn => fn()); };
  }, []);

  const handleRun = useCallback(async (wf: Workflow) => {
    setRunningId(wf.id);
    onRunWorkflow(wf);
  }, [onRunWorkflow]);

  if (workflows.length === 0) return null;

  return (
    <div className="quick-actions-list">
      <div className="quick-actions-header">Quick Actions</div>
      {workflows.slice(0, 6).map(wf => {
        const style = getActionStyle(wf.name);
        const isRunning = runningId === wf.id;
        return (
          <button
            key={wf.id}
            className={`quick-action-card ${isRunning ? 'running' : ''}`}
            onClick={() => handleRun(wf)}
            disabled={isRunning}
          >
            <div
              className="quick-action-icon"
              style={{ background: `${style.color}18`, color: style.color }}
            >
              {style.icon}
            </div>
            <div className="quick-action-info">
              <span className="quick-action-name">{wf.name}</span>
              {wf.description && (
                <span className="quick-action-desc">{wf.description}</span>
              )}
            </div>
            <div className="quick-action-meta">
              {wf.steps.length} {wf.steps.length === 1 ? 'step' : 'steps'}
            </div>
            <svg className="quick-action-chevron" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="9 18 15 12 9 6"/>
            </svg>
          </button>
        );
      })}
    </div>
  );
}
