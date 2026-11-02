import { useCallback, useEffect, useState } from 'react';
import type { Peer } from '../types';
import { listPeers, onDataChanged, peerFollowUp, peerRefresh, unpairApp } from '../hooks/api';
import './PairedApps.css';

/*
 * Paired apps as chips: "ChatGPT · Connected". Click one for Bob's line
 * about it, the one human step if any (Restart Claude, Allow
 * Accessibility), a re-check, and unpair. The chips are the whole UI
 * for pair mode; the probe ladder stays Bob's business.
 */

const STATUS_LABEL: Record<Peer['status'], string> = {
  connected: 'Connected',
  needs_restart: 'Restart needed',
  needs_permission: 'Permission needed',
  needs_sign_in: 'Sign-in needed',
  limited: 'Clipboard only',
};

export default function PairedApps() {
  const [peers, setPeers] = useState<Peer[]>([]);
  const [openId, setOpenId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const reload = useCallback(() => {
    listPeers().then(setPeers).catch(() => {});
  }, []);

  useEffect(() => {
    reload();
    const unsub = onDataChanged((reason) => {
      if (reason === 'peers') reload();
    });
    return () => {
      unsub.then((fn) => fn());
    };
  }, [reload]);

  if (peers.length === 0) return null;

  const run = async (id: string, fn: () => Promise<unknown>) => {
    setBusy(id);
    try {
      await fn();
    } catch {
      /* the pair-event toast reports failures */
    } finally {
      setBusy(null);
      reload();
    }
  };

  return (
    <div className="paired-apps">
      {peers.map((p) => (
        <div key={p.id} className="paired-app">
          <button
            type="button"
            className={`paired-app-chip status-${p.status} ${openId === p.id ? 'open' : ''}`}
            onClick={() => setOpenId((cur) => (cur === p.id ? null : p.id))}
            title={p.bob_line}
          >
            <span className="paired-app-dot" />
            <span className="paired-app-name">{p.app}</span>
            <span className="paired-app-sep">&middot;</span>
            <span className="paired-app-status">{STATUS_LABEL[p.status]}</span>
          </button>
          {openId === p.id && (
            <div className="paired-app-card">
              <p className="paired-app-line">{p.bob_line}</p>
              {p.last_error && <p className="paired-app-error">{p.last_error}</p>}
              <div className="paired-app-actions">
                {p.follow_up && (
                  <button
                    type="button"
                    className="paired-app-btn primary"
                    disabled={busy === p.id}
                    onClick={() => run(p.id, () => peerFollowUp(p.app))}
                  >
                    {p.follow_up.label}
                  </button>
                )}
                <button
                  type="button"
                  className="paired-app-btn"
                  disabled={busy === p.id}
                  onClick={() => run(p.id, () => peerRefresh(p.app))}
                >
                  Check again
                </button>
                <button
                  type="button"
                  className="paired-app-btn danger"
                  disabled={busy === p.id}
                  onClick={() => run(p.id, () => unpairApp(p.app))}
                >
                  Unpair
                </button>
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
