import { useState } from 'react';
import type { ToolCall } from '../types';
import { approveToolCall, denyToolCall } from '../hooks/api';
import './ToolApproval.css';

interface Props {
  call: ToolCall;
  onResolved: (call: ToolCall) => void;
}

export default function ToolApproval({ call, onResolved }: Props) {
  const [loading, setLoading] = useState(false);

  const handleApprove = async () => {
    setLoading(true);
    try {
      const resolved = await approveToolCall(call.id);
      onResolved(resolved);
    } catch {
      setLoading(false);
    }
  };

  const handleDeny = async () => {
    setLoading(true);
    try {
      const resolved = await denyToolCall(call.id);
      onResolved(resolved);
    } catch {
      setLoading(false);
    }
  };

  const argEntries = Object.entries(call.arguments);

  return (
    <div className="tool-approval">
      <div className="tool-approval-header">
        <div className="tool-approval-icon">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>
          </svg>
        </div>
        <div className="tool-approval-info">
          <span className="tool-approval-name">{call.tool_name}</span>
          <span className="tool-approval-label">wants to execute</span>
        </div>
      </div>

      {argEntries.length > 0 && (
        <div className="tool-approval-args">
          {argEntries.map(([key, val]) => (
            <div key={key} className="tool-arg">
              <span className="tool-arg-key">{key}:</span>
              <span className="tool-arg-val">{typeof val === 'string' ? val : JSON.stringify(val)}</span>
            </div>
          ))}
        </div>
      )}

      {!loading ? (
        <div className="tool-approval-actions">
          <button className="tool-btn approve" onClick={handleApprove}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
            Approve
          </button>
          <button className="tool-btn deny" onClick={handleDeny}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
            Deny
          </button>
        </div>
      ) : (
        <div className="tool-approval-loading">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="spin">
            <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
          </svg>
          Executing...
        </div>
      )}
    </div>
  );
}
