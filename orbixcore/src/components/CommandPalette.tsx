import { useState, useEffect, useRef, useCallback } from 'react';
import type { Workflow, PanelType } from '../types';
import { listWorkflows, openRunMonitor } from '../hooks/api';
import './CommandPalette.css';

interface CommandItem {
  id: string;
  label: string;
  description?: string;
  category: 'workflow' | 'panel' | 'action';
  icon: React.ReactNode;
  action: () => void;
}

const PANEL_COMMANDS: { id: PanelType; label: string; icon: React.ReactNode }[] = [
  {
    id: 'agents', label: 'Agents',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="8" r="4"/><path d="M5 20c0-4 3.1-7 7-7s7 3 7 7"/></svg>,
  },
  {
    id: 'tools', label: 'Tools',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>,
  },
  {
    id: 'memory', label: 'Memory',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></svg>,
  },
  {
    id: 'guide', label: 'How to Orbix',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>,
  },
  {
    id: 'my-ui', label: 'My UI',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="9" y1="21" x2="9" y2="9"/></svg>,
  },
  {
    id: 'window-intel', label: 'Window Intelligence',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="3" width="20" height="14" rx="2"/><circle cx="12" cy="10" r="3"/></svg>,
  },
  {
    id: 'voice', label: 'Voice Settings',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 2a3 3 0 0 1 3 3v7a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/></svg>,
  },
  {
    id: 'settings', label: 'Settings',
    icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09"/></svg>,
  },
];

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenPanel: (panel: PanelType) => void;
  onRunWorkflow: (wf: Workflow) => void;
}

export default function CommandPalette({ open, onClose, onOpenPanel, onRunWorkflow }: Props) {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      listWorkflows().then(setWorkflows).catch(() => {});
      setQuery('');
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const buildCommands = useCallback((): CommandItem[] => {
    const items: CommandItem[] = [];

    // Workflows
    for (const wf of workflows) {
      items.push({
        id: `wf-${wf.id}`,
        label: wf.name,
        description: `${wf.steps.length} steps`,
        category: 'workflow',
        icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>,
        action: () => { onRunWorkflow(wf); onClose(); },
      });
    }

    // The Run monitor window: live log of everything Bob and the agents do.
    items.push({
      id: 'run-monitor',
      label: 'Run monitor',
      description: 'Live log of what Bob and the agents are doing',
      category: 'action',
      icon: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="3" y="4" width="18" height="14" rx="2"/><path d="M7 9h6M7 13h10"/></svg>,
      action: () => { openRunMonitor().catch(() => {}); onClose(); },
    });

    // Panels
    for (const panel of PANEL_COMMANDS) {
      items.push({
        id: `panel-${panel.id}`,
        label: `Open ${panel.label}`,
        category: 'panel',
        icon: panel.icon,
        action: () => { onOpenPanel(panel.id); onClose(); },
      });
    }

    return items;
  }, [workflows, onOpenPanel, onRunWorkflow, onClose]);

  const commands = buildCommands();

  const filtered = query.trim()
    ? commands.filter(cmd =>
        cmd.label.toLowerCase().includes(query.toLowerCase()) ||
        (cmd.description || '').toLowerCase().includes(query.toLowerCase())
      )
    : commands;

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex(i => Math.min(i + 1, filtered.length - 1));
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex(i => Math.max(i - 1, 0));
    }
    if (e.key === 'Enter' && filtered[selectedIndex]) {
      filtered[selectedIndex].action();
    }
  };

  if (!open) return null;

  // Group by category
  const workflowItems = filtered.filter(c => c.category === 'workflow');
  const panelItems = filtered.filter(c => c.category === 'panel');
  const actionItems = filtered.filter(c => c.category === 'action');

  return (
    <>
      <div className="cmd-backdrop" onClick={onClose} />
      <div className="cmd-palette" onKeyDown={handleKeyDown}>
        <div className="cmd-search">
          <svg className="cmd-search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input
            ref={inputRef}
            className="cmd-input"
            placeholder="Search actions, workflows, panels..."
            value={query}
            onChange={e => setQuery(e.target.value)}
            autoFocus
          />
          <kbd className="cmd-esc">esc</kbd>
        </div>

        <div className="cmd-results">
          {filtered.length === 0 && (
            <div className="cmd-empty">No results for "{query}"</div>
          )}

          {workflowItems.length > 0 && (
            <>
              <div className="cmd-group-label">Workflows</div>
              {workflowItems.map(cmd => {
                const globalIdx = filtered.indexOf(cmd);
                return (
                  <button
                    key={cmd.id}
                    className={`cmd-item ${globalIdx === selectedIndex ? 'selected' : ''}`}
                    onClick={cmd.action}
                    onMouseEnter={() => setSelectedIndex(globalIdx)}
                  >
                    <div className="cmd-item-icon">{cmd.icon}</div>
                    <span className="cmd-item-label">{cmd.label}</span>
                    {cmd.description && <span className="cmd-item-desc">{cmd.description}</span>}
                  </button>
                );
              })}
            </>
          )}

          {actionItems.length > 0 && (
            <>
              <div className="cmd-group-label">Actions</div>
              {actionItems.map(cmd => {
                const globalIdx = filtered.indexOf(cmd);
                return (
                  <button
                    key={cmd.id}
                    className={`cmd-item ${globalIdx === selectedIndex ? 'selected' : ''}`}
                    onClick={cmd.action}
                    onMouseEnter={() => setSelectedIndex(globalIdx)}
                  >
                    <div className="cmd-item-icon">{cmd.icon}</div>
                    <span className="cmd-item-label">{cmd.label}</span>
                    {cmd.description && <span className="cmd-item-desc">{cmd.description}</span>}
                  </button>
                );
              })}
            </>
          )}

          {panelItems.length > 0 && (
            <>
              <div className="cmd-group-label">Navigation</div>
              {panelItems.map(cmd => {
                const globalIdx = filtered.indexOf(cmd);
                return (
                  <button
                    key={cmd.id}
                    className={`cmd-item ${globalIdx === selectedIndex ? 'selected' : ''}`}
                    onClick={cmd.action}
                    onMouseEnter={() => setSelectedIndex(globalIdx)}
                  >
                    <div className="cmd-item-icon">{cmd.icon}</div>
                    <span className="cmd-item-label">{cmd.label}</span>
                  </button>
                );
              })}
            </>
          )}
        </div>

        <div className="cmd-footer">
          <span><kbd>↑↓</kbd> navigate</span>
          <span><kbd>↵</kbd> run</span>
          <span><kbd>esc</kbd> close</span>
        </div>
      </div>
    </>
  );
}
