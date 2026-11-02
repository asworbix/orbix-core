import { useCallback, useEffect, useState } from 'react';
import type { UserAction } from '../types';
import { listUserActions, onDataChanged, onUserActions, runUserAction } from '../hooks/api';
import './ActionsBar.css';

/*
 * Steps waiting on the user, as a bar that stays until they are done.
 * "Claude needs a restart before it can call Bob. [Restart Claude]".
 * A toast is not enough for this: it is gone in four seconds and the
 * pairing silently never completes. The list comes from the backend
 * (user_actions.rs), so Bob sees the same items and can run one when
 * the user says so in chat.
 */

export default function ActionsBar() {
  const [pending, setPending] = useState<UserAction[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    listUserActions().then(setPending).catch(() => {});
  }, []);

  useEffect(() => {
    reload();
    const unsubData = onDataChanged((reason) => {
      if (reason === 'peers' || reason === 'user-actions') reload();
    });
    const unsubLive = onUserActions(setPending);
    return () => {
      unsubData.then((fn) => fn());
      unsubLive.then((fn) => fn());
    };
  }, [reload]);

  if (pending.length === 0) return null;

  const run = async (a: UserAction) => {
    setBusy(a.id);
    setError(null);
    try {
      await runUserAction(a.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      reload();
    }
  };

  return (
    <div className="actions-bar" role="status">
      {pending.map((a) => (
        <div key={a.id} className="actions-bar-item">
          <span className="actions-bar-mark">◉</span>
          <span className="actions-bar-text">
            <strong>{a.subject}</strong> {a.reason}
          </span>
          <button
            type="button"
            className="actions-bar-btn"
            disabled={busy === a.id}
            onClick={() => run(a)}
          >
            {busy === a.id ? 'Working…' : a.label}
          </button>
        </div>
      ))}
      {error && <div className="actions-bar-error">{error}</div>}
    </div>
  );
}
