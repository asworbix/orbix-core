import { useState, useEffect, useCallback } from 'react';
import type { UniversalConnector, UniversalAuthType, AuthConfig, ConnectorEndpoint, DiscoveryResult, ConnectorTestResult } from '../types';
import {
  listUniversalConnectors, createUniversalConnector,
  deleteUniversalConnector, discoverConnectorEndpoints, addConnectorEndpoint,
  removeConnectorEndpoint, testUniversalConnector, executeConnectorEndpoint,
} from '../hooks/api';
import './Panel.css';
import './UniversalConnectorsPanel.css';

interface Props {
  onClose: () => void;
}

type View = 'list' | 'add' | 'detail' | 'discover';

const AUTH_LABELS: Record<UniversalAuthType, string> = {
  none: 'No Auth',
  api_key: 'API Key',
  bearer: 'Bearer Token',
  oauth2: 'OAuth 2.0',
  basic_auth: 'Basic Auth',
};

const AUTH_COLORS: Record<UniversalAuthType, string> = {
  none: 'var(--text-dim)',
  api_key: '#fbbf24',
  bearer: '#6c8fff',
  oauth2: '#a855f7',
  basic_auth: '#34d399',
};

const METHOD_COLORS: Record<string, string> = {
  GET: '#34d399',
  POST: '#6c8fff',
  PUT: '#fbbf24',
  PATCH: '#a855f7',
  DELETE: '#f87171',
};

const STATUS_DOT: Record<string, string> = {
  connected: '#34d399',
  connecting: '#fbbf24',
  error: '#f87171',
  disconnected: 'var(--text-dim)',
  expired: '#f87171',
};

