import { useState, useEffect } from 'react';
import {
  listConfidentialFolders, addConfidentialFolder, updateConfidentialFolder,
  removeConfidentialFolder, listFolderContents, getFolderAccessLog, pickFolder,
  listAgents,
} from '../hooks/api';
import type { Agent } from '../types';
import './Panel.css';
import './DataControlPanel.css';

interface Props {
  onClose: () => void;
}

export default function DataControlPanel({ onClose }: Props) {
  const [folders, setFolders] = useState<any[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [contents, setContents] = useState<any>(null);
  const [accessLog, setAccessLog] = useState<any[]>([]);
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState('');
  const [newPath, setNewPath] = useState('');
  const [newAccess, setNewAccess] = useState<'read_only' | 'read_write'>('read_only');
  const [newAgents, setNewAgents] = useState<Set<string>>(new Set());
  const [tab, setTab] = useState<'files' | 'log' | 'permissions'>('files');

  useEffect(() => {
    listConfidentialFolders().then(setFolders).catch(() => {});
    listAgents().then(setAgents).catch(() => {});
  }, []);

  useEffect(() => {
    if (selectedId) {
      listFolderContents(selectedId).then(setContents).catch(() => setContents(null));
      getFolderAccessLog(selectedId).then(setAccessLog).catch(() => setAccessLog([]));
    }
  }, [selectedId]);

  const selectedFolder = folders.find(f => f.id === selectedId);

  const handlePickFolder = async () => {
    try {
      const path = await pickFolder();
      setNewPath(path);
      if (!newName) {
        // Auto-name from folder
        const parts = path.split('/');
        setNewName(parts[parts.length - 1] || 'Untitled');
      }
    } catch { /* cancelled */ }
  };

  const handleAdd = async () => {
    if (!newName.trim() || !newPath.trim()) return;
    const folder = await addConfidentialFolder(newName, newPath, Array.from(newAgents), newAccess);
    setFolders(prev => [...prev, folder]);
    setAdding(false);
    setNewName('');
    setNewPath('');
    setNewAgents(new Set());
    setNewAccess('read_only');
    setSelectedId(folder.id);
  };

  const handleRemove = async (id: string) => {
    await removeConfidentialFolder(id);
    setFolders(prev => prev.filter(f => f.id !== id));
    if (selectedId === id) {
      setSelectedId(null);
      setContents(null);
    }
  };

  const handleToggleAgent = async (agentId: string) => {
    if (!selectedFolder) return;
    const current = new Set<string>(selectedFolder.allowed_agents as string[]);
    if (current.has(agentId)) current.delete(agentId);
    else current.add(agentId);
    const updated = await updateConfidentialFolder(selectedFolder.id, undefined, Array.from(current));
    if (updated) {
      setFolders(prev => prev.map(f => f.id === updated.id ? updated : f));
    }
  };

  const handleToggleAccess = async () => {
    if (!selectedFolder) return;
    const newLevel = selectedFolder.access_level === 'read_only' ? 'read_write' : 'read_only';
    const updated = await updateConfidentialFolder(selectedFolder.id, undefined, undefined, newLevel);
    if (updated) {
      setFolders(prev => prev.map(f => f.id === updated.id ? updated : f));
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
    if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
    if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(0)} KB`;
    return `${bytes} B`;
  };

  const formatTime = (iso: string) => {
    try { return new Date(iso).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }); }
    catch { return ''; }
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/>
            <rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>
          </svg>
          Data Control
          <span className="dc-count">{folders.length}</span>
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body dc-body">
        {/* Add folder form */}
        {adding ? (
          <div className="dc-add-form">
            <div className="dc-add-header">Add Confidential Folder</div>
            <div className="dc-field">
              <label>Name</label>
              <input value={newName} onChange={e => setNewName(e.target.value)} placeholder="e.g. Client Contracts" autoFocus />
            </div>
            <div className="dc-field">
              <label>Folder Path</label>
              <div className="dc-path-row">
                <input value={newPath} onChange={e => setNewPath(e.target.value)} placeholder="/Users/..." readOnly />
                <button className="dc-browse-btn" onClick={handlePickFolder}>Browse</button>
              </div>
            </div>
            <div className="dc-field">
              <label>Access Level</label>
              <div className="dc-access-toggle">
                <button className={`dc-access-btn ${newAccess === 'read_only' ? 'active' : ''}`} onClick={() => setNewAccess('read_only')}>Read Only</button>
                <button className={`dc-access-btn ${newAccess === 'read_write' ? 'active' : ''}`} onClick={() => setNewAccess('read_write')}>Read & Write</button>
              </div>
            </div>
            <div className="dc-field">
              <label>Allowed Agents</label>
              <div className="dc-agent-list">
                {agents.map(a => (
                  <button key={a.id} className={`dc-agent-chip ${newAgents.has(a.id) ? 'active' : ''}`}
                    onClick={() => { const s = new Set(newAgents); s.has(a.id) ? s.delete(a.id) : s.add(a.id); setNewAgents(s); }}>
                    {a.name}
                  </button>
                ))}
                {agents.length === 0 && <span className="dc-empty-hint">No agents yet</span>}
              </div>
            </div>
            <div className="dc-add-actions">
              <button className="dc-btn secondary" onClick={() => setAdding(false)}>Cancel</button>
              <button className="dc-btn primary" onClick={handleAdd} disabled={!newName.trim() || !newPath.trim()}>Add Folder</button>
            </div>
          </div>
        ) : (
          <>
            <button className="dc-add-btn" onClick={() => setAdding(true)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
              Add Confidential Folder
            </button>

            {/* Folder list */}
            {folders.length === 0 ? (
              <div className="dc-empty">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" style={{ color: 'var(--text-dim)', opacity: 0.4 }}>
                  <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                </svg>
                <span>No confidential folders registered</span>
                <span className="dc-empty-sub">Add a folder to control agent access to sensitive files</span>
              </div>
            ) : (
              <div className="dc-folder-list">
                {folders.map(f => (
                  <div key={f.id} className={`dc-folder-row ${selectedId === f.id ? 'selected' : ''}`} onClick={() => setSelectedId(f.id)}>
                    <div className="dc-folder-icon">
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                      </svg>
                      <svg className="dc-lock-badge" width="8" height="8" viewBox="0 0 24 24" fill="currentColor">
                        <rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                      </svg>
                    </div>
                    <div className="dc-folder-info">
                      <span className="dc-folder-name">{f.name}</span>
                      <span className="dc-folder-path">{f.path}</span>
                    </div>
                    <div className="dc-folder-meta">
                      <span className={`dc-access-badge ${f.access_level}`}>{f.access_level === 'read_write' ? 'R/W' : 'R'}</span>
                      <span className="dc-agent-count">{f.allowed_agents.length} agent{f.allowed_agents.length !== 1 ? 's' : ''}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}

            {/* Selected folder detail */}
            {selectedFolder && (
              <div className="dc-detail">
                <div className="dc-detail-header">
                  <span className="dc-detail-name">{selectedFolder.name}</span>
                  <button className="dc-remove-btn" onClick={() => handleRemove(selectedFolder.id)} title="Remove folder">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                    </svg>
                  </button>
                </div>

                {/* Tabs */}
                <div className="dc-tabs">
                  <button className={`dc-tab ${tab === 'files' ? 'active' : ''}`} onClick={() => setTab('files')}>Files</button>
                  <button className={`dc-tab ${tab === 'permissions' ? 'active' : ''}`} onClick={() => setTab('permissions')}>Permissions</button>
                  <button className={`dc-tab ${tab === 'log' ? 'active' : ''}`} onClick={() => setTab('log')}>Access Log</button>
                </div>

                {tab === 'files' && contents && (
                  <div className="dc-files">
                    {contents.files.length === 0 ? (
                      <div className="dc-empty-hint">Folder is empty</div>
                    ) : contents.files.map((f: any, i: number) => (
                      <div key={i} className="dc-file-row">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" style={{ color: 'var(--text-dim)', flexShrink: 0 }}>
                          {f.is_dir
                            ? <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                            : <><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></>
                          }
                        </svg>
                        <span className="dc-file-name">{f.name}</span>
                        {!f.is_dir && <span className="dc-file-size">{formatSize(f.size_bytes)}</span>}
                      </div>
                    ))}
                  </div>
                )}

                {tab === 'permissions' && (
                  <div className="dc-permissions">
                    <div className="dc-perm-row" onClick={handleToggleAccess} style={{ cursor: 'pointer' }}>
                      <span className="dc-perm-label">Access Level</span>
                      <span className={`dc-access-badge ${selectedFolder.access_level}`}>
                        {selectedFolder.access_level === 'read_write' ? 'Read & Write' : 'Read Only'}
                      </span>
                    </div>
                    <div className="dc-perm-section">Allowed Agents</div>
                    {agents.map(a => (
                      <div key={a.id} className="dc-perm-agent" onClick={() => handleToggleAgent(a.id)}>
                        <span className={`dc-perm-check ${selectedFolder.allowed_agents.includes(a.id) ? 'active' : ''}`}>
                          {selectedFolder.allowed_agents.includes(a.id) ? '\u2713' : ''}
                        </span>
                        <span className="dc-perm-agent-name">{a.name}</span>
                        <span className="dc-perm-agent-model">{a.model}</span>
                      </div>
                    ))}
                  </div>
                )}

                {tab === 'log' && (
                  <div className="dc-log">
                    {accessLog.length === 0 ? (
                      <div className="dc-empty-hint">No access recorded yet</div>
                    ) : accessLog.slice(-50).reverse().map(l => (
                      <div key={l.id} className="dc-log-row">
                        <span className="dc-log-time">{formatTime(l.timestamp)}</span>
                        <span className="dc-log-agent">{l.agent_name}</span>
                        <span className={`dc-log-action ${l.action}`}>{l.action}</span>
                        <span className="dc-log-file">{l.file_path.split('/').pop()}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
