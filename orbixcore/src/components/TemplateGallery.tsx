import { useState, useEffect } from 'react';
import type { WorkflowTemplate, TemplateCategory, Workflow } from '../types';
import {
  listWorkflowTemplates, listTemplateCategories, instantiateTemplate,
  deleteWorkflowTemplate,
} from '../hooks/api';
import './TemplateGallery.css';

interface Props {
  onCreated: (workflow: Workflow) => void;
  onClose: () => void;
}

const CATEGORY_ICONS: Record<string, React.ReactNode> = {
  database: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>
    </svg>
  ),
  zap: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <polygon points="13 2 3 14 12 14 11 22 21 10 12 10"/>
    </svg>
  ),
  activity: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>
    </svg>
  ),
  'bar-chart': (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <line x1="12" y1="20" x2="12" y2="10"/><line x1="18" y1="20" x2="18" y2="4"/><line x1="6" y1="20" x2="6" y2="16"/>
    </svg>
  ),
  user: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>
    </svg>
  ),
};

const STEP_TYPE_COLORS: Record<string, string> = {
  prompt: '#6C8FFF',
  action: '#34D399',
  condition: '#FBBF24',
  wait: '#A78BFA',
  loop: '#EC4899',
};

export default function TemplateGallery({ onCreated, onClose }: Props) {
  const [templates, setTemplates] = useState<WorkflowTemplate[]>([]);
  const [categories, setCategories] = useState<TemplateCategory[]>([]);
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [loading, setLoading] = useState(false);
  const [previewId, setPreviewId] = useState<string | null>(null);

  useEffect(() => {
    listWorkflowTemplates().then(setTemplates).catch(() => {});
    listTemplateCategories().then(setCategories).catch(() => {});
  }, []);

  const filtered = selectedCategory === 'all'
    ? templates
    : templates.filter(t => t.category === selectedCategory);

  const preview = previewId ? templates.find(t => t.id === previewId) : null;

  const handleUse = async (templateId: string) => {
    setLoading(true);
    try {
      const workflow = await instantiateTemplate(templateId);
      onCreated(workflow);
    } catch { /* */ }
    setLoading(false);
  };

  const handleDelete = async (id: string) => {
    await deleteWorkflowTemplate(id).catch(() => {});
    setTemplates(prev => prev.filter(t => t.id !== id));
  };

  if (preview) {
    return (
      <div className="tpl-preview">
        <div className="tpl-preview-header">
          <button className="tpl-back-btn" onClick={() => setPreviewId(null)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="15 18 9 12 15 6"/>
            </svg>
            Back
          </button>
          <button
            className="panel-btn accent"
            onClick={() => handleUse(preview.id)}
            disabled={loading}
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            {loading ? 'Creating...' : 'Use Template'}
          </button>
        </div>

        <div className="tpl-preview-title">{preview.name}</div>
        <div className="tpl-preview-desc">{preview.description}</div>

        <div className="tpl-preview-steps">
          <div className="tpl-preview-steps-title">Steps ({preview.steps.length})</div>
          {preview.steps.map((step, i) => (
            <div key={i} className="tpl-preview-step">
              <div className="tpl-preview-step-num">{i + 1}</div>
              <div className="tpl-preview-step-info">
                <span
                  className="tpl-preview-step-badge"
                  style={{ background: STEP_TYPE_COLORS[step.step_type] || '#666' }}
                >
                  {step.step_type}
                </span>
                <span className="tpl-preview-step-label">{step.label}</span>
                {step.connector_id && (
                  <span className="tpl-preview-step-connector">
                    <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/></svg>
                    Needs connector binding
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>

        {!preview.is_builtin && (
          <button className="link-btn danger" style={{ marginTop: 8 }} onClick={() => { handleDelete(preview.id); setPreviewId(null); }}>
            Delete Template
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="tpl-gallery">
      <div className="tpl-gallery-header">
        <div className="tpl-gallery-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="9" y1="21" x2="9" y2="9"/>
          </svg>
          Templates
        </div>
        <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      {/* Category filter */}
      <div className="tpl-categories">
        <button
          className={`tpl-cat-btn ${selectedCategory === 'all' ? 'active' : ''}`}
          onClick={() => setSelectedCategory('all')}
        >
          All
        </button>
        {categories.map(cat => (
          <button
            key={cat.id}
            className={`tpl-cat-btn ${selectedCategory === cat.id ? 'active' : ''}`}
            onClick={() => setSelectedCategory(cat.id)}
          >
            {CATEGORY_ICONS[cat.icon] || null}
            {cat.label}
          </button>
        ))}
      </div>

      {/* Template grid */}
      <div className="tpl-grid">
        {filtered.length === 0 ? (
          <div className="tpl-empty">No templates in this category</div>
        ) : (
          filtered.map(tpl => (
            <div key={tpl.id} className="tpl-card" onClick={() => setPreviewId(tpl.id)}>
              <div className="tpl-card-top">
                <div className="tpl-card-name">{tpl.name}</div>
                {!tpl.is_builtin && <span className="tpl-card-custom-badge">Custom</span>}
              </div>
              <div className="tpl-card-desc">{tpl.description}</div>
              <div className="tpl-card-footer">
                <div className="tpl-card-steps">
                  {tpl.steps.map((s, i) => (
                    <span
                      key={i}
                      className="tpl-card-step-dot"
                      style={{ background: STEP_TYPE_COLORS[s.step_type] || '#666' }}
                      title={`${s.step_type}: ${s.label}`}
                    />
                  ))}
                </div>
                <span className="tpl-card-step-count">{tpl.steps.length} steps</span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
