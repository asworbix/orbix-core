import { useState, useEffect, useCallback } from 'react';
import type {
  ConnectorDefinition, ConnectorInstance,
  LoginAssistantMessage, ValidationResult,
} from '../types';
import {
  listConnectorCatalog, listConnectorInstances,
  createConnectorInstance, testConnector, removeConnector,
  getLoginHints, askConnectorAssistant, validateConnectorField,
  startOauthLogin, getOauthConnectors,
} from '../hooks/api';
import UniversalConnectorsPanel from './UniversalConnectorsPanel';
import ConnectorWizard from './ConnectorWizard';
import './Panel.css';
import './ConnectorsPanel.css';

interface Props {
  onClose: () => void;
}

type ConnectorTab = 'services' | 'universal';

type View = 'list' | 'catalog' | 'setup';

const STATUS_CONFIG: Record<string, { label: string; color: string; dot: string }> = {
  connected: { label: 'Connected', color: '#34d399', dot: '#34d399' },
  connecting: { label: 'Testing...', color: '#fbbf24', dot: '#fbbf24' },
  error: { label: 'Error', color: '#f87171', dot: '#f87171' },
  expired: { label: 'Expired', color: '#f87171', dot: '#f87171' },
  disconnected: { label: 'Not tested', color: 'var(--text-dim)', dot: 'var(--text-dim)' },
};

const OAUTH_LABELS: Record<string, { label: string; bg: string; color: string }> = {
  microsoft: { label: 'Sign in with Microsoft', bg: '#2f2f2f', color: '#fff' },
  google: { label: 'Sign in with Google', bg: '#fff', color: '#333' },
  github: { label: 'Sign in with GitHub', bg: '#24292e', color: '#fff' },
  slack: { label: 'Sign in with Slack', bg: '#4A154B', color: '#fff' },
  linkedin: { label: 'Sign in with LinkedIn', bg: '#0A66C2', color: '#fff' },
};

