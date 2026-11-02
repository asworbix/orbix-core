import { useState, useEffect } from 'react';
import type { McpServer, McpServerInput, McpTransport, CliTemplate } from '../types';
import {
  listMcpServers, addMcpServer, removeMcpServer, toggleMcpServer,
  generateCliTemplate
} from '../hooks/api';
import './Panel.css';
import './McpPanel.css';

interface Props {
  servers: McpServer[];
  setServers: (servers: McpServer[]) => void;
  onClose: () => void;
}

type Tab = 'servers' | 'cli';
type McpView = 'list' | 'add';

const STATUS_COLORS = {
  running: '#34d399',
  stopped: '#4a5568',
  error:   '#f87171',
  unknown: '#fbbf24',
};

const emptyMcpForm = (): McpServerInput => ({
  name: '',
  transport: 'stdio',
  command: '',
  args: [],
  env: {},
  url: undefined,
});

export default function McpPanel({ servers, setServers, onClose }: Props) {
  const [tab, setTab] = useState<Tab>('servers');
  const [mcpView, setMcpView] = useState<McpView>('list');
  const [form, setForm] = useState<McpServerInput>(emptyMcpForm());
  const [argsText, setArgsText] = useState('');
  const [envText, setEnvText] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  // CLI tab state
  const [selectedTemplate, setSelectedTemplate] = useState<string>('mcp_server');
  const [template, setTemplate] = useState<CliTemplate | null>(null);
  const [cliLoading, setCliLoading] = useState(false);
  const [copiedCode, setCopiedCode] = useState(false);
  const [copiedConfig, setCopiedConfig] = useState(false);

  useEffect(() => {
    listMcpServers().then(setServers).catch(() => {});
  }, []);

  const handleAddServer = async () => {
    if (!form.name.trim()) { setError('Name is required'); return; }
    if (form.transport === 'stdio' && !form.command.trim()) { setError('Command is required'); return; }
    if (form.transport !== 'stdio' && !form.url?.trim()) { setError('URL is required'); return; }

    let parsedArgs: string[] = [];
    let parsedEnv: Record<string, string> = {};
    try {
      parsedArgs = argsText.trim()
        ? argsText.split('\n').map(s => s.trim()).filter(Boolean)
        : [];
      if (envText.trim()) {
        envText.split('\n').forEach(line => {
          const idx = line.indexOf('=');
          if (idx > 0) {
            parsedEnv[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
          }
        });
      }
    } catch {
      setError('Invalid args or env format');
      return;
    }

    setLoading(true);
    setError('');
    try {
      const server = await addMcpServer({ ...form, args: parsedArgs, env: parsedEnv });
      setServers([...servers, server]);
      setMcpView('list');
      setForm(emptyMcpForm());
      setArgsText('');
      setEnvText('');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to add server');
    } finally {
      setLoading(false);
    }
  };

  const handleRemove = async (id: string) => {
    await removeMcpServer(id).catch(() => {});
    setServers(servers.filter(s => s.id !== id));
  };

  const handleToggle = async (id: string) => {
    const updated = await toggleMcpServer(id).catch(() => null);
    if (updated) setServers(servers.map(s => s.id === id ? updated : s));
  };

  const handleLoadTemplate = async () => {
    setCliLoading(true);
    try {
      const t = await generateCliTemplate(selectedTemplate);
      setTemplate(t);
    } catch {
      // fallback
    } finally {
      setCliLoading(false);
    }
  };

  const copyToClipboard = async (text: string, setFlag: (v: boolean) => void) => {
    await navigator.clipboard.writeText(text).catch(() => {});
    setFlag(true);
    setTimeout(() => setFlag(false), 1500);
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <polyline points="4 17 10 11 4 5"/>
            <line x1="12" y1="19" x2="20" y2="19"/>
          </svg>
          MCP & CLI
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      {/* Tabs */}
      <div className="mcp-tabs">
        <button className={`mcp-tab ${tab === 'servers' ? 'active' : ''}`} onClick={() => setTab('servers')}>
          MCP Servers
        </button>
        <button className={`mcp-tab ${tab === 'cli' ? 'active' : ''}`} onClick={() => setTab('cli')}>
          CLI Generator
        </button>
      </div>

      <div className="panel-body">
        {/* ── MCP SERVERS TAB ── */}
        {tab === 'servers' && (
          <>
            {mcpView === 'list' && (
              <>
                <div className="mcp-list-header">
                  <span className="panel-section-header">
                    {servers.length} server{servers.length !== 1 ? 's' : ''}
                  </span>
                  <button className="panel-btn accent" onClick={() => setMcpView('add')}>
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                      <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                    </svg>
                    Add Server
                  </button>
                </div>

                {servers.length === 0 ? (
                  <div className="panel-empty" style={{ paddingTop: 30 }}>
                    <div className="panel-empty-icon">
                      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                        <rect x="2" y="3" width="20" height="14" rx="2"/>
                        <line x1="8" y1="21" x2="16" y2="21"/>
                        <line x1="12" y1="17" x2="12" y2="21"/>
                      </svg>
                    </div>
                    <p>No MCP servers</p>
                    <p className="panel-empty-sub">Connect filesystem, browser, database, or custom MCP servers</p>
                  </div>
                ) : (
                  <div className="server-list">
                    {servers.map(server => (
                      <div key={server.id} className={`server-card ${server.enabled ? '' : 'disabled'}`}>
                        <div className="server-top">
                          <div className="server-status-dot" style={{ background: STATUS_COLORS[server.status] }} />
                          <div className="server-info">
                            <div className="server-name">{server.name}</div>
                            <div className="server-meta">
                              <span className="server-transport">{server.transport}</span>
                              {server.transport === 'stdio'
                                ? <span>{server.command} {server.args.slice(0, 2).join(' ')}</span>
                                : <span>{server.url}</span>
                              }
                            </div>
                          </div>
                          <button
                            className={`toggle-switch ${server.enabled ? 'on' : 'off'}`} role="switch" aria-checked={server.enabled}
                            onClick={() => handleToggle(server.id)}
                          >
                            <div className="toggle-thumb" />
                          </button>
                        </div>
                        <div className="server-foot">
                          <span className="status-label" style={{ color: STATUS_COLORS[server.status] }}>
                            {server.status}
                          </span>
                          <button className="link-btn danger" onClick={() => handleRemove(server.id)}>
                            Remove
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}

                {/* Quick config reference */}
                <div className="mcp-config-hint">
                  <div className="hint-title">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="12" cy="12" r="10"/>
                      <line x1="12" y1="8" x2="12" y2="12"/>
                      <line x1="12" y1="16" x2="12.01" y2="16"/>
                    </svg>
                    Claude Desktop config path
                  </div>
                  <code className="hint-code">~/Library/Application Support/Claude/claude_desktop_config.json</code>
                </div>
              </>
            )}

            {mcpView === 'add' && (
              <div className="panel-form animate-fade-up">
                <button className="back-btn" onClick={() => { setMcpView('list'); setError(''); }}>
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polyline points="15 18 9 12 15 6"/>
                  </svg>
                  Back
                </button>
                <h3 className="form-title">Add MCP Server</h3>

                <div className="form-field">
                  <label>Name</label>
                  <input className="form-input" placeholder="e.g. filesystem" value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))} />
                </div>

                <div className="form-field">
                  <label>Transport</label>
                  <div className="transport-tabs">
                    {(['stdio', 'http', 'sse'] as McpTransport[]).map(t => (
                      <button key={t} className={`transport-btn ${form.transport === t ? 'active' : ''}`}
                        onClick={() => setForm(f => ({ ...f, transport: t }))}>
                        {t.toUpperCase()}
                      </button>
                    ))}
                  </div>
                </div>

                {form.transport === 'stdio' ? (
                  <>
                    <div className="form-field">
                      <label>Command</label>
                      <input className="form-input" placeholder="npx, node, python, ..." value={form.command}
                        onChange={e => setForm(f => ({ ...f, command: e.target.value }))} />
                    </div>
                    <div className="form-field">
                      <label>Args <span className="label-sub">(one per line)</span></label>
                      <textarea className="form-input form-textarea" placeholder="-y&#10;@modelcontextprotocol/server-filesystem&#10;/Users/you/Desktop"
                        value={argsText} onChange={e => setArgsText(e.target.value)} rows={3} />
                    </div>
                    <div className="form-field">
                      <label>Env <span className="label-sub">(KEY=VALUE, one per line)</span></label>
                      <textarea className="form-input form-textarea" placeholder="API_KEY=sk-..."
                        value={envText} onChange={e => setEnvText(e.target.value)} rows={2} />
                    </div>
                  </>
                ) : (
                  <div className="form-field">
                    <label>URL</label>
                    <input className="form-input" placeholder="http://localhost:3000/mcp" value={form.url ?? ''}
                      onChange={e => setForm(f => ({ ...f, url: e.target.value }))} />
                  </div>
                )}

                {error && <div className="form-error">{error}</div>}
                <button className="panel-btn accent full" onClick={handleAddServer} disabled={loading}>
                  {loading ? 'Adding...' : 'Add Server'}
                </button>
              </div>
            )}
          </>
        )}

        {/* ── CLI GENERATOR TAB ── */}
        {tab === 'cli' && (
          <div className="cli-tab animate-fade-up">
            <p className="cli-desc">
              Generate starter code for MCP servers, CLI tools, or agent plugins. 
              Paste into your editor and customize.
            </p>

            <div className="form-field">
              <label>Template Type</label>
              <div className="template-grid">
                {[
                  { id: 'mcp_server', label: 'MCP Server', sub: 'Node.js + SDK', icon: '⬡' },
                  { id: 'cli_tool',   label: 'CLI Tool',   sub: 'Bash script', icon: '❯_' },
                ].map(t => (
                  <button key={t.id}
                    className={`template-card ${selectedTemplate === t.id ? 'active' : ''}`}
                    onClick={() => setSelectedTemplate(t.id)}>
                    <span className="template-icon">{t.icon}</span>
                    <div>
                      <div className="template-name">{t.label}</div>
                      <div className="template-sub">{t.sub}</div>
                    </div>
                  </button>
                ))}
              </div>
            </div>

            <button className="panel-btn accent full" onClick={handleLoadTemplate} disabled={cliLoading}>
              {cliLoading ? 'Generating...' : 'Generate Template'}
            </button>

            {template && (
              <div className="template-output animate-fade-up">
                <div className="code-block-header">
                  <span>{template.name}</span>
                  <button className="copy-btn" onClick={() => copyToClipboard(template.code, setCopiedCode)}>
                    {copiedCode ? 'Copied' : 'Copy Code'}
                  </button>
                </div>
                <pre className="code-block">{template.code}</pre>

                {template.config !== '{}' && (
                  <>
                    <div className="code-block-header" style={{ marginTop: 12 }}>
                      <span>Config (claude_desktop_config.json)</span>
                      <button className="copy-btn" onClick={() => copyToClipboard(template.config, setCopiedConfig)}>
                        {copiedConfig ? 'Copied' : 'Copy'}
                      </button>
                    </div>
                    <pre className="code-block">{template.config}</pre>
                  </>
                )}

                <div className="template-actions">
                  <button className="panel-btn accent"
                    onClick={() => {
                      const s: McpServerInput = {
                        name: template.name,
                        transport: 'stdio',
                        command: 'node',
                        args: ['./index.js'],
                        env: {},
                      };
                      setForm(s);
                      setTab('servers');
                      setMcpView('add');
                    }}>
                    Add as MCP Server →
                  </button>
                </div>
              </div>
            )}

            {/* AI coding hint */}
            <div className="ai-coding-hint">
              <div className="hint-title">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M12 2L2 7l10 5 10-5-10-5z"/>
                  <path d="M2 17l10 5 10-5"/>
                  <path d="M2 12l10 5 10-5"/>
                </svg>
                AI-assisted MCP development
              </div>
              <p>
                Connect an agent and describe what you want your MCP server to do - 
                the AI can help you write, debug, and extend it.
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
