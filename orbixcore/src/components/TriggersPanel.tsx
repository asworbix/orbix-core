import { useState, useEffect, useCallback } from 'react';
import type {
  EmailAccount, TriggerRule, WebhookEndpoint, FileWatch,
  EventLogEntry, Workflow, RuleCondition, RuleConditionType,
  RuleLogic, TriggerSource, FileWatchEvent,
} from '../types';
import {
  listEmailAccounts, removeEmailAccount,
  emailDeviceStart, emailDeviceComplete,
  listTriggerRules, createTriggerRule, updateTriggerRule, deleteTriggerRule,
  listWebhooks, createWebhook, deleteWebhook,
  listFileWatches, createFileWatch, deleteFileWatch,
  getEventLog, clearEventLog, pollEmailsNow, testEmailConnection,
  listWorkflows,
} from '../hooks/api';
import './TriggersPanel.css';

type Tab = 'rules' | 'email' | 'webhooks' | 'watches' | 'log';

interface Props {
  onClose: () => void;
}

export default function TriggersPanel({ onClose }: Props) {
  const [tab, setTab] = useState<Tab>('rules');
  const [emailAccounts, setEmailAccounts] = useState<EmailAccount[]>([]);
  const [rules, setRules] = useState<TriggerRule[]>([]);
  const [webhooks, setWebhooks] = useState<WebhookEndpoint[]>([]);
  const [watches, setWatches] = useState<FileWatch[]>([]);
  const [eventLog, setEventLog] = useState<EventLogEntry[]>([]);
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  // Form states
  const [showRuleForm, setShowRuleForm] = useState(false);
  const [showEmailForm, setShowEmailForm] = useState(false);
  const [showWebhookForm, setShowWebhookForm] = useState(false);
  const [showWatchForm, setShowWatchForm] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [accts, rls, whs, fws, log, wfs] = await Promise.all([
        listEmailAccounts(),
        listTriggerRules(),
        listWebhooks(),
        listFileWatches(),
        getEventLog(),
        listWorkflows(),
      ]);
      setEmailAccounts(accts);
      setRules(rls);
      setWebhooks(whs);
      setWatches(fws);
      setEventLog(log);
      setWorkflows(wfs);
    } catch (e) { console.error(e); }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const tabs: { id: Tab; label: string; count?: number }[] = [
    { id: 'rules', label: 'Rules', count: rules.length },
    { id: 'email', label: 'Email', count: emailAccounts.length },
    { id: 'webhooks', label: 'Webhooks', count: webhooks.length },
    { id: 'watches', label: 'File Watch', count: watches.length },
    { id: 'log', label: 'Event Log', count: eventLog.length },
  ];

  return (
    <div className="panel-overlay animate-slide-right">
      <div className="triggers-header">
        <div className="triggers-title-row">
          <h2>Event Triggers</h2>
          <button className="panel-close-btn" onClick={onClose}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>
        <div className="triggers-tabs">
          {tabs.map(t => (
            <button
              key={t.id}
              className={`trigger-tab ${tab === t.id ? 'active' : ''}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
              {t.count !== undefined && t.count > 0 && (
                <span className="trigger-tab-count">{t.count}</span>
              )}
            </button>
          ))}
        </div>
      </div>

      <div className="triggers-body">
        {tab === 'rules' && (
          <RulesTab
            rules={rules}
            workflows={workflows}
            onRefresh={refresh}
            showForm={showRuleForm}
            setShowForm={setShowRuleForm}
          />
        )}
        {tab === 'email' && (
          <EmailTab
            accounts={emailAccounts}
            onRefresh={refresh}
            showForm={showEmailForm}
            setShowForm={setShowEmailForm}
          />
        )}
        {tab === 'webhooks' && (
          <WebhooksTab
            webhooks={webhooks}
            workflows={workflows}
            onRefresh={refresh}
            showForm={showWebhookForm}
            setShowForm={setShowWebhookForm}
          />
        )}
        {tab === 'watches' && (
          <WatchesTab
            watches={watches}
            workflows={workflows}
            onRefresh={refresh}
            showForm={showWatchForm}
            setShowForm={setShowWatchForm}
          />
        )}
        {tab === 'log' && (
          <EventLogTab log={eventLog} onRefresh={refresh} />
        )}
      </div>
    </div>
  );
}

// ── Rules Tab ────────────────────────────────

function RulesTab({
  rules, workflows, onRefresh, showForm, setShowForm,
}: {
  rules: TriggerRule[];
  workflows: Workflow[];
  onRefresh: () => void;
  showForm: boolean;
  setShowForm: (v: boolean) => void;
}) {
  const [name, setName] = useState('');
  const [source, setSource] = useState<TriggerSource>('email');
  const [workflowId, setWorkflowId] = useState('');
  const [logic, setLogic] = useState<RuleLogic>('and');
  const [conditions, setConditions] = useState<RuleCondition[]>([
    { condition_type: 'sender_contains', value: '' },
  ]);

  const handleCreate = async () => {
    if (!name.trim() || !workflowId) return;
    const validConds = conditions.filter(c => c.value.trim());
    if (validConds.length === 0) return;
    await createTriggerRule(name, source, workflowId, validConds, logic);
    setShowForm(false);
    setName(''); setConditions([{ condition_type: 'sender_contains', value: '' }]);
    onRefresh();
  };

  const handleToggle = async (rule: TriggerRule) => {
    await updateTriggerRule(rule.id, { enabled: !rule.enabled });
    onRefresh();
  };

  const handleDelete = async (id: string) => {
    await deleteTriggerRule(id);
    onRefresh();
  };

  const addCondition = () => {
    setConditions([...conditions, { condition_type: 'sender_contains', value: '' }]);
  };

  const updateCondition = (i: number, field: keyof RuleCondition, val: string) => {
    const next = [...conditions];
    if (field === 'condition_type') {
      next[i] = { ...next[i], condition_type: val as RuleConditionType };
    } else {
      next[i] = { ...next[i], value: val };
    }
    setConditions(next);
  };

  const removeCondition = (i: number) => {
    setConditions(conditions.filter((_, idx) => idx !== i));
  };

  const getWorkflowName = (id: string) =>
    workflows.find(w => w.id === id)?.name || 'Unknown';

  const conditionLabel = (type: RuleConditionType) => {
    const map: Record<RuleConditionType, string> = {
      sender_contains: 'Sender contains',
      subject_contains: 'Subject contains',
      body_contains: 'Body contains',
      has_label: 'Has label',
      sender_equals: 'Sender equals',
      subject_matches: 'Subject matches (regex)',
    };
    return map[type] || type;
  };

  return (
    <div className="trigger-section">
      {!showForm && (
        <button className="trigger-add-btn" onClick={() => setShowForm(true)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
          New Rule
        </button>
      )}

      {showForm && (
        <div className="trigger-form">
          <div className="trigger-form-title">Create Rule</div>
          <input
            className="trigger-input"
            placeholder="Rule name..."
            value={name}
            onChange={e => setName(e.target.value)}
          />
          <div className="trigger-form-row">
            <label>Source</label>
            <select className="trigger-select" value={source} onChange={e => setSource(e.target.value as TriggerSource)}>
              <option value="email">Email</option>
              <option value="webhook">Webhook</option>
              <option value="file_watch">File Watch</option>
            </select>
          </div>
          <div className="trigger-form-row">
            <label>Workflow</label>
            <select className="trigger-select" value={workflowId} onChange={e => setWorkflowId(e.target.value)}>
              <option value="">Select workflow...</option>
              {workflows.map(w => (
                <option key={w.id} value={w.id}>{w.name}</option>
              ))}
            </select>
          </div>
          <div className="trigger-form-row">
            <label>Logic</label>
            <div className="trigger-logic-toggle">
              <button className={logic === 'and' ? 'active' : ''} onClick={() => setLogic('and')}>AND</button>
              <button className={logic === 'or' ? 'active' : ''} onClick={() => setLogic('or')}>OR</button>
            </div>
          </div>
          <div className="trigger-conditions">
            <label>Conditions</label>
            {conditions.map((cond, i) => (
              <div key={i} className="trigger-condition-row">
                <select
                  className="trigger-select small"
                  value={cond.condition_type}
                  onChange={e => updateCondition(i, 'condition_type', e.target.value)}
                >
                  <option value="sender_contains">Sender contains</option>
                  <option value="subject_contains">Subject contains</option>
                  <option value="body_contains">Body contains</option>
                  <option value="has_label">Has label</option>
                  <option value="sender_equals">Sender equals</option>
                  <option value="subject_matches">Subject regex</option>
                </select>
                <input
                  className="trigger-input small"
                  placeholder="Value..."
                  value={cond.value}
                  onChange={e => updateCondition(i, 'value', e.target.value)}
                />
                {conditions.length > 1 && (
                  <button className="trigger-remove-cond" onClick={() => removeCondition(i)}>
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                    </svg>
                  </button>
                )}
              </div>
            ))}
            <button className="trigger-add-cond" onClick={addCondition}>+ Add condition</button>
          </div>
          <div className="trigger-form-actions">
            <button className="trigger-btn secondary" onClick={() => setShowForm(false)}>Cancel</button>
            <button className="trigger-btn primary" onClick={handleCreate}>Create Rule</button>
          </div>
        </div>
      )}

      {rules.length === 0 && !showForm && (
        <div className="trigger-empty">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" opacity="0.3">
            <path d="M22 12h-4l-3 9L9 3l-3 9H2"/>
          </svg>
          <p>No rules yet. Create one to auto-route events to workflows.</p>
        </div>
      )}

      {rules.map(rule => (
        <div key={rule.id} className={`trigger-rule-card ${rule.enabled ? '' : 'disabled'}`}>
          <div className="trigger-rule-header">
            <div className="trigger-rule-info">
              <span className="trigger-rule-name">{rule.name}</span>
              <span className={`trigger-source-badge ${rule.source}`}>{rule.source}</span>
            </div>
            <div className="trigger-rule-actions">
              <button
                className={`trigger-toggle ${rule.enabled ? 'on' : 'off'}`}
                onClick={() => handleToggle(rule)}
                title={rule.enabled ? 'Disable' : 'Enable'}
              >
                <div className="toggle-track">
                  <div className="toggle-thumb"/>
                </div>
              </button>
              <button className="trigger-delete-btn" onClick={() => handleDelete(rule.id)}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
                  <path d="M10 11v6"/><path d="M14 11v6"/>
                </svg>
              </button>
            </div>
          </div>
          <div className="trigger-rule-details">
            <span className="trigger-rule-logic">{rule.logic.toUpperCase()}</span>
            {rule.conditions.map((c, i) => (
              <span key={i} className="trigger-condition-pill">
                {conditionLabel(c.condition_type)}: {c.value}
              </span>
            ))}
          </div>
          <div className="trigger-rule-workflow">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polygon points="5 3 19 12 5 21 5 3"/>
            </svg>
            {getWorkflowName(rule.workflow_id)}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Email Tab ────────────────────────────────

function EmailTab({
  accounts, onRefresh, showForm, setShowForm,
}: {
  accounts: EmailAccount[];
  onRefresh: () => void;
  showForm: boolean;
  setShowForm: (v: boolean) => void;
}) {
  const [provider, setProvider] = useState<'gmail' | 'outlook'>('gmail');
  const [polling, setPolling] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [signin, setSignin] = useState<{ code: string; uri: string; waiting: boolean; error?: string } | null>(null);

  const handleSignIn = async () => {
    setSignin(null);
    try {
      const d = await emailDeviceStart(provider === 'gmail' ? 'google' : 'microsoft');
      try { await navigator.clipboard.writeText(d.user_code); } catch { /* clipboard optional */ }
      setSignin({ code: d.user_code, uri: d.verification_uri, waiting: true });
      // Blocks until the user finishes in the browser (or the code expires).
      await emailDeviceComplete(d.connector_id, d.device_code);
      setSignin(null);
      setShowForm(false);
      onRefresh();
    } catch (e) {
      setSignin(prev => prev ? { ...prev, waiting: false, error: String(e) } : { code: '', uri: '', waiting: false, error: String(e) });
    }
  };

  const handleRemove = async (id: string) => {
    await removeEmailAccount(id);
    onRefresh();
  };

  const handlePoll = async () => {
    setPolling(true);
    try {
      await pollEmailsNow();
      onRefresh();
    } catch (e) { console.error(e); }
    setPolling(false);
  };

  const handleTest = async (id: string) => {
    setTestResult(null);
    try {
      const msg = await testEmailConnection(id);
      setTestResult(msg);
    } catch (e) {
      setTestResult(typeof e === 'string' ? e : 'Connection failed');
    }
  };

  const statusDot = (status: string) => {
    const colors: Record<string, string> = {
      connected: 'var(--success)',
      disconnected: 'var(--text-dim)',
      expired: 'var(--warning)',
      error: 'var(--error)',
    };
    return <span className="status-dot" style={{ background: colors[status] || colors.disconnected }}/>;
  };

  return (
    <div className="trigger-section">
      <div className="trigger-section-header">
        <button className="trigger-add-btn" onClick={() => setShowForm(true)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
          Add Account
        </button>
        {accounts.length > 0 && (
          <button className="trigger-btn secondary small" onClick={handlePoll} disabled={polling}>
            {polling ? 'Polling...' : 'Poll Now'}
          </button>
        )}
      </div>

      {showForm && (
        <div className="trigger-form">
          <div className="trigger-form-title">Connect an email account</div>
          <div className="trigger-form-row">
            <label>Provider</label>
            <select className="trigger-select" value={provider} onChange={e => setProvider(e.target.value as 'gmail' | 'outlook')}>
              <option value="outlook">Outlook / Microsoft 365</option>
              <option value="gmail">Gmail</option>
            </select>
          </div>
          {!signin && (
            <>
              <p className="trigger-hint">
                One click: your browser opens the sign-in page and the pairing code is copied for you.
                Mail, calendar and triggers all use this one sign-in.
              </p>
              <div className="trigger-form-actions">
                <button className="trigger-btn secondary" onClick={() => setShowForm(false)}>Cancel</button>
                <button className="trigger-btn primary" onClick={handleSignIn}>
                  Sign in with {provider === 'gmail' ? 'Google' : 'Microsoft'}
                </button>
              </div>
            </>
          )}
          {signin && (
            <div className="trigger-signin-card">
              {signin.error ? (
                <>
                  <p className="trigger-hint" style={{ color: 'var(--error)' }}>{signin.error}</p>
                  <div className="trigger-form-actions">
                    <button className="trigger-btn secondary" onClick={() => setSignin(null)}>Back</button>
                    <button className="trigger-btn primary" onClick={handleSignIn}>Try again</button>
                  </div>
                </>
              ) : (
                <>
                  <p className="trigger-hint">
                    Paste this code in the browser tab that just opened (it's already copied):
                  </p>
                  <div style={{ fontSize: 24, fontWeight: 700, letterSpacing: '0.15em', textAlign: 'center', padding: '10px 0', fontFamily: 'monospace' }}>
                    {signin.code}
                  </div>
                  <p className="trigger-hint" style={{ textAlign: 'center' }}>
                    {signin.waiting ? 'Waiting for you to finish signing in…' : ''}
                  </p>
                  <div className="trigger-form-actions">
                    <button className="trigger-btn secondary" onClick={() => setSignin(null)}>Cancel</button>
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {testResult && (
        <div className="trigger-test-result">{testResult}</div>
      )}

      {accounts.length === 0 && !showForm && (
        <div className="trigger-empty">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" opacity="0.3">
            <rect x="2" y="4" width="20" height="16" rx="2"/><path d="M22 7l-10 7L2 7"/>
          </svg>
          <p>Connect Gmail or Outlook to monitor incoming emails.</p>
        </div>
      )}

      {accounts.map(acct => (
        <div key={acct.id} className="trigger-email-card">
          <div className="trigger-email-info">
            {statusDot(acct.status)}
            <div>
              <div className="trigger-email-addr">{acct.email}</div>
              <div className="trigger-email-meta">
                {acct.provider === 'gmail' ? 'Gmail' : 'Outlook'}
                {acct.last_polled && ` · Last polled ${new Date(acct.last_polled).toLocaleTimeString()}`}
              </div>
            </div>
          </div>
          <div className="trigger-email-actions">
            <button className="trigger-btn secondary small" onClick={() => handleTest(acct.id)}>Test</button>
            <button className="trigger-delete-btn" onClick={() => handleRemove(acct.id)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
              </svg>
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Webhooks Tab ─────────────────────────────

function WebhooksTab({
  webhooks, workflows, onRefresh, showForm, setShowForm,
}: {
  webhooks: WebhookEndpoint[];
  workflows: Workflow[];
  onRefresh: () => void;
  showForm: boolean;
  setShowForm: (v: boolean) => void;
}) {
  const [name, setName] = useState('');
  const [workflowId, setWorkflowId] = useState('');

  const handleCreate = async () => {
    if (!name.trim() || !workflowId) return;
    await createWebhook(name, workflowId);
    setShowForm(false);
    setName('');
    onRefresh();
  };

  const handleDelete = async (id: string) => {
    await deleteWebhook(id);
    onRefresh();
  };

  return (
    <div className="trigger-section">
      {!showForm && (
        <button className="trigger-add-btn" onClick={() => setShowForm(true)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
          New Webhook
        </button>
      )}

      {showForm && (
        <div className="trigger-form">
          <div className="trigger-form-title">Create Webhook</div>
          <input className="trigger-input" placeholder="Webhook name..." value={name} onChange={e => setName(e.target.value)} />
          <div className="trigger-form-row">
            <label>Workflow</label>
            <select className="trigger-select" value={workflowId} onChange={e => setWorkflowId(e.target.value)}>
              <option value="">Select workflow...</option>
              {workflows.map(w => <option key={w.id} value={w.id}>{w.name}</option>)}
            </select>
          </div>
          <div className="trigger-form-actions">
            <button className="trigger-btn secondary" onClick={() => setShowForm(false)}>Cancel</button>
            <button className="trigger-btn primary" onClick={handleCreate}>Create</button>
          </div>
        </div>
      )}

      {webhooks.length === 0 && !showForm && (
        <div className="trigger-empty">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" opacity="0.3">
            <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
            <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
          </svg>
          <p>Create webhook endpoints that trigger workflows on incoming POST requests.</p>
        </div>
      )}

      {webhooks.map(wh => (
        <div key={wh.id} className="trigger-webhook-card">
          <div className="trigger-webhook-info">
            <div className="trigger-webhook-name">{wh.name}</div>
            <code className="trigger-webhook-path">{wh.path}</code>
            <div className="trigger-webhook-meta">
              {wh.received_count} received · {workflows.find(w => w.id === wh.workflow_id)?.name || 'No workflow'}
            </div>
          </div>
          <button className="trigger-delete-btn" onClick={() => handleDelete(wh.id)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
            </svg>
          </button>
        </div>
      ))}
    </div>
  );
}

// ── File Watch Tab ───────────────────────────

function WatchesTab({
  watches, workflows, onRefresh, showForm, setShowForm,
}: {
  watches: FileWatch[];
  workflows: Workflow[];
  onRefresh: () => void;
  showForm: boolean;
  setShowForm: (v: boolean) => void;
}) {
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [workflowId, setWorkflowId] = useState('');
  const [eventFilter, setEventFilter] = useState<FileWatchEvent>('any');

  const handleCreate = async () => {
    if (!name.trim() || !path.trim() || !workflowId) return;
    await createFileWatch(name, path, workflowId, true, eventFilter);
    setShowForm(false);
    setName(''); setPath('');
    onRefresh();
  };

  const handleDelete = async (id: string) => {
    await deleteFileWatch(id);
    onRefresh();
  };

  const eventLabel = (e: FileWatchEvent) => {
    const map: Record<FileWatchEvent, string> = {
      any: 'Any change', created: 'Created', modified: 'Modified', deleted: 'Deleted',
    };
    return map[e];
  };

  return (
    <div className="trigger-section">
      {!showForm && (
        <button className="trigger-add-btn" onClick={() => setShowForm(true)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
          New File Watch
        </button>
      )}

      {showForm && (
        <div className="trigger-form">
          <div className="trigger-form-title">Watch Directory</div>
          <input className="trigger-input" placeholder="Watch name..." value={name} onChange={e => setName(e.target.value)} />
          <input className="trigger-input" placeholder="/path/to/watch..." value={path} onChange={e => setPath(e.target.value)} />
          <div className="trigger-form-row">
            <label>Event</label>
            <select className="trigger-select" value={eventFilter} onChange={e => setEventFilter(e.target.value as FileWatchEvent)}>
              <option value="any">Any change</option>
              <option value="created">File created</option>
              <option value="modified">File modified</option>
              <option value="deleted">File deleted</option>
            </select>
          </div>
          <div className="trigger-form-row">
            <label>Workflow</label>
            <select className="trigger-select" value={workflowId} onChange={e => setWorkflowId(e.target.value)}>
              <option value="">Select workflow...</option>
              {workflows.map(w => <option key={w.id} value={w.id}>{w.name}</option>)}
            </select>
          </div>
          <div className="trigger-form-actions">
            <button className="trigger-btn secondary" onClick={() => setShowForm(false)}>Cancel</button>
            <button className="trigger-btn primary" onClick={handleCreate}>Create</button>
          </div>
        </div>
      )}

      {watches.length === 0 && !showForm && (
        <div className="trigger-empty">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" opacity="0.3">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
          <p>Watch directories for file changes that trigger workflows.</p>
        </div>
      )}

      {watches.map(fw => (
        <div key={fw.id} className="trigger-watch-card">
          <div className="trigger-watch-info">
            <div className="trigger-watch-name">{fw.name}</div>
            <code className="trigger-watch-path">{fw.path}</code>
            <div className="trigger-watch-meta">
              {eventLabel(fw.event_filter)} · {fw.recursive ? 'Recursive' : 'Top-level'}
              {' · '}{workflows.find(w => w.id === fw.workflow_id)?.name || 'No workflow'}
            </div>
          </div>
          <button className="trigger-delete-btn" onClick={() => handleDelete(fw.id)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
            </svg>
          </button>
        </div>
      ))}
    </div>
  );
}

// ── Event Log Tab ────────────────────────────

function EventLogTab({ log, onRefresh }: { log: EventLogEntry[]; onRefresh: () => void }) {
  const handleClear = async () => {
    await clearEventLog();
    onRefresh();
  };

  const statusIcon = (status: string) => {
    switch (status) {
      case 'completed': return <span className="log-status-icon success">*</span>;
      case 'failed': return <span className="log-status-icon error">*</span>;
      case 'running': return <span className="log-status-icon running">*</span>;
      case 'skipped': return <span className="log-status-icon dim">*</span>;
      default: return <span className="log-status-icon accent">*</span>;
    }
  };

  return (
    <div className="trigger-section">
      {log.length > 0 && (
        <div className="trigger-section-header">
          <span className="trigger-section-count">{log.length} events</span>
          <button className="trigger-btn secondary small" onClick={handleClear}>Clear Log</button>
        </div>
      )}

      {log.length === 0 && (
        <div className="trigger-empty">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" opacity="0.3">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <path d="M14 2v6h6"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/>
          </svg>
          <p>No events yet. Events appear here when triggers fire.</p>
        </div>
      )}

      {log.map(entry => (
        <div key={entry.id} className="trigger-log-entry">
          <div className="trigger-log-header">
            {statusIcon(entry.status)}
            <span className={`trigger-source-badge ${entry.source}`}>{entry.source}</span>
            <span className="trigger-log-time">
              {new Date(entry.timestamp).toLocaleString()}
            </span>
          </div>
          <div className="trigger-log-message">{entry.message}</div>
          {entry.source_detail && (
            <div className="trigger-log-detail">{entry.source_detail}</div>
          )}
          {entry.rule_name && (
            <div className="trigger-log-rule">Rule: {entry.rule_name}</div>
          )}
        </div>
      ))}
    </div>
  );
}