export default function ConnectorsPanel({ onClose }: Props) {
  const [connTab, setConnTab] = useState<ConnectorTab>('services');
  const [view, setView] = useState<View>('list');
  const [showWizard, setShowWizard] = useState(false);
  const [catalog, setCatalog] = useState<ConnectorDefinition[]>([]);
  const [instances, setInstances] = useState<ConnectorInstance[]>([]);
  const [oauthConnectors, setOauthConnectors] = useState<string[]>([]);

  // Setup state
  const [selectedDef, setSelectedDef] = useState<ConnectorDefinition | null>(null);
  const [formValues, setFormValues] = useState<Record<string, string>>({});
  const [connName, setConnName] = useState('');
  const [hints, setHints] = useState<LoginAssistantMessage[]>([]);
  const [validations, setValidations] = useState<Record<string, ValidationResult>>({});
  const [assistantMsg, setAssistantMsg] = useState('');
  const [assistantLoading, setAssistantLoading] = useState(false);
  const [activeHintField, setActiveHintField] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<{ success: boolean; message: string; details?: string | null } | null>(null);
  const [error, setError] = useState('');
  const [showManualSetup, setShowManualSetup] = useState(false);
  const [oauthLoading, setOauthLoading] = useState<string | null>(null);

  useEffect(() => {
    listConnectorCatalog().then(setCatalog).catch(() => {});
    listConnectorInstances().then(setInstances).catch(() => {});
    getOauthConnectors().then(setOauthConnectors).catch(() => {});
  }, []);

  const isOauthSupported = (connectorId: string) => oauthConnectors.includes(connectorId);

  const openSetup = useCallback((def: ConnectorDefinition) => {
    setSelectedDef(def);
    setFormValues({});
    setConnName(def.name);
    setValidations({});
    setAssistantMsg('');
    setActiveHintField(null);
    setError('');
    setTestResult(null);
    setShowManualSetup(false);
    setOauthLoading(null);
    setView('setup');
    getLoginHints(def.id).then(setHints).catch(() => {});
  }, []);

  const [setupGuide, setSetupGuide] = useState<string | null>(null);
  const [clientIdInput, setClientIdInput] = useState('');

  const handleSaveClientId = useCallback(async (connectorId: string) => {
    if (!clientIdInput.trim()) return;
    try {
      const { setOauthClientId } = await import('../hooks/api');
      await setOauthClientId(connectorId, clientIdInput.trim());
      setSetupGuide(null);
      setClientIdInput('');
      // Retry OAuth with the new client ID
      handleOauthLogin(connectorId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [clientIdInput]);

  const handleOauthLogin = useCallback(async (connectorId: string) => {
    setOauthLoading(connectorId);
    setError('');
    setSetupGuide(null);
    try {
      const result = await startOauthLogin(connectorId);
      if (result.success) {
        listConnectorInstances().then(setInstances).catch(() => {});
        setView('list');
        setTestResult({
          success: true,
          message: `Connected as ${result.user_name || result.user_email || 'user'}`,
        });
      } else {
        const err = result.error || 'Login failed';
        if (err.startsWith('SETUP_GUIDE:')) {
          setSetupGuide(err.replace('SETUP_GUIDE:', ''));
        } else {
          setError(err);
          if (err.includes('client ID not configured')) {
            setShowManualSetup(true);
          }
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('SETUP_GUIDE:')) {
        setSetupGuide(msg.replace('SETUP_GUIDE:', ''));
      } else {
        setError(msg);
        if (msg.includes('client ID not configured')) {
          setShowManualSetup(true);
        }
      }
    }
    setOauthLoading(null);
  }, []);

  const handleFieldChange = useCallback((key: string, value: string) => {
    setFormValues(prev => ({ ...prev, [key]: value }));
  }, []);

  const handleValidate = useCallback(async (fieldKey: string) => {
    if (!selectedDef) return;
    const value = formValues[fieldKey] || '';
    const result = await validateConnectorField(selectedDef.id, fieldKey, value);
    setValidations(prev => ({ ...prev, [fieldKey]: result }));
  }, [selectedDef, formValues]);

  const handleAskAssistant = useCallback(async (fieldKey: string) => {
    if (!selectedDef) return;
    setAssistantLoading(true);
    setActiveHintField(fieldKey);
    try {
      const msg = await askConnectorAssistant(selectedDef.id, fieldKey, formValues[fieldKey] || '');
      setAssistantMsg(msg);
    } catch {
      setAssistantMsg('Unable to get help right now.');
    }
    setAssistantLoading(false);
  }, [selectedDef, formValues]);

  const handleSave = useCallback(async () => {
    if (!selectedDef) return;
    setSaving(true);
    setError('');
    try {
      const inst = await createConnectorInstance(selectedDef.id, connName, formValues);
      setInstances(prev => [...prev, inst]);

      // Auto-test
      setTesting(inst.id);
      try {
        const result = await testConnector(inst.id);
        setTestResult(result);
        listConnectorInstances().then(setInstances).catch(() => {});
      } catch {
        setTestResult({ success: false, message: 'Test failed' });
      }
      setTesting(null);
      setView('list');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setSaving(false);
  }, [selectedDef, connName, formValues]);

  const handleTest = useCallback(async (instanceId: string) => {
    setTesting(instanceId);
    setTestResult(null);
    try {
      const result = await testConnector(instanceId);
      setTestResult(result);
      listConnectorInstances().then(setInstances).catch(() => {});
    } catch {
      setTestResult({ success: false, message: 'Test failed' });
    }
    setTesting(null);
  }, []);

  const handleRemove = useCallback(async (instanceId: string) => {
    await removeConnector(instanceId).catch(() => {});
    setInstances(prev => prev.filter(i => i.id !== instanceId));
  }, []);

  const allRequiredFilled = selectedDef?.fields
    .filter(f => f.required)
    .every(f => (formValues[f.key] || '').trim().length > 0) ?? false;

  // If universal tab is active, render the standalone universal connectors panel
  if (connTab === 'universal') {
    return (
      <div className="panel">
        <div className="connector-tabs" style={{ borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
          <button className={`connector-tab`} onClick={() => setConnTab('services')}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
            Services
          </button>
          <button className={`connector-tab active`}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83"/></svg>
            Universal
          </button>
        </div>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
          <UniversalConnectorsPanel onClose={onClose} />
        </div>
      </div>
    );
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
            <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
          </svg>
          {view === 'setup' ? 'Connect Service' : 'Connectors'}
        </div>
        <div className="panel-header-actions">
          {view !== 'list' && (
            <button className="panel-close-btn" onClick={() => setView('list')} title="Back">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="15 18 9 12 15 6" />
              </svg>
            </button>
          )}
          {view === 'list' && (
            <button
              className="panel-close-btn"
              onClick={() => setShowWizard(true)}
              title="Add module connector"
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
              </svg>
            </button>
          )}
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      {/* Tab switcher */}
      <div className="connector-tabs">
        <button className="connector-tab active" onClick={() => setConnTab('services')}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
          Services
        </button>
        <button className="connector-tab" onClick={() => setConnTab('universal')}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/><path d="M12 1v4M12 19v4M4.22 4.22l2.83 2.83M16.95 16.95l2.83 2.83M1 12h4M19 12h4M4.22 19.78l2.83-2.83M16.95 7.05l2.83-2.83"/></svg>
          Universal
        </button>
      </div>

      <div className="panel-body">
        {/* ── Connected Services List ── */}
        {view === 'list' && (
          <>
            {instances.length > 0 && (
              <>
                <div className="panel-section-header">Connected Services</div>
                {instances.map(inst => {
                  const statusCfg = STATUS_CONFIG[inst.status] || STATUS_CONFIG.disconnected;
                  return (
                    <div key={inst.id} className="connector-instance-row">
                      <div className="connector-instance-info">
                        <div className="connector-instance-dot" style={{ background: statusCfg.dot }} />
                        <div>
                          <div className="connector-instance-name">{inst.name}</div>
                          <div className="connector-instance-status" style={{ color: statusCfg.color }}>
                            {statusCfg.label}
                            {inst.error_message && inst.status === 'error' && (
                              <span className="connector-error-hint"> - {inst.error_message}</span>
                            )}
                          </div>
                        </div>
                      </div>
                      <div className="connector-instance-actions">
                        <button
                          className="connector-action-btn"
                          onClick={() => handleTest(inst.id)}
                          disabled={testing === inst.id}
                          title="Test connection"
                        >
                          {testing === inst.id ? (
                            <svg className="spin" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                            </svg>
                          ) : (
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                              <polyline points="20 6 9 17 4 12" />
                            </svg>
                          )}
                        </button>
                        <button
                          className="connector-action-btn danger"
                          onClick={() => handleRemove(inst.id)}
                          title="Remove"
                        >
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                          </svg>
                        </button>
                      </div>
                    </div>
                  );
                })}
              </>
            )}

            <div className="panel-section-header" style={instances.length > 0 ? { marginTop: 16 } : undefined}>
              Add Service
            </div>
            <div className="connector-catalog-grid">
              {catalog.map(def => (
                <button
                  key={def.id}
                  className="connector-catalog-card"
                  onClick={() => openSetup(def)}
                >
                  <span className="connector-catalog-icon">{def.icon}</span>
                  <span className="connector-catalog-name">{def.name}</span>
                  <span className="connector-catalog-desc">{def.description}</span>
                </button>
              ))}
            </div>
          </>
        )}

        {/* ── Setup / Login Flow ── */}
        {view === 'setup' && selectedDef && (
          <>
            <div className="connector-setup-header">
              <span className="connector-setup-icon">{selectedDef.icon}</span>
              <div>
                <div className="connector-setup-title">{selectedDef.name}</div>
                <div className="connector-setup-desc">{selectedDef.description}</div>
              </div>
            </div>

            {/* ── OAuth Quick Login (primary path) ── */}
            {isOauthSupported(selectedDef.id) && !showManualSetup && (
              <div className="connector-oauth-section">
                {/* Setup guide - shown when provider needs client ID registration */}
                {setupGuide ? (
                  <div className="connector-setup-guide">
                    <div className="connector-guide-text">
                      {setupGuide.split('\n').map((line, i) => (
                        <p key={i}>{line}</p>
                      ))}
                    </div>
                    <div className="connector-guide-input">
                      <input
                        className="form-input"
                        placeholder="Paste your Client ID here"
                        value={clientIdInput}
                        onChange={e => setClientIdInput(e.target.value)}
                        onKeyDown={e => e.key === 'Enter' && handleSaveClientId(selectedDef.id)}
                      />
                      <button
                        className="connector-guide-save"
                        onClick={() => handleSaveClientId(selectedDef.id)}
                        disabled={!clientIdInput.trim()}
                      >
                        Connect
                      </button>
                    </div>
                    <button
                      className="connector-manual-link"
                      onClick={() => { setSetupGuide(null); setShowManualSetup(true); }}
                    >
                      Or set up manually with full credentials
                    </button>
                  </div>
                ) : (
                  <>
                    <button
                      className="connector-oauth-btn"
                      style={{
                        background: OAUTH_LABELS[selectedDef.id]?.bg || '#333',
                        color: OAUTH_LABELS[selectedDef.id]?.color || '#fff',
                      }}
                      onClick={() => handleOauthLogin(selectedDef.id)}
                      disabled={oauthLoading !== null}
                    >
                      {oauthLoading === selectedDef.id ? (
                        <svg className="spin" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                        </svg>
                      ) : (
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" />
                          <polyline points="10 17 15 12 10 7" />
                          <line x1="15" y1="12" x2="3" y2="12" />
                        </svg>
                      )}
                      {oauthLoading === selectedDef.id
                        ? 'Opening browser...'
                        : OAUTH_LABELS[selectedDef.id]?.label || `Sign in with ${selectedDef.name}`
                      }
                    </button>

                    <div className="connector-oauth-hint">
                      Your browser will open for secure login. No passwords are stored in Orbix Core.
                    </div>

                    {error && <div className="form-error">{error}</div>}

                    <div className="connector-oauth-divider">
                      <span>or</span>
                    </div>

                    <button
                      className="connector-manual-link"
                      onClick={() => setShowManualSetup(true)}
                    >
                      Advanced: set up manually with API credentials
                    </button>
                  </>
                )}
              </div>
            )}

            {/* ── Manual Setup (fallback or for non-OAuth connectors) ── */}
            {(!isOauthSupported(selectedDef.id) || showManualSetup) && (
              <div className="connector-setup-form">
                {showManualSetup && isOauthSupported(selectedDef.id) && (
                  <button
                    className="connector-manual-link"
                    onClick={() => { setShowManualSetup(false); setError(''); }}
                    style={{ marginBottom: 12 }}
                  >
                    Back to quick sign in
                  </button>
                )}

                <div className="connector-field-group">
                  <label className="connector-field-label">Connection Name</label>
                  <input
                    className="form-input"
                    value={connName}
                    onChange={e => setConnName(e.target.value)}
                    placeholder="e.g., My Company Microsoft"
                  />
                </div>

                {selectedDef.fields.map(field => {
                  const validation = validations[field.key];
                  const hint = hints.find(h => h.field_key === field.key);
                  return (
                    <div key={field.key} className="connector-field-group">
                      <div className="connector-field-label-row">
                        <label className="connector-field-label">
                          {field.label}
                          {field.required && <span className="connector-required">*</span>}
                        </label>
                        <button
                          className="connector-help-btn"
                          onClick={() => handleAskAssistant(field.key)}
                          title="Get help from AI assistant"
                        >
                          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <circle cx="12" cy="12" r="10" />
                            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
                            <line x1="12" y1="17" x2="12.01" y2="17" />
                          </svg>
                        </button>
                      </div>

                      <input
                        className={`form-input ${validation?.valid === false ? 'error' : validation?.valid === true ? 'success' : ''}`}
                        type={field.field_type === 'password' ? 'password' : 'text'}
                        placeholder={field.placeholder}
                        value={formValues[field.key] || ''}
                        onChange={e => handleFieldChange(field.key, e.target.value)}
                        onBlur={() => handleValidate(field.key)}
                      />

                      {validation && (
                        <div className={`connector-validation ${validation.valid ? 'valid' : 'invalid'}`}>
                          {validation.valid ? '\u2713' : '\u2717'} {validation.message}
                        </div>
                      )}

                      {hint && !activeHintField && (
                        <div className="connector-hint">{field.help_text}</div>
                      )}

                      {activeHintField === field.key && (
                        <div className="connector-assistant-bubble">
                          {assistantLoading ? (
                            <span className="connector-assistant-loading">AI is thinking...</span>
                          ) : (
                            assistantMsg
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}

                {selectedDef.docs_url && (
                  <div className="connector-docs-link">
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                    <span className="connector-docs-text">
                      View {selectedDef.name} setup documentation
                    </span>
                  </div>
                )}

                {error && <div className="form-error">{error}</div>}

                {testResult && (
                  <div className={`connector-test-result ${testResult.success ? 'success' : 'error'}`}>
                    <strong>{testResult.success ? 'Connected!' : 'Connection failed'}</strong>
                    <span>{testResult.message}</span>
                    {testResult.details && <span className="connector-test-details">{testResult.details}</span>}
                  </div>
                )}

                <div className="connector-setup-actions">
                  <button className="panel-btn" onClick={() => setView('list')}>Cancel</button>
                  <button
                    className="panel-btn accent"
                    onClick={handleSave}
                    disabled={saving || !allRequiredFilled || !connName.trim()}
                  >
                    {saving ? 'Connecting...' : 'Connect & Test'}
                  </button>
                </div>
              </div>
            )}
          </>
        )}
      </div>
      {showWizard && <ConnectorWizard onClose={() => setShowWizard(false)} />}
    </div>
  );
}
