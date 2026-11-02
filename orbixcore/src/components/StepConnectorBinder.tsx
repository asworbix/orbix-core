import { useState } from 'react';
import type { WorkflowStep, UniversalConnector } from '../types';
import { updateWorkflowStepConnector } from '../hooks/api';
import './StepConnectorBinder.css';

interface Props {
  workflowId: string;
  step: WorkflowStep;
  connectors: UniversalConnector[];
  onUpdated: (step: WorkflowStep) => void;
}

export default function StepConnectorBinder({ workflowId, step, connectors, onUpdated }: Props) {
  const currentConnId = step.config?.connector_id as string | undefined;
  const currentEpKey = step.config?.endpoint_key as string | undefined;
  const currentConn = connectors.find(c => c.id === currentConnId);

  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  const handleBind = async (connectorId: string, endpointKey: string) => {
    setSaving(true);
    try {
      const updated = await updateWorkflowStepConnector(workflowId, step.id, connectorId, endpointKey);
      const updatedStep = updated.steps.find(s => s.id === step.id);
      if (updatedStep) onUpdated(updatedStep);
      setOpen(false);
    } catch { /* */ }
    setSaving(false);
  };

  const handleUnbind = async () => {
    setSaving(true);
    try {
      const updated = await updateWorkflowStepConnector(workflowId, step.id, null, null);
      const updatedStep = updated.steps.find(s => s.id === step.id);
      if (updatedStep) onUpdated(updatedStep);
      setOpen(false);
    } catch { /* */ }
    setSaving(false);
  };

  if (step.step_type !== 'action') return null;

  return (
    <div className="scb">
      {/* Current binding indicator */}
      <button className={`scb-trigger ${currentConnId ? 'bound' : ''}`} onClick={() => setOpen(!open)}>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
        </svg>
        {currentConn ? (
          <span className="scb-bound-label">{currentConn.name}{currentEpKey ? ` / ${currentEpKey}` : ''}</span>
        ) : (
          <span className="scb-unbound-label">Bind connector</span>
        )}
      </button>

      {/* Dropdown picker */}
      {open && (
        <div className="scb-dropdown">
          {connectors.length === 0 ? (
            <div className="scb-empty">No connectors available. Add one in the Connectors panel.</div>
          ) : (
            <>
              {currentConnId && (
                <button className="scb-option scb-unbind" onClick={handleUnbind} disabled={saving}>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                  </svg>
                  Remove binding
                </button>
              )}
              {connectors.map(conn => (
                <div key={conn.id} className="scb-connector-group">
                  <div className="scb-connector-name">{conn.name}</div>
                  {conn.endpoints.length === 0 ? (
                    <div className="scb-no-endpoints">No endpoints</div>
                  ) : (
                    conn.endpoints.map(ep => (
                      <button
                        key={ep.key}
                        className={`scb-option ${currentConnId === conn.id && currentEpKey === ep.key ? 'active' : ''}`}
                        onClick={() => handleBind(conn.id, ep.key)}
                        disabled={saving}
                      >
                        <span className="scb-ep-method" data-method={ep.method.toLowerCase()}>
                          {ep.method}
                        </span>
                        <span className="scb-ep-path">{ep.path}</span>
                        <span className="scb-ep-key">{ep.key}</span>
                      </button>
                    ))
                  )}
                </div>
              ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}
