// Review Queue Panel - surface for drafts produced by the JobProfile
// scheduler when review_gate is Always or FirstN. Each item is the
// output of a connector's draft_<engagement> action; on approval the
// matching publish action fires through the bus with the saved payload.

import { useEffect, useState } from 'react';
import {
  reviewQueueApprove,
  reviewQueueList,
  reviewQueueReject,
  type ConnectorResult,
  type PendingReview,
} from '../hooks/api';
import './ReviewQueuePanel.css';

interface Props { onClose: () => void }

export default function ReviewQueuePanel({ onClose }: Props) {
  const [items, setItems] = useState<PendingReview[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<Record<string, ConnectorResult | string>>({});

  async function refresh() {
    setLoading(true);
    try {
      const out = await reviewQueueList();
      setItems(out);
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => { refresh(); }, []);

  async function approve(id: string) {
    setBusy(id);
    setResult((r) => ({ ...r, [id]: 'Publishing…' }));
    try {
      const r = await reviewQueueApprove(id);
      setResult((prev) => ({ ...prev, [id]: r }));
      if (r.success) await refresh();
    } catch (e: any) {
      setResult((prev) => ({ ...prev, [id]: String(e) }));
    } finally {
      setBusy(null);
    }
  }

  async function reject(id: string) {
    setBusy(id);
    try {
      await reviewQueueReject(id);
      await refresh();
    } catch (e: any) {
      setResult((prev) => ({ ...prev, [id]: String(e) }));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="rq-shell">
      <header className="rq-head">
        <h2>Review Queue</h2>
        <button className="rq-close" onClick={onClose} aria-label="close">×</button>
      </header>

      <div className="rq-body">
        {loading && <p className="rq-empty">Loading…</p>}
        {!loading && items.length === 0 && (
          <p className="rq-empty">
            Nothing pending. Drafts produced by your scheduled profiles
            will appear here for approval before they publish.
          </p>
        )}

        {items.map((item) => {
          const r = result[item.id];
          return (
            <div key={item.id} className="rq-card">
              <div className="rq-row">
                <span className="rq-slug">{item.connector_slug}</span>
                <span className="rq-action">{item.draft_action} → {item.publish_action}</span>
                <time className="rq-time">{new Date(item.created_at).toLocaleString()}</time>
              </div>
              <p className="rq-goal">{item.goal}</p>
              <details>
                <summary>Draft payload</summary>
                <pre className="rq-mono">{JSON.stringify(item.payload, null, 2)}</pre>
              </details>
              <div className="rq-actions">
                <button
                  className="rq-secondary"
                  onClick={() => reject(item.id)}
                  disabled={busy === item.id}
                >Reject</button>
                <button
                  className="rq-primary"
                  onClick={() => approve(item.id)}
                  disabled={busy === item.id}
                >
                  {busy === item.id ? 'Working…' : 'Approve & publish'}
                </button>
              </div>
              {r && (
                <pre className={`rq-mono ${typeof r === 'object' && r.success === false ? 'rq-fail' : ''}`}>
                  {typeof r === 'string' ? r : JSON.stringify(r, null, 2)}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
