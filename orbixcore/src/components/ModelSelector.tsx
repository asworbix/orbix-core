import { useState, useEffect, useRef, useLayoutEffect } from 'react';
import type { LocalModel, DownloadProgress, Agent, UnsupportedModelFile } from '../types';
import {
  listLocalModels, downloadModel, deleteLocalModel, selectLocalModel,
  listUnsupportedModelFiles, deleteUnsupportedModelFile,
  getSelectedLocalModel, onModelDownloadProgress, getDownloadProgress,
  listAgents, updateAgent,
  getLocalLlmStatus, stopLocalLlm,
  addCustomModel, removeCustomModel, hfResolveRepo, type HfGgufFile,
  modelGuidance, type ModelGuidance,
} from '../hooks/api';
import './ModelSelector.css';

interface Props {
  compact?: boolean;
}

const CLAUDE_MODELS = [
  { id: 'claude-opus-4-20250514', name: 'Claude Opus 4', desc: 'Most capable - deep reasoning, complex tasks' },
  { id: 'claude-sonnet-4-20250514', name: 'Claude Sonnet 4', desc: 'Best balance - fast, capable, cost-effective' },
  { id: 'claude-haiku-4-5-20251001', name: 'Claude Haiku 4.5', desc: 'Fastest - lightweight, low latency' },
];

// Sort order for how comfortably a model runs on THIS device.
const FIT_RANK: Record<string, number> = { great: 0, ok: 0, unknown: 1, tight: 2, too_big: 3 };
// The two families Orbix ships and verifies tool calling against. The
// list itself comes from the backend catalog; only the headings live here.
const FAMILIES: { key: string; label: string }[] = [
  { key: 'gemma', label: 'Gemma 4' },
  { key: 'qwen', label: 'Qwen 3' },
];
const isEmbedding = (m: LocalModel) => m.id.includes('embedding');

