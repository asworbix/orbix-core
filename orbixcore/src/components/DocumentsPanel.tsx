import { useState, useEffect, useCallback } from 'react';
import {
  listGeneratedDocuments, previewGeneratedDocument, openGeneratedDocument,
  revealGeneratedDocument, regenerateDocument, deleteGeneratedDocument,
  type GeneratedDoc, type DocSection,
} from '../hooks/api';
import './Panel.css';
import './DocumentsPanel.css';

interface Props {
  onClose: () => void;
}

const KIND_LABEL: Record<string, string> = { docx: 'Word', pptx: 'PowerPoint', html: 'Web page', website: 'Website' };

/* Generated documents: Quick Look previews plus spec-level editing.
   Editing changes the structured content the file was generated from,
   then regenerates - into the same format or any other. */
export default function DocumentsPanel({ onClose }: Props) {
  const [docs, setDocs] = useState<GeneratedDoc[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState<{ kind: string; title: string; subtitle: string; sections: DocSection[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(() => {
    listGeneratedDocuments().then(setDocs).catch(() => {});
  }, []);
  useEffect(() => { refresh(); }, [refresh]);

  const startEdit = (d: GeneratedDoc) => {
    setEditingId(d.id);
    setError('');
    setDraft({
      kind: d.kind,
      title: d.spec.title,
      subtitle: d.spec.subtitle ?? '',
      sections: d.spec.sections.map(s => ({ heading: s.heading, body: s.body ?? '', bullets: [...(s.bullets ?? [])] })),
    });
  };

  const patchSection = (i: number, patch: Partial<DocSection>) => {
    setDraft(prev => prev ? {
      ...prev,
      sections: prev.sections.map((s, j) => j === i ? { ...s, ...patch } : s),
    } : prev);
  };

  const handleRegenerate = async () => {
    if (!editingId || !draft || !draft.title.trim()) return;
    setBusy(true);
    setError('');
    try {
      await regenerateDocument(editingId, draft.kind, {
        title: draft.title,
        subtitle: draft.subtitle || undefined,
        sections: draft.sections,
      });
      setEditingId(null);
      setDraft(null);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <polyline points="14 2 14 8 20 8"/>
          </svg>
          Documents
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
        {docs.length === 0 && (
          <div className="panel-empty">
            <p>No generated documents yet</p>
            <p className="panel-empty-sub">
              Ask Bob: "create a PowerPoint about..." - or generate a sample from Settings, Brand.
            </p>
          </div>
        )}

        {docs.map(d => (
          <div key={d.id} className="doc-card">
            <div className="doc-card-head">
              <span className={`doc-kind ${d.kind}`}>{KIND_LABEL[d.kind] ?? d.kind}</span>
              <div className="doc-info">
                <span className="doc-title">{d.title}</span>
                <span className="doc-meta">
                  {new Date(d.created_at).toLocaleString()} &middot; {d.kit_name}
                </span>
              </div>
            </div>
            <div className="doc-actions">
              <button className="link-btn" onClick={() => previewGeneratedDocument(d.path).catch(() => {})}>
                Preview
              </button>
              <button className="link-btn" onClick={() => openGeneratedDocument(d.path).catch(() => {})}>
                Open
              </button>
              <button className="link-btn" onClick={() => startEdit(d)}>
                {editingId === d.id ? 'Editing...' : 'Edit'}
              </button>
              <button className="link-btn" onClick={() => revealGeneratedDocument(d.path).catch(() => {})}>
                Finder
              </button>
              <button
                className="link-btn danger"
                onClick={() => deleteGeneratedDocument(d.id, false).then(refresh)}
                title="Remove from this list (keeps the file)"
              >
                Remove
              </button>
            </div>

            {editingId === d.id && draft && (
              <div className="doc-editor">
                <div className="doc-editor-row">
                  <label className="form-label">Format</label>
                  <select
                    className="form-input form-select"
                    value={draft.kind}
                    onChange={e => setDraft({ ...draft, kind: e.target.value })}
                  >
                    <option value="docx">Word (.docx)</option>
                    <option value="pptx">PowerPoint (.pptx)</option>
                    <option value="html">Web page (.html)</option>
                    <option value="website">Website (folder)</option>
                  </select>
                </div>
                <label className="form-label">Title</label>
                <input
                  className="form-input"
                  value={draft.title}
                  onChange={e => setDraft({ ...draft, title: e.target.value })}
                />
                <label className="form-label">Subtitle</label>
                <input
                  className="form-input"
                  value={draft.subtitle}
                  onChange={e => setDraft({ ...draft, subtitle: e.target.value })}
                />
                {draft.sections.map((s, i) => (
                  <div key={i} className="doc-editor-section">
                    <div className="doc-editor-section-head">
                      <input
                        className="form-input"
                        placeholder="Section heading"
                        value={s.heading}
                        onChange={e => patchSection(i, { heading: e.target.value })}
                      />
                      <button
                        className="link-btn danger"
                        onClick={() => setDraft({ ...draft, sections: draft.sections.filter((_, j) => j !== i) })}
                        aria-label="Remove section"
                      >
                        Remove
                      </button>
                    </div>
                    <textarea
                      className="form-input"
                      rows={3}
                      placeholder="Body text. Blank line = new paragraph."
                      value={s.body ?? ''}
                      onChange={e => patchSection(i, { body: e.target.value })}
                    />
                    <textarea
                      className="form-input"
                      rows={2}
                      placeholder="Bullets, one per line"
                      value={(s.bullets ?? []).join('\n')}
                      onChange={e => patchSection(i, { bullets: e.target.value.split('\n').filter(b => b.trim()) })}
                    />
                  </div>
                ))}
                <button
                  className="panel-btn"
                  onClick={() => setDraft({ ...draft, sections: [...draft.sections, { heading: '', body: '', bullets: [] }] })}
                >
                  + Add section
                </button>
                {error && <div className="form-error">{error}</div>}
                <div className="doc-editor-actions">
                  <button className="panel-btn" onClick={() => { setEditingId(null); setDraft(null); }}>Cancel</button>
                  <button className="panel-btn accent" disabled={busy || !draft.title.trim()} onClick={handleRegenerate}>
                    {busy ? 'Regenerating...' : 'Regenerate document'}
                  </button>
                </div>
                <span className="doc-editor-hint">
                  Regenerating writes a fresh branded file and opens this entry to it. Files you
                  edited manually in Office stay untouched on disk.
                </span>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
