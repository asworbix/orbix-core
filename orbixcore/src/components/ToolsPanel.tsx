import { useState, useEffect } from 'react';
import type { ToolDef, ToolCategory } from '../types';
import { listTools, toggleTool, setToolAutoApprove } from '../hooks/api';
import './Panel.css';
import './ToolsPanel.css';

interface Props {
  onClose: () => void;
}

const CATEGORY_LABELS: Record<ToolCategory, string> = {
  file_system: 'File System',
  shell: 'Shell',
  network: 'Network',
  utility: 'Utility',
  custom: 'Custom',
};

const CATEGORY_COLORS: Record<ToolCategory, string> = {
  file_system: '#6c8fff',
  shell: '#fbbf24',
  network: '#34d399',
  utility: '#a78bfa',
  custom: '#ec4899',
};

export default function ToolsPanel({ onClose }: Props) {
  const [tools, setTools] = useState<ToolDef[]>([]);
  const [autoApproved, setAutoApproved] = useState<Record<string, boolean>>({});

  useEffect(() => {
    listTools().then(setTools).catch(() => {});
  }, []);

  const handleToggle = async (toolId: string) => {
    try {
      const updated = await toggleTool(toolId);
      setTools(tools.map(t => t.id === toolId ? updated : t));
    } catch {}
  };

  const handleAutoApprove = async (toolName: string) => {
    const current = autoApproved[toolName] ?? false;
    const next = !current;
    setAutoApproved({ ...autoApproved, [toolName]: next });
    try {
      await setToolAutoApprove(toolName, next);
    } catch {
      setAutoApproved({ ...autoApproved, [toolName]: current });
    }
  };

  const grouped = tools.reduce<Record<string, ToolDef[]>>((acc, tool) => {
    const cat = tool.category;
    if (!acc[cat]) acc[cat] = [];
    acc[cat].push(tool);
    return acc;
  }, {});

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>
          </svg>
          Tools
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body">
        {tools.length === 0 ? (
          <div className="panel-empty">
            <div className="panel-empty-icon">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>
              </svg>
            </div>
            <p>No tools available</p>
          </div>
        ) : (
          <div className="tools-list">
            {Object.entries(grouped).map(([cat, catTools]) => (
              <div key={cat} className="tools-category">
                <div className="panel-section-header">
                  <span className="cat-dot" style={{ background: CATEGORY_COLORS[cat as ToolCategory] }} />
                  {CATEGORY_LABELS[cat as ToolCategory] ?? cat}
                </div>
                {catTools.map(tool => (
                  <div key={tool.id} className={`tool-card ${tool.enabled ? '' : 'disabled'}`}>
                    <div className="tool-card-top">
                      <div className="tool-info">
                        <div className="tool-name-row">
                          <span className="tool-name">{tool.name}</span>
                          {tool.requires_approval && (
                            <span className="approval-badge">approval</span>
                          )}
                        </div>
                        <div className="tool-desc">{tool.description}</div>
                      </div>
                      <button
                        className={`toggle-switch ${tool.enabled ? 'on' : 'off'}`} role="switch" aria-checked={tool.enabled}
                        onClick={() => handleToggle(tool.id)}
                        title={tool.enabled ? 'Disable' : 'Enable'}
                      >
                        <div className="toggle-thumb" />
                      </button>
                    </div>
                    {tool.enabled && tool.requires_approval && (
                      <div className="tool-card-bottom">
                        <label className="auto-approve-label">
                          <input
                            type="checkbox"
                            checked={autoApproved[tool.name] ?? false}
                            onChange={() => handleAutoApprove(tool.name)}
                          />
                          Auto-approve
                        </label>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