export default function ModelSelector({ compact }: Props) {
  const [models, setModels] = useState<LocalModel[]>([]);
  const [guidance, setGuidance] = useState<ModelGuidance | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [modelTab, setModelTab] = useState<'local' | 'cloud'>('local');
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [llmRunning, setLlmRunning] = useState(false);
  const [llmModelId, setLlmModelId] = useState('');
  const [downloadError, setDownloadError] = useState<{ modelId: string; message: string } | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [addName, setAddName] = useState('');
  const [addUrl, setAddUrl] = useState('');
  const [addDesc, setAddDesc] = useState('');
  const [addError, setAddError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [addToken, setAddToken] = useState('');
  const [repoFiles, setRepoFiles] = useState<HfGgufFile[] | null>(null);
  const [resolving, setResolving] = useState(false);
  const [orphans, setOrphans] = useState<UnsupportedModelFile[]>([]);

  const isCustom = (m: LocalModel) => !m.builtin;

  // Dropdown placement + dismissal (compact pill). The pill lives in two
  // very different spots - the chat input at the bottom and mid-panel in
  // Settings - so we position the menu with fixed coords from the pill's
  // rect (escapes any parent overflow clipping) and flip up/down by the
  // room available, instead of always opening upward.
  const compactRef = useRef<HTMLDivElement | null>(null);
  const pillRef = useRef<HTMLButtonElement | null>(null);
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties>({});

  useLayoutEffect(() => {
    if (!expanded || !pillRef.current) return;
    const place = () => {
      const r = pillRef.current!.getBoundingClientRect();
      const MENU_W = 340;
      const gap = 6;
      const left = Math.max(8, Math.min(r.left, window.innerWidth - MENU_W - 8));
      const spaceBelow = window.innerHeight - r.bottom;
      const dropUp = spaceBelow < 380 && r.top > spaceBelow;
      setMenuStyle(
        dropUp
          ? { position: 'fixed', left, bottom: window.innerHeight - r.top + gap, top: 'auto',
              maxHeight: Math.min(r.top - 16, 480) }
          : { position: 'fixed', left, top: r.bottom + gap, bottom: 'auto',
              maxHeight: Math.min(spaceBelow - 16, 480) },
      );
    };
    place();
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
    };
  }, [expanded]);

  // Close on click outside or Escape.
  useEffect(() => {
    if (!expanded) return;
    const onDown = (e: MouseEvent) => {
      if (compactRef.current && !compactRef.current.contains(e.target as Node)) {
        setExpanded(false);
      }
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setExpanded(false); };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [expanded]);

  const isDirectGguf = addUrl.trim().toLowerCase().endsWith('.gguf');

  // W1.4: repo input -> list GGUF quants via the HF API, pick one to install.
  const handleBrowseRepo = async () => {
    setAddError(null);
    setResolving(true);
    setRepoFiles(null);
    try {
      const files = await hfResolveRepo(addUrl.trim(), addToken.trim() || undefined);
      setRepoFiles(files);
    } catch (err) {
      setAddError(typeof err === 'string' ? err : (err as Error).message || 'Could not read that repo');
    } finally {
      setResolving(false);
    }
  };

  const handlePickQuant = async (file: HfGgufFile) => {
    setAddError(null);
    setAdding(true);
    try {
      const repoTail = addUrl.trim().replace(/\/+$/, '').split('/').pop() || 'model';
      const base = repoTail.replace(/-?GGUF$/i, '');
      const name = addName.trim() || `${base} ${file.quant || file.filename}`.trim();
      await addCustomModel(name, file.url, addDesc.trim() || undefined, file.size_bytes || undefined);
      const fresh = await listLocalModels();
      setModels(fresh);
      setAddName(''); setAddUrl(''); setAddDesc(''); setAddToken('');
      setRepoFiles(null);
      setAddOpen(false);
    } catch (err) {
      setAddError(typeof err === 'string' ? err : (err as Error).message || 'Failed to add model');
    } finally {
      setAdding(false);
    }
  };

  const handleAddCustom = async () => {
    setAddError(null);
    if (!addName.trim() || !addUrl.trim()) {
      setAddError('Name and URL are required');
      return;
    }
    setAdding(true);
    try {
      const created = await addCustomModel(
        addName.trim(),
        addUrl.trim(),
        addDesc.trim() || undefined,
      );
      // Refresh the list and clear the form.
      const fresh = await listLocalModels();
      setModels(fresh);
      setAddName('');
      setAddUrl('');
      setAddDesc('');
      setAddOpen(false);
      // Optimistically pre-select the new model so the next click downloads it.
      if (created?.id) setSelectedId(prev => prev || null);
    } catch (err) {
      setAddError(typeof err === 'string' ? err : (err as Error).message || 'Failed to add model');
    } finally {
      setAdding(false);
    }
  };

  const handleRemoveCustom = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm('Remove this custom model from the list? The downloaded file is kept on disk.')) return;
    try {
      await removeCustomModel(id);
      const fresh = await listLocalModels();
      setModels(fresh);
    } catch {
      /* ignore */
    }
  };

  useEffect(() => {
    modelGuidance().then(setGuidance).catch(() => {});
    listLocalModels().then(setModels).catch(() => {});
    listUnsupportedModelFiles().then(setOrphans).catch(() => {});
    getSelectedLocalModel().then(setSelectedId).catch(() => {});
    getDownloadProgress().then(p => {
      if (p.is_downloading) setProgress(p);
    }).catch(() => {});
    listAgents().then(setAgents).catch(() => {});
    getLocalLlmStatus().then(s => {
      setLlmRunning(s.running);
      setLlmModelId(s.model_id || '');
    }).catch(() => {});

    // Live progress: prefer Tauri events (they fire every ~500KB on the
    // backend's `app.emit`). On Windows we observed cases where the IPC
    // listener doesn't deliver them reliably even though the backend is
    // streaming bytes - so we also poll the same state via the Tauri command
    // every 600ms as a fallback. Whichever path updates first wins.
    const unsub = onModelDownloadProgress((p) => {
      setProgress(p.percent >= 100 ? null : p);
      if (p.percent >= 100) {
        listLocalModels().then(setModels).catch(() => {});
      }
    });

    let stopped = false;
    let pollTimer: number | undefined;
    const tick = () => {
      if (stopped) return;
      getDownloadProgress().then(p => {
        if (stopped) return;
        if (p.is_downloading) {
          setProgress(prev => {
            // Avoid clobbering a fresher event-driven update.
            if (prev && prev.percent > p.percent && prev.model_id === p.model_id) return prev;
            return p;
          });
        } else {
          setProgress(prev => {
            if (prev && prev.percent < 100) {
              // Backend says we're done - refresh the model list so the row
              // flips to "downloaded" and the bar disappears.
              listLocalModels().then(setModels).catch(() => {});
              return null;
            }
            return prev;
          });
        }
      }).catch(() => {}).finally(() => {
        if (!stopped) pollTimer = window.setTimeout(tick, 600);
      });
    };
    pollTimer = window.setTimeout(tick, 600);

    return () => {
      stopped = true;
      if (pollTimer !== undefined) window.clearTimeout(pollTimer);
      unsub.then(fn => fn());
    };
  }, []);

  const handleSelect = async (id: string) => {
    setSelectedId(id);
    await selectLocalModel(id);
    setLlmRunning(true);
    setLlmModelId(id);
    setExpanded(false);
  };

  const handleSelectClaude = async (modelId: string) => {
    const anthropicAgent = agents.find(a => a.provider === 'anthropic' && a.enabled);
    if (anthropicAgent) {
      try {
        const updated = await updateAgent(anthropicAgent.id, {
          name: anthropicAgent.name,
          provider: anthropicAgent.provider,
          model: modelId,
          endpoint: anthropicAgent.endpoint,
          api_key: '',
          is_default: anthropicAgent.is_default,
        });
        setAgents(agents.map(a => a.id === updated.id ? updated : a));
      } catch { /* */ }
    }
    // Stop local server when switching to cloud
    if (llmRunning) {
      await stopLocalLlm().catch(() => {});
      setLlmRunning(false);
      setLlmModelId('');
    }
    setSelectedId(modelId);
    setExpanded(false);
  };

  const handleDownload = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    // Show immediate feedback
    setProgress({ is_downloading: true, model_id: id, downloaded: 0, total: 1, percent: 0 });
    setDownloadError(null);
    try {
      await downloadModel(id);
    } catch (err) {
      console.error('Download failed:', err);
      // Surface the error to the user - don't just silently reset.
      const msg = typeof err === 'string' ? err
        : err instanceof Error ? err.message
        : 'Download failed for an unknown reason.';
      setDownloadError({ modelId: id, message: msg });
      setProgress(null);
    }
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      if (llmModelId === id) {
        await stopLocalLlm();
        setLlmRunning(false);
        setLlmModelId('');
      }
      await deleteLocalModel(id);
      setModels(prev => prev.map(m => m.id === id ? { ...m, downloaded: false } : m));
      if (selectedId === id) setSelectedId(null);
    } catch { /* */ }
  };

  const orphanBytes = orphans.reduce((n, f) => n + f.size_bytes, 0);
  const handleRemoveOrphans = async () => {
    if (!confirm(`Delete ${orphans.length} leftover model file${orphans.length > 1 ? 's' : ''} (${formatSize(orphanBytes)})? These are from models Orbix no longer ships and are not used by anything.`)) return;
    for (const f of orphans) {
      await deleteUnsupportedModelFile(f.filename).catch(() => {});
    }
    listUnsupportedModelFiles().then(setOrphans).catch(() => {});
  };

  const selected = models.find(m => m.id === selectedId);
  const selectedClaude = CLAUDE_MODELS.find(m => m.id === selectedId);
  const hasAnthropicAgent = agents.some(a => a.provider === 'anthropic' && a.enabled);

  useEffect(() => {
    if (!selectedId) {
      const downloaded = models.find(m => m.downloaded);
      if (downloaded) {
        setSelectedId(downloaded.id);
        selectLocalModel(downloaded.id).catch(() => {});
      }
    }
  }, [models, selectedId]);

  const formatSize = (bytes: number) => {
    const gb = bytes / (1024 * 1024 * 1024);
    return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  };

  const isClaudeSelected = !!selectedClaude;
  const displayName = selectedClaude?.name || selected?.name || 'Select Model';
  const isReady = selected?.downloaded || isClaudeSelected;
  const isLocalMode = !!selected && !isClaudeSelected;

  // Grouped by family, best fit first within a family. Custom models and
  // the embedding model get their own sections.
  const byFit = (a: LocalModel, b: LocalModel) => (FIT_RANK[a.fit_class] ?? 1) - (FIT_RANK[b.fit_class] ?? 1);
  const familyModels = (key: string) =>
    models.filter(m => m.builtin && m.family === key && !isEmbedding(m)).sort(byFit);
  const customModels = models.filter(m => !m.builtin);
  const embeddingModels = models.filter(m => m.builtin && isEmbedding(m));

  const downloadCell = (m: LocalModel) => (
    progress?.model_id === m.id ? (
      <div className="model-progress-inline">
        <div className="model-progress-bar">
          <div className="model-progress-fill gemma4" style={{ width: `${progress.percent}%` }} />
        </div>
        <span className="model-progress-text">{progress.phase === 'projector' ? `vision ${progress.percent}%` : `${progress.percent}%`}</span>
      </div>
    ) : downloadError?.modelId === m.id ? (
      <div className="model-download-error" title={downloadError.message}>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <span>{downloadError.message.length > 50 ? downloadError.message.slice(0, 50) + '…' : downloadError.message}</span>
        <button className="model-download-retry" onClick={(e) => handleDownload(m.id, e)}>Retry</button>
      </div>
    ) : !m.fits ? (
      <span
        className="model-download-btn"
        style={{ opacity: 0.55, cursor: 'default' }}
        title={`Needs ~${formatSize(m.size_bytes * 1.15)} of memory to run - more than this device can give it. Pick a smaller model.`}
      >
        Too big for this device
      </span>
    ) : (
      <button className="model-download-btn gemma4" onClick={(e) => handleDownload(m.id, e)}>
        {formatSize(m.size_bytes)}
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>
        </svg>
      </button>
    )
  );

  // One row in the compact list. `selectable` is false for the embedding
  // model: it is downloaded for search and memory, never talked to.
  const renderRow = (m: LocalModel, selectable = true) => (
    <div
      key={m.id}
      className={`model-option gemma4-option ${m.id === selectedId ? 'selected' : ''} ${!m.downloaded ? 'not-downloaded' : ''}`}
      onClick={() => selectable && m.downloaded && handleSelect(m.id)}
    >
      <div className="model-option-info">
        <span className={`model-dot ${m.downloaded ? 'ready gemma4' : 'pending'}`} />
        <div>
          <div className="model-option-name">
            {m.name}
            {guidance?.recommended_id === m.id ? ' (Best for this device)' : ''}
            {m.fit_class === 'tight' ? ' · slow on this device' : ''}
          </div>
          <div className="model-option-desc">{m.description}</div>
        </div>
      </div>
      {!m.downloaded && downloadCell(m)}
      {m.downloaded && m.vision && !m.vision_ready && (
        progress?.model_id === m.id ? (
          <div className="model-progress-inline">
            <div className="model-progress-bar">
              <div className="model-progress-fill gemma4" style={{ width: `${progress.percent}%` }} />
            </div>
            <span className="model-progress-text">vision {progress.percent}%</span>
          </div>
        ) : (
          <button
            className="model-download-btn gemma4"
            onClick={(e) => handleDownload(m.id, e)}
            title="Download this model's image projector so Bob can see screenshots and photos"
          >
            Add vision
          </button>
        )
      )}
      {m.downloaded && selectable && m.id === selectedId && (
        <div className="model-option-actions">
          {llmRunning && llmModelId === m.id && <span className="model-running-badge">Running</span>}
          <svg className="model-check" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5"><polyline points="20 6 9 17 4 12"/></svg>
        </div>
      )}
      {m.downloaded && (!selectable || m.id !== selectedId) && (
        <button className="model-delete-btn" onClick={(e) => handleDelete(m.id, e)} title="Delete model file">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
        </button>
      )}
      {isCustom(m) && (
        <button
          className="model-delete-btn"
          onClick={(e) => handleRemoveCustom(m.id, e)}
          title="Remove from list (keeps file on disk)"
        >
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      )}
    </div>
  );

  if (compact) {
    return (
      <div className="model-selector-compact" ref={compactRef}>
        <button
          ref={pillRef}
          className="model-selector-pill"
          onClick={() => setExpanded(!expanded)}
        >
          <span className={`model-dot ${isReady ? 'ready' : 'pending'} ${isClaudeSelected ? 'claude' : ''} ${isLocalMode ? 'gemma4' : ''}`} />
          <span className="model-pill-name">
            {displayName}
          </span>
          {isLocalMode && llmRunning && (
            <span className="model-pill-badge local">LOCAL</span>
          )}
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
            <polyline points={expanded ? "18 15 12 9 6 15" : "6 9 12 15 18 9"} />
          </svg>
        </button>

        {expanded && (
          <div className="model-dropdown" style={menuStyle}>
            {/* ── Tab bar ── */}
            <div className="model-tab-bar">
              <button className={`model-tab ${modelTab === 'local' ? 'active' : ''}`} onClick={() => setModelTab('local')}>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                </svg>
                Local
              </button>
              <button className={`model-tab ${modelTab === 'cloud' ? 'active' : ''}`} onClick={() => setModelTab('cloud')}>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/>
                </svg>
                Cloud
              </button>
            </div>

            {modelTab === 'local' && (
              <>
                {guidance && (
                  <div className="model-option-desc" style={{ padding: '8px 12px 6px' }}>
                    This device: {guidance.total_gb} GB memory - models up to ~{guidance.comfortable_model_gb} GB run well. Every model here can see images.
                  </div>
                )}
                {guidance && guidance.recommended_id && !guidance.recommended_downloaded && (
                  <button
                    className="model-download-btn gemma4"
                    style={{ margin: '0 12px 10px', width: 'calc(100% - 24px)' }}
                    onClick={(e) => handleDownload(guidance.recommended_id!, e)}
                  >
                    Get the best model for this device - {guidance.recommended_name}
                  </button>
                )}

                {FAMILIES.map(fam => (
                  <div key={fam.key}>
                    <div className="model-section-label gemma4" style={{ marginTop: 6 }}>{fam.label}</div>
                    {familyModels(fam.key).map(m => renderRow(m))}
                  </div>
                ))}

                {customModels.length > 0 && (
                  <>
                    <div className="model-section-label gemma4" style={{ marginTop: 8 }}>Your models</div>
                    {customModels.map(m => renderRow(m))}
                  </>
                )}

                {embeddingModels.length > 0 && (
                  <>
                    <div className="model-section-label" style={{ marginTop: 8 }}>Embeddings (search and memory)</div>
                    {embeddingModels.map(m => renderRow(m, false))}
                  </>
                )}

                {orphans.length > 0 && (
                  <div className="model-option-desc" style={{ padding: '8px 12px 2px' }}>
                    {orphans.length} leftover file{orphans.length > 1 ? 's' : ''} on disk ({formatSize(orphanBytes)}) from models Orbix no longer ships.{' '}
                    <button className="model-add-toggle" style={{ display: 'inline', padding: 0 }} onClick={handleRemoveOrphans}>
                      Remove
                    </button>
                  </div>
                )}

                <div className="model-add-row">
                  {!addOpen ? (
                    <button className="model-add-toggle" onClick={() => setAddOpen(true)}>
                      + Add custom model
                    </button>
                  ) : (
                    <div className="model-add-form">
                      <div className="model-add-title">Add a GGUF model</div>
                      <input
                        className="model-add-input"
                        placeholder="Display name (e.g. Llama 3.1 8B)"
                        value={addName}
                        onChange={(e) => setAddName(e.target.value)}
                        autoFocus
                      />
                      <input
                        className="model-add-input"
                        placeholder="org/model or https://.../model.gguf"
                        value={addUrl}
                        onChange={(e) => { setAddUrl(e.target.value); setRepoFiles(null); }}
                      />
                      <input
                        className="model-add-input"
                        placeholder="Access token (only for gated repos)"
                        type="password"
                        value={addToken}
                        onChange={(e) => setAddToken(e.target.value)}
                      />
                      {repoFiles && (
                        <div className="model-quant-list">
                          {repoFiles.map((f) => (
                            <button
                              key={f.filename}
                              className="model-quant-row"
                              disabled={f.split || adding}
                              title={f.split ? 'Multi-part GGUF: not installable as a single file' : f.filename}
                              onClick={() => handlePickQuant(f)}
                            >
                              <span className="model-quant-label">{f.quant || f.filename}</span>
                              <span className="model-quant-size">
                                {f.size_bytes ? `${(f.size_bytes / 1e9).toFixed(1)} GB` : '?'}
                                {f.split ? ' (split)' : ''}
                              </span>
                            </button>
                          ))}
                        </div>
                      )}
                      <input
                        className="model-add-input"
                        placeholder="Short description (optional)"
                        value={addDesc}
                        onChange={(e) => setAddDesc(e.target.value)}
                      />
                      {addError && <div className="model-add-error">{addError}</div>}
                      <div className="model-add-actions">
                        <button
                          className="model-add-cancel"
                          onClick={() => { setAddOpen(false); setAddError(null); }}
                          disabled={adding}
                        >
                          Cancel
                        </button>
                        <button
                          className="model-add-submit"
                          onClick={isDirectGguf ? handleAddCustom : handleBrowseRepo}
                          disabled={adding || resolving || !addUrl.trim() || (isDirectGguf && !addName.trim())}
                        >
                          {isDirectGguf
                            ? (adding ? 'Adding…' : 'Add to library')
                            : (resolving ? 'Reading repo…' : 'Browse repo')}
                        </button>
                      </div>
                      <div className="model-add-hint">
                        Paste a Hugging Face repo (like <code>org/model-GGUF</code>) to pick a quant, or a direct <code>.gguf</code> URL.
                      </div>
                    </div>
                  )}
                </div>

                <div className="model-local-note">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                  All local models run 100% on this device. No data leaves it.
                </div>
              </>
            )}

            {modelTab === 'cloud' && (
              <>
                <div className="model-section-label">Claude (Anthropic)</div>
            {CLAUDE_MODELS.map(m => (
              <div
                key={m.id}
                className={`model-option ${m.id === selectedId ? 'selected' : ''} ${!hasAnthropicAgent ? 'not-downloaded' : ''}`}
                onClick={() => hasAnthropicAgent && handleSelectClaude(m.id)}
              >
                <div className="model-option-info">
                  <span className={`model-dot ready claude`} />
                  <div>
                    <div className="model-option-name">{m.name}</div>
                    <div className="model-option-desc">{m.desc}</div>
                  </div>
                </div>
                {!hasAnthropicAgent ? (
                  <span className="model-cloud-hint">Add API key in Agents</span>
                ) : (
                  m.id === selectedId && (
                    <svg className="model-check" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                      <polyline points="20 6 9 17 4 12"/>
                    </svg>
                  )
                )}
              </div>
            ))}

                <div className="model-local-note">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/></svg>
                  Requires an Anthropic API key configured in an agent.
                </div>
              </>
            )}
          </div>
        )}
      </div>
    );
  }

  // Full welcome view
  const renderCard = (m: LocalModel) => (
    <div
      key={m.id}
      className={`model-card gemma4-card ${m.downloaded ? 'downloaded' : ''} ${m.id === selectedId ? 'selected' : ''}`}
      onClick={() => m.downloaded && handleSelect(m.id)}
    >
      <div className="model-card-name">
        {m.name}{guidance?.recommended_id === m.id ? ' · best for this device' : ''}
      </div>
      <div className="model-card-desc">{m.description}</div>
      <div className="model-card-size">{formatSize(m.size_bytes)}{m.fit_class === 'tight' ? ' · slow here' : ''}</div>
      {m.downloaded && m.vision && !m.vision_ready && (
        <button className="model-card-btn download" onClick={(e) => handleDownload(m.id, e)}>
          {progress?.model_id === m.id ? `Vision ${progress.percent}%` : 'Add vision'}
        </button>
      )}
      {m.downloaded ? (
        <button
          className={`model-card-btn ${m.id === selectedId ? 'active' : ''}`}
          onClick={() => handleSelect(m.id)}
        >
          {m.id === selectedId ? (llmRunning ? 'Running' : 'Selected') : 'Use'}
        </button>
      ) : (
        progress?.model_id === m.id ? (
          <div className="model-card-progress">
            <div className="model-progress-bar large">
              <div className="model-progress-fill gemma4" style={{ width: `${progress.percent}%` }} />
            </div>
            <span className="model-progress-text">{progress.phase === 'projector' ? `vision ${progress.percent}%` : `${progress.percent}%`}</span>
          </div>
        ) : !m.fits ? (
          <button className="model-card-btn" disabled title="More memory than this device can give it">
            Too big for this device
          </button>
        ) : (
          <button
            className="model-card-btn download gemma4"
            onClick={(e) => handleDownload(m.id, e)}
          >
            Download
          </button>
        )
      )}
    </div>
  );

  return (
    <div className="model-selector-welcome">
      <h3 className="model-welcome-title">Choose your AI model</h3>
      <p className="model-welcome-sub">
        Run Gemma 4 or Qwen 3 locally - no API keys, no cloud, full privacy, and every model can see images. Or use Claude for maximum capability.
      </p>

      {FAMILIES.map(fam => (
        <div key={fam.key}>
          <div className="model-section-label gemma4" style={{ marginTop: 20, marginBottom: 10, paddingLeft: 2 }}>
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
            </svg>
            {fam.label}
          </div>
          <div className="model-cards">
            {familyModels(fam.key).map(renderCard)}
          </div>
        </div>
      ))}

      {customModels.length > 0 && (
        <>
          <div className="model-section-label" style={{ marginTop: 20, marginBottom: 10, paddingLeft: 2 }}>Your models</div>
          <div className="model-cards">
            {customModels.map(renderCard)}
          </div>
        </>
      )}
    </div>
  );
}
