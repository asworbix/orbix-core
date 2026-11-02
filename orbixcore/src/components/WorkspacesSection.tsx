import { useCallback, useEffect, useState } from 'react';
import {
  listWorkspaceProfiles, createWorkspaceProfile, switchWorkspaceProfile,
  deleteWorkspaceProfile, WorkspaceProfile,
} from '../hooks/api';
import './WorkspacesSection.css';

/* Workspace profiles: several Orbix Cores side by side - work, private,
   client sandbox - each with its own conversations, agents, workflows,
   memory and policy. Models and login are shared. Switching restarts
   the app so every store loads the new workspace cleanly. */

export default function WorkspacesSection() {
  const [profiles, setProfiles] = useState<WorkspaceProfile[]>([]);
  const [active, setActive] = useState('');
  const [name, setName] = useState('');
  const [cloneCurrent, setCloneCurrent] = useState(true);
  const [includeHistory, setIncludeHistory] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(() => {
    listWorkspaceProfiles()
      .then(v => { setProfiles(v.profiles); setActive(v.active); })
      .catch(() => {});
  }, []);
  useEffect(() => { refresh(); }, [refresh]);

  const create = async () => {
    setError('');
    setBusy(true);
    try {
      await createWorkspaceProfile(name, cloneCurrent, cloneCurrent && includeHistory);
      setName('');
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Could not create the workspace.');
    } finally {
      setBusy(false);
    }
  };

  const switchTo = async (p: WorkspaceProfile) => {
    if (p.id === active) return;
    if (!window.confirm(`Switch to "${p.name}"? Orbix will restart into that workspace.`)) return;
    try {
      await switchWorkspaceProfile(p.id);
      // The app restarts; nothing more to do here.
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Switch failed.');
    }
  };

  const remove = async (p: WorkspaceProfile) => {
    if (!window.confirm(`Delete workspace "${p.name}" and everything in it? This cannot be undone.`)) return;
    try {
      await deleteWorkspaceProfile(p.id);
      refresh();
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Delete failed.');
    }
  };

  return (
    <div className="ws-section">
      {error && <div className="ws-error">{error}</div>}
      <div className="ws-list">
        {profiles.map(p => (
          <div key={p.id} className={`ws-row ${p.id === active ? 'is-active' : ''}`}>
            <span className="ws-dot" />
            <span className="ws-name">{p.name}</span>
            {p.id === active ? (
              <span className="ws-badge">active</span>
            ) : (
              <>
                <button className="ws-btn" onClick={() => switchTo(p)}>Switch</button>
                {p.id !== 'default' && (
                  <button className="ws-btn ws-del" onClick={() => remove(p)} title="Delete workspace">×</button>
                )}
              </>
            )}
          </div>
        ))}
      </div>

      <div className="ws-new">
        <input
          value={name}
          onChange={e => setName(e.target.value)}
          placeholder="New workspace, e.g. Work"
          onKeyDown={e => { if (e.key === 'Enter' && name.trim()) create(); }}
        />
        <button className="ws-btn ws-create" disabled={!name.trim() || busy} onClick={create}>
          {busy ? 'Creating…' : 'Create'}
        </button>
      </div>
      <label className="ws-opt">
        <input type="checkbox" checked={cloneCurrent} onChange={e => setCloneCurrent(e.target.checked)} />
        Start from the current setup (agents, workflows, memory)
      </label>
      {cloneCurrent && (
        <label className="ws-opt ws-opt-sub">
          <input type="checkbox" checked={includeHistory} onChange={e => setIncludeHistory(e.target.checked)} />
          Also copy conversation history
        </label>
      )}
      <p className="ws-fine">Models and your login are shared. Switching restarts Orbix into the chosen workspace.</p>
    </div>
  );
}
