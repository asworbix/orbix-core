import { useState, useEffect, useCallback } from 'react';
import './ApprovalQueuePanel.css';

interface ApprovalItem {
  id: number;
  session_id: string | null;
  agent: string;
  type: string;
  summary: string;
  amount: number | null;
  currency: string;
  data: Record<string, unknown> | null;
  status: string;
  notes: string | null;
  resolved_by: string | null;
  resolved_at: string | null;
  created_at: string;
}

interface Props { onClose: () => void }

const API = 'http://localhost:3000/api/agent';

const TYPE_LABELS: Record<string, string> = {
  payroll: 'Payroll',
  vat_submission: 'VAT / Moms',
  invoice_send: 'Invoice Send',
  large_payment: 'Large Payment',
  contract: 'Contract',
  hiring: 'Hiring',
  budget_commit: 'Budget',
  other: 'Other',
};

const STATUS_COLORS: Record<string, string> = {
  pending: '#f59e0b',
  approved: '#10b981',
  rejected: '#ef4444',
};

function timeAgo(ts: string): string {
  const diff = Date.now() - new Date(ts).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

export default function ApprovalQueuePanel({ onClose }: Props) {
  const [items, setItems] = useState<ApprovalItem[]>([]);
  const [filter, setFilter] = useState<'pending' | 'all'>('pending');
  const [resolving, setResolving] = useState<number | null>(null);
  const [notes, setNotes] = useState('');

  const fetchItems = useCallback(() => {
    const url = filter === 'pending' ? `${API}/approvals?status=pending` : `${API}/approvals`;
    fetch(url).then(r => r.json()).then(d => setItems(d.data || [])).catch(() => {});
  }, [filter]);

  useEffect(() => { fetchItems(); }, [fetchItems]);

  const handleResolve = async (id: number, approved: boolean) => {
    try {
      await fetch(`${API}/approvals/${id}/resolve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ approved, notes, resolved_by: 'Director' }),
      });
      setResolving(null);
      setNotes('');
      fetchItems();
    } catch (err) {
      console.error('Failed to resolve:', err);
    }
  };

  const pendingCount = items.filter(i => i.status === 'pending').length;

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M9 12l2 2 4-4"/>
            <path d="M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z"/>
          </svg>
          Approval Queue
          {pendingCount > 0 && <span className="aq-badge">{pendingCount}</span>}
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="aq-filter-bar">
        <button className={filter === 'pending' ? 'active' : ''} onClick={() => setFilter('pending')}>
          Pending {pendingCount > 0 && `(${pendingCount})`}
        </button>
        <button className={filter === 'all' ? 'active' : ''} onClick={() => setFilter('all')}>All</button>
        <button className="aq-refresh" onClick={fetchItems} title="Refresh" aria-label="Refresh">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M1 4v6h6"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/>
          </svg>
        </button>
      </div>

      <div className="panel-body">
        {items.length === 0 ? (
          <div className="panel-empty">
            <div className="panel-empty-icon">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                <path d="M9 12l2 2 4-4"/>
                <circle cx="12" cy="12" r="10"/>
              </svg>
            </div>
            <p>{filter === 'pending' ? 'No pending approvals' : 'No approval history'}</p>
            <p className="panel-empty-sub">Agent actions requiring director sign-off appear here.</p>
          </div>
        ) : (
          items.map(item => (
            <div key={item.id} className={`aq-item ${item.status}`}>
              <div className="aq-item-header">
                <span className="aq-type-badge" style={{ background: `${STATUS_COLORS[item.status] || '#666'}22`, color: STATUS_COLORS[item.status] || '#666' }}>
                  {TYPE_LABELS[item.type] || item.type}
                </span>
                <span className="aq-agent">{item.agent}</span>
                <span className="aq-time">{timeAgo(item.created_at)}</span>
              </div>

              <div className="aq-summary">{item.summary}</div>

              {item.amount != null && (
                <div className="aq-amount">
                  {item.currency} {item.amount.toLocaleString(undefined, { minimumFractionDigits: 2 })}
                </div>
              )}

              {item.status === 'pending' && (
                <>
                  {resolving === item.id ? (
                    <div className="aq-resolve-form">
                      <textarea
                        className="aq-notes-input"
                        placeholder="Notes (optional)"
                        value={notes}
                        onChange={e => setNotes(e.target.value)}
                        rows={2}
                      />
                      <div className="aq-resolve-actions">
                        <button className="aq-btn reject" onClick={() => handleResolve(item.id, false)}>Reject</button>
                        <button className="aq-btn approve" onClick={() => handleResolve(item.id, true)}>Approve</button>
                        <button className="aq-btn cancel" onClick={() => { setResolving(null); setNotes(''); }}>Cancel</button>
                      </div>
                    </div>
                  ) : (
                    <div className="aq-pending-actions">
                      <button className="aq-btn approve" onClick={() => setResolving(item.id)}>Review</button>
                    </div>
                  )}
                </>
              )}

              {item.status !== 'pending' && (
                <div className="aq-resolved-info">
                  <span className={`aq-status-badge ${item.status}`}>{item.status}</span>
                  {item.resolved_by && <span>by {item.resolved_by}</span>}
                  {item.notes && <span className="aq-resolved-notes">{item.notes}</span>}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