export default function UniversalConnectorsPanel({ onClose }: Props) {
  const [connectors, setConnectors] = useState<UniversalConnector[]>([]);
  const [view, setView] = useState<View>('list');
  const [selected, setSelected] = useState<UniversalConnector | null>(null);
  const [error, setError] = useState('');

  // Add form state
  const [formName, setFormName] = useState('');
  const [formUrl, setFormUrl] = useState('');
  const [formAuthType, setFormAuthType] = useState<UniversalAuthType>('none');
  const [formAuth, setFormAuth] = useState<AuthConfig>({});
  const [formHeaders, setFormHeaders] = useState('');
  const [saving, setSaving] = useState(false);

  // Discovery state
  const [discovering, setDiscovering] = useState(false);
  const [discoveryResult, setDiscoveryResult] = useState<DiscoveryResult | null>(null);

  // Test state
  const [testing, setTesting] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<ConnectorTestResult | null>(null);

  // Execute state
  const [execEndpoint, setExecEndpoint] = useState<string | null>(null);
  const [execVars, setExecVars] = useState<Record<string, string>>({});
  const [execResult, setExecResult] = useState<{ status: number; body: string } | null>(null);
  const [executing, setExecuting] = useState(false);

  // Endpoint add form
  const [showAddEndpoint, setShowAddEndpoint] = useState(false);
  const [epKey, setEpKey] = useState('');
  const [epMethod, setEpMethod] = useState('GET');
  const [epPath, setEpPath] = useState('');
  const [epDesc, setEpDesc] = useState('');

  useEffect(() => {
    listUniversalConnectors().then(setConnectors).catch(() => {});
  }, []);

  const resetForm = () => {
    setFormName('');
    setFormUrl('');
    setFormAuthType('none');
    setFormAuth({});
    setFormHeaders('');
    setError('');
    setDiscoveryResult(null);
    setTestResult(null);
  };

  const handleAdd = useCallback(async () => {
    if (!formName.trim() || !formUrl.trim()) {
      setError('Name and Base URL are required');
      return;
    }
    setSaving(true);
    setError('');
    try {
      let headers: Record<string, string> = {};
      if (formHeaders.trim()) {
        try { headers = JSON.parse(formHeaders); } catch { setError('Invalid headers JSON'); setSaving(false); return; }
      }
      const connector = await createUniversalConnector(formName, formUrl, formAuthType, formAuth, headers);
      setConnectors(prev => [...prev, connector]);
      resetForm();
      setView('list');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setSaving(false);
  }, [formName, formUrl, formAuthType, formAuth, formHeaders]);

  const handleDiscover = useCallback(async () => {
    if (!formUrl.trim()) { setError('Enter a Base URL first'); return; }
    setDiscovering(true);
    setError('');
    try {
      const result = await discoverConnectorEndpoints(formUrl);
      setDiscoveryResult(result);
      if (result.title && !formName) setFormName(result.title);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setDiscovering(false);
  }, [formUrl, formName]);

  const handleTest = useCallback(async (id: string) => {
    setTesting(id);
    setTestResult(null);
    try {
      const result = await testUniversalConnector(id);
      setTestResult(result);
      listUniversalConnectors().then(setConnectors).catch(() => {});
    } catch {
      setTestResult({ success: false, message: 'Test failed', details: null });
    }
    setTesting(null);
  }, []);

  const handleDelete = useCallback(async (id: string) => {
    await deleteUniversalConnector(id).catch(() => {});
    setConnectors(prev => prev.filter(c => c.id !== id));
    if (selected?.id === id) { setSelected(null); setView('list'); }
  }, [selected]);

  const handleAddEndpoint = useCallback(async () => {
    if (!selected || !epKey.trim() || !epPath.trim()) return;
    const ep: ConnectorEndpoint = { key: epKey, method: epMethod, path: epPath, description: epDesc };
    try {
      await addConnectorEndpoint(selected.id, ep);
      const updated = { ...selected, endpoints: [...selected.endpoints, ep] };
      setSelected(updated);
      setConnectors(prev => prev.map(c => c.id === selected.id ? updated : c));
      setShowAddEndpoint(false);
      setEpKey(''); setEpMethod('GET'); setEpPath(''); setEpDesc('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [selected, epKey, epMethod, epPath, epDesc]);

  const handleRemoveEndpoint = useCallback(async (key: string) => {
    if (!selected) return;
    await removeConnectorEndpoint(selected.id, key).catch(() => {});
    const updated = { ...selected, endpoints: selected.endpoints.filter(e => e.key !== key) };
    setSelected(updated);
    setConnectors(prev => prev.map(c => c.id === selected.id ? updated : c));
  }, [selected]);

  const handleExecute = useCallback(async (endpointKey: string) => {
    if (!selected) return;
    setExecuting(true);
    setExecResult(null);
    try {
      const result = await executeConnectorEndpoint(selected.id, endpointKey, execVars);
      setExecResult(result);
    } catch (e) {
      setExecResult({ status: 0, body: e instanceof Error ? e.message : String(e) });
    }
    setExecuting(false);
  }, [selected, execVars]);

  const importDiscoveredEndpoints = useCallback(async () => {
    if (!discoveryResult?.endpoints.length || !selected) return;
    for (const ep of discoveryResult.endpoints) {
      try { await addConnectorEndpoint(selected.id, ep); } catch { /* skip duplicates */ }
    }
    const refreshed = await listUniversalConnectors();
    setConnectors(refreshed);
    const updated = refreshed.find(c => c.id === selected.id);
    if (updated) setSelected(updated);
    setDiscoveryResult(null);
  }, [discoveryResult, selected]);

  const openDetail = (c: UniversalConnector) => {
    setSelected(c);
    setTestResult(null);
    setExecEndpoint(null);
    setExecResult(null);
    setError('');
    setView('detail');
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3"/>
            <path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83"/>
          </svg>
          {view === 'add' ? 'Add Connector' : view === 'detail' ? selected?.name || 'Connector' : 'Universal Connectors'}
        </div>
        <div className="panel-header-actions">
          {view === 'list' && (
            <button className="panel-btn accent" onClick={() => { resetForm(); setView('add'); }}>
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              Add
            </button>
          )}
          {view !== 'list' && (
            <button className="panel-close-btn" onClick={() => { setView('list'); setError(''); }} title="Back">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="15 18 9 12 15 6"/>
              </svg>
            </button>
          )}
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body">
        {/* ── LIST VIEW ── */}
        {view === 'list' && (
          <>
            {connectors.length === 0 ? (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                    <circle cx="12" cy="12" r="3"/>
                    <path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83"/>
                  </svg>
                </div>
                <p>No universal connectors</p>
                <p className="panel-empty-sub">Connect any REST API with auto-discovery, auth management, and endpoint testing.</p>
                <button className="panel-btn accent" onClick={() => { resetForm(); setView('add'); }}>Add Connector</button>
              </div>
            ) : (
              <div className="uc-list">
                {connectors.map(c => (
                  <div key={c.id} className="uc-card" onClick={() => openDetail(c)}>
                    <div className="uc-card-left">
                      <div className="uc-status-dot" style={{ background: STATUS_DOT[c.status] || STATUS_DOT.disconnected }} />
                      <div className="uc-card-info">
                        <div className="uc-card-name">{c.name}</div>
                        <div className="uc-card-url">{c.base_url}</div>
                      </div>
                    </div>
                    <div className="uc-card-right">
                      <span className="uc-auth-badge" style={{ color: AUTH_COLORS[c.auth_type] }}>
                        {AUTH_LABELS[c.auth_type]}
                      </span>
                      <span className="uc-ep-count">{c.endpoints.length} ep</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </>
        )}

        {/* ── ADD VIEW ── */}
        {view === 'add' && (
          <div className="panel-form animate-fade-up">
            <div className="connector-field-group">
              <label className="connector-field-label">Base URL <span className="connector-required">*</span></label>
              <div className="uc-url-row">
                <input
                  className="form-input"
                  placeholder="https://api.example.com"
                  value={formUrl}
                  onChange={e => setFormUrl(e.target.value)}
                />
                <button
                  className="panel-btn uc-discover-btn"
                  onClick={handleDiscover}
                  disabled={discovering || !formUrl.trim()}
                  title="Auto-discover endpoints" aria-label="Auto-discover endpoints"
                >
                  {discovering ? (
                    <svg className="spin" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                    </svg>
                  ) : (
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
                    </svg>
                  )}
                  Discover
                </button>
              </div>
            </div>

            {discoveryResult && (
              <div className={`uc-discovery-result ${discoveryResult.found ? 'found' : 'empty'}`}>
                {discoveryResult.found ? (
                  <>
                    <strong>{discoveryResult.title || 'API Found'}</strong>
                    {discoveryResult.spec_type && <span className="uc-spec-badge">{discoveryResult.spec_type}</span>}
                    <span>{discoveryResult.endpoints.length} endpoints discovered</span>
                    {discoveryResult.description && <span className="uc-discovery-desc">{discoveryResult.description}</span>}
                  </>
                ) : (
                  <span>No OpenAPI/Swagger spec found. Add endpoints manually.</span>
                )}
              </div>
            )}

            <div className="connector-field-group">
              <label className="connector-field-label">Name <span className="connector-required">*</span></label>
              <input
                className="form-input"
                placeholder="e.g., Stripe API"
                value={formName}
                onChange={e => setFormName(e.target.value)}
              />
            </div>

            <div className="connector-field-group">
              <label className="connector-field-label">Authentication</label>
              <div className="uc-auth-grid">
                {(Object.keys(AUTH_LABELS) as UniversalAuthType[]).map(t => (
                  <button
                    key={t}
                    className={`uc-auth-option ${formAuthType === t ? 'active' : ''}`}
                    onClick={() => { setFormAuthType(t); setFormAuth({}); }}
                    style={{ '--auth-color': AUTH_COLORS[t] } as React.CSSProperties}
                  >
                    <span className="uc-auth-dot" style={{ background: AUTH_COLORS[t] }} />
                    {AUTH_LABELS[t]}
                  </button>
                ))}
              </div>
            </div>

            {/* Auth-specific fields */}
            {formAuthType === 'api_key' && (
              <>
                <div className="connector-field-group">
                  <label className="connector-field-label">Header Name</label>
                  <input
                    className="form-input"
                    placeholder="X-API-Key"
                    value={formAuth.header_name || ''}
                    onChange={e => setFormAuth(a => ({ ...a, header_name: e.target.value }))}
                  />
                </div>
                <div className="connector-field-group">
                  <label className="connector-field-label">API Key</label>
                  <input
                    className="form-input"
                    type="password"
                    placeholder="sk_live_..."
                    value={formAuth.header_value || ''}
                    onChange={e => setFormAuth(a => ({ ...a, header_value: e.target.value }))}
                  />
                </div>
              </>
            )}

            {formAuthType === 'bearer' && (
              <div className="connector-field-group">
                <label className="connector-field-label">Token</label>
                <input
                  className="form-input"
                  type="password"
                  placeholder="eyJhbGciOi..."
                  value={formAuth.token || ''}
                  onChange={e => setFormAuth(a => ({ ...a, token: e.target.value }))}
                />
              </div>
            )}

            {formAuthType === 'basic_auth' && (
              <>
                <div className="connector-field-group">
                  <label className="connector-field-label">Username</label>
                  <input
                    className="form-input"
                    placeholder="username"
                    value={formAuth.username || ''}
                    onChange={e => setFormAuth(a => ({ ...a, username: e.target.value }))}
                  />
                </div>
                <div className="connector-field-group">
                  <label className="connector-field-label">Password</label>
                  <input
                    className="form-input"
                    type="password"
                    placeholder="password"
                    value={formAuth.password || ''}
                    onChange={e => setFormAuth(a => ({ ...a, password: e.target.value }))}
                  />
                </div>
              </>
            )}

            {formAuthType === 'oauth2' && (
              <>
                <div className="connector-field-group">
                  <label className="connector-field-label">Client ID</label>
                  <input className="form-input" placeholder="client_id" value={formAuth.client_id || ''} onChange={e => setFormAuth(a => ({ ...a, client_id: e.target.value }))} />
                </div>
                <div className="connector-field-group">
                  <label className="connector-field-label">Client Secret</label>
                  <input className="form-input" type="password" placeholder="client_secret" value={formAuth.client_secret || ''} onChange={e => setFormAuth(a => ({ ...a, client_secret: e.target.value }))} />
                </div>
                <div className="connector-field-group">
                  <label className="connector-field-label">Token URL</label>
                  <input className="form-input" placeholder="https://auth.example.com/token" value={formAuth.token_url || ''} onChange={e => setFormAuth(a => ({ ...a, token_url: e.target.value }))} />
                </div>
              </>
            )}

            <div className="connector-field-group">
              <label className="connector-field-label">Default Headers <span style={{ fontWeight: 400, textTransform: 'none' }}>(JSON, optional)</span></label>
              <input
                className="form-input"
                placeholder='{"Content-Type": "application/json"}'
                value={formHeaders}
                onChange={e => setFormHeaders(e.target.value)}
              />
            </div>

            {error && <div className="form-error">{error}</div>}

            <div className="connector-setup-actions">
              <button className="panel-btn" onClick={() => { setView('list'); setError(''); }}>Cancel</button>
              <button className="panel-btn accent" onClick={handleAdd} disabled={saving || !formName.trim() || !formUrl.trim()}>
                {saving ? 'Creating...' : 'Create Connector'}
              </button>
            </div>
          </div>
        )}

        {/* ── DETAIL VIEW ── */}
        {view === 'detail' && selected && (
          <div className="animate-fade-up">
            {/* Header card */}
            <div className="uc-detail-header">
              <div className="uc-detail-top">
                <div className="uc-status-dot" style={{ background: STATUS_DOT[selected.status] || STATUS_DOT.disconnected }} />
                <div>
                  <div className="uc-detail-name">{selected.name}</div>
                  <div className="uc-detail-url">{selected.base_url}</div>
                </div>
              </div>
              <div className="uc-detail-meta">
                <span className="uc-auth-badge" style={{ color: AUTH_COLORS[selected.auth_type] }}>
                  {AUTH_LABELS[selected.auth_type]}
                </span>
                <span className="uc-detail-status">{selected.status}</span>
              </div>
            </div>

            {/* Test connection */}
            <div className="uc-detail-section">
              <div className="uc-detail-section-row">
                <span className="panel-section-header" style={{ margin: 0 }}>Connection</span>
                <button
                  className="panel-btn"
                  onClick={() => handleTest(selected.id)}
                  disabled={testing === selected.id}
                >
                  {testing === selected.id ? (
                    <><svg className="spin" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg> Testing...</>
                  ) : 'Test'}
                </button>
              </div>
              {testResult && (
                <div className={`connector-test-result ${testResult.success ? 'success' : 'error'}`}>
                  <strong>{testResult.success ? 'Connected!' : 'Failed'}</strong>
                  <span>{testResult.message}</span>
                  {testResult.details && <span className="connector-test-details">{testResult.details}</span>}
                </div>
              )}
            </div>

            {/* Endpoints */}
            <div className="uc-detail-section">
              <div className="uc-detail-section-row">
                <span className="panel-section-header" style={{ margin: 0 }}>Endpoints ({selected.endpoints.length})</span>
                <div style={{ display: 'flex', gap: 4 }}>
                  <button className="panel-btn" onClick={() => { setDiscoveryResult(null); setFormUrl(selected.base_url); setView('discover'); }}>
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
                    Discover
                  </button>
                  <button className="panel-btn" onClick={() => setShowAddEndpoint(!showAddEndpoint)}>
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
                  </button>
                </div>
              </div>

              {/* Add endpoint form */}
              {showAddEndpoint && (
                <div className="uc-add-ep-form">
                  <div className="uc-add-ep-row">
                    <select className="form-input uc-method-select" value={epMethod} onChange={e => setEpMethod(e.target.value)}>
                      <option>GET</option><option>POST</option><option>PUT</option><option>PATCH</option><option>DELETE</option>
                    </select>
                    <input className="form-input" placeholder="/path" value={epPath} onChange={e => setEpPath(e.target.value)} style={{ flex: 1 }} />
                  </div>
                  <input className="form-input" placeholder="Endpoint key (e.g., list_users)" value={epKey} onChange={e => setEpKey(e.target.value)} />
                  <input className="form-input" placeholder="Description (optional)" value={epDesc} onChange={e => setEpDesc(e.target.value)} />
                  <div style={{ display: 'flex', gap: 6, justifyContent: 'flex-end' }}>
                    <button className="panel-btn" onClick={() => setShowAddEndpoint(false)}>Cancel</button>
                    <button className="panel-btn accent" onClick={handleAddEndpoint} disabled={!epKey.trim() || !epPath.trim()}>Add</button>
                  </div>
                </div>
              )}

              {/* Endpoint list */}
              {selected.endpoints.length === 0 ? (
                <div className="uc-ep-empty">No endpoints. Use Discover or add manually.</div>
              ) : (
                <div className="uc-ep-list">
                  {selected.endpoints.map(ep => (
                    <div key={ep.key} className={`uc-ep-row ${execEndpoint === ep.key ? 'expanded' : ''}`}>
                      <div className="uc-ep-row-main" onClick={() => setExecEndpoint(execEndpoint === ep.key ? null : ep.key)}>
                        <span className="uc-ep-method" style={{ color: METHOD_COLORS[ep.method] || 'var(--text-secondary)' }}>{ep.method}</span>
                        <span className="uc-ep-path">{ep.path}</span>
                        <span className="uc-ep-key">{ep.key}</span>
                        <button className="connector-action-btn danger" onClick={e => { e.stopPropagation(); handleRemoveEndpoint(ep.key); }} title="Remove">
                          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                        </button>
                      </div>
                      {ep.description && <div className="uc-ep-desc">{ep.description}</div>}

                      {/* Execute panel */}
                      {execEndpoint === ep.key && (
                        <div className="uc-exec-panel">
                          <div className="connector-field-group">
                            <label className="connector-field-label">Variables (key=value per line)</label>
                            <textarea
                              className="form-input uc-vars-input"
                              rows={3}
                              placeholder={"user_id=123\nstatus=active"}
                              value={Object.entries(execVars).map(([k,v]) => `${k}=${v}`).join('\n')}
                              onChange={e => {
                                const vars: Record<string, string> = {};
                                e.target.value.split('\n').forEach(line => {
                                  const idx = line.indexOf('=');
                                  if (idx > 0) vars[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
                                });
                                setExecVars(vars);
                              }}
                            />
                          </div>
                          <button
                            className="panel-btn accent"
                            onClick={() => handleExecute(ep.key)}
                            disabled={executing}
                          >
                            {executing ? 'Executing...' : 'Execute'}
                          </button>
                          {execResult && (
                            <div className={`uc-exec-result ${execResult.status >= 200 && execResult.status < 300 ? 'success' : 'error'}`}>
                              <div className="uc-exec-status">Status: {execResult.status}</div>
                              <pre className="uc-exec-body">{execResult.body}</pre>
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Danger zone */}
            <div className="uc-detail-section" style={{ marginTop: 8 }}>
              <button className="link-btn danger" onClick={() => handleDelete(selected.id)}>Delete Connector</button>
            </div>

            {error && <div className="form-error">{error}</div>}
          </div>
        )}

        {/* ── DISCOVER VIEW (for existing connector) ── */}
        {view === 'discover' && selected && (
          <div className="animate-fade-up">
            <div className="connector-field-group">
              <label className="connector-field-label">Discover from URL</label>
              <div className="uc-url-row">
                <input className="form-input" value={formUrl} onChange={e => setFormUrl(e.target.value)} />
                <button className="panel-btn uc-discover-btn" onClick={handleDiscover} disabled={discovering}>
                  {discovering ? (
                    <svg className="spin" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
                  ) : 'Scan'}
                </button>
              </div>
            </div>

            {discoveryResult && (
              <>
                <div className={`uc-discovery-result ${discoveryResult.found ? 'found' : 'empty'}`}>
                  {discoveryResult.found ? (
                    <>
                      <strong>{discoveryResult.title || 'API Found'}</strong>
                      {discoveryResult.spec_type && <span className="uc-spec-badge">{discoveryResult.spec_type}</span>}
                      <span>{discoveryResult.endpoints.length} endpoints found</span>
                    </>
                  ) : (
                    <span>No spec found at this URL.</span>
                  )}
                </div>

                {discoveryResult.found && discoveryResult.endpoints.length > 0 && (
                  <>
                    <div className="uc-discovered-list">
                      {discoveryResult.endpoints.slice(0, 20).map((ep, i) => (
                        <div key={i} className="uc-discovered-ep">
                          <span className="uc-ep-method" style={{ color: METHOD_COLORS[ep.method] }}>{ep.method}</span>
                          <span className="uc-ep-path">{ep.path}</span>
                        </div>
                      ))}
                      {discoveryResult.endpoints.length > 20 && (
                        <div className="uc-ep-empty">+ {discoveryResult.endpoints.length - 20} more</div>
                      )}
                    </div>
                    <button className="panel-btn accent full" onClick={importDiscoveredEndpoints}>
                      Import {discoveryResult.endpoints.length} Endpoints
                    </button>
                  </>
                )}
              </>
            )}

            <button className="panel-btn" onClick={() => setView('detail')} style={{ marginTop: 8 }}>Back to Connector</button>
          </div>
        )}
      </div>
    </div>
  );
}
