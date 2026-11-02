import type { WorkflowStep, WorkflowRunState, StepType, UniversalConnector } from '../types';
import './WorkflowPipeline.css';

interface Props {
  steps: WorkflowStep[];
  runState: WorkflowRunState | null;
  isRunning: boolean;
  mode?: 'horizontal' | 'vertical';
  connectors?: UniversalConnector[];
}

const STEP_COLORS: Record<StepType, string> = {
  prompt: '#6C8FFF',
  action: '#34D399',
  condition: '#FBBF24',
  wait: '#A78BFA',
  loop: '#EC4899',
  relay: '#10B981',
};

const STEP_ICONS: Record<StepType, React.ReactNode> = {
  prompt: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/>
    </svg>
  ),
  action: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <polygon points="13 2 3 14 12 14 11 22 21 10 12 10"/>
    </svg>
  ),
  condition: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 3l9 9-9 9-9-9z"/>
    </svg>
  ),
  wait: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
    </svg>
  ),
  loop: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
    </svg>
  ),
  relay: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="2" y="6" width="6" height="12" rx="1"/><rect x="16" y="6" width="6" height="12" rx="1"/>
      <line x1="8" y1="12" x2="16" y2="12"/><circle cx="12" cy="12" r="2"/>
    </svg>
  ),
};

function getStepStatus(index: number, runState: WorkflowRunState | null, isRunning: boolean) {
  if (!runState || !isRunning) return 'pending';
  if (index < runState.current_step) return 'done';
  if (index === runState.current_step) return 'active';
  return 'pending';
}

function getFinishedStatus(index: number, runState: WorkflowRunState | null) {
  if (!runState) return 'pending';
  if (runState.status === 'completed') return 'done';
  if (runState.status === 'failed' && index <= (runState.current_step || 0)) {
    return index === runState.current_step ? 'failed' : 'done';
  }
  return 'pending';
}

export default function WorkflowPipeline({ steps, runState, isRunning, mode = 'vertical', connectors = [] }: Props) {
  if (steps.length === 0) return null;

  const isFinished = runState && (runState.status === 'completed' || runState.status === 'failed' || runState.status === 'stopped');

  return (
    <div className={`pipeline pipeline-${mode}`}>
      {steps.map((step, i) => {
        const color = STEP_COLORS[step.step_type];
        const icon = STEP_ICONS[step.step_type];
        const status = isRunning
          ? getStepStatus(i, runState, isRunning)
          : isFinished
            ? getFinishedStatus(i, runState)
            : 'pending';
        const output = runState?.output?.[i];
        const isLast = i === steps.length - 1;

        return (
          <div key={step.id} className={`pipeline-node pipeline-node-${status}`}>
            {/* Connector line (not on first step in vertical, handled differently in horizontal) */}
            {mode === 'vertical' && i > 0 && (
              <div className={`pipeline-connector ${status === 'done' || status === 'active' ? 'filled' : ''}`} />
            )}

            <div className="pipeline-step">
              {/* Status indicator */}
              <div className="pipeline-indicator" style={{ borderColor: color }}>
                {status === 'done' ? (
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#34D399" strokeWidth="3">
                    <polyline points="20 6 9 17 4 12"/>
                  </svg>
                ) : status === 'active' ? (
                  <div className="pipeline-spinner" style={{ borderTopColor: color }} />
                ) : status === 'failed' ? (
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#F87171" strokeWidth="3">
                    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                  </svg>
                ) : (
                  <span className="pipeline-step-num">{i + 1}</span>
                )}
              </div>

              {/* Card */}
              <div className={`pipeline-card ${status}`} style={{ '--step-color': color } as React.CSSProperties}>
                <div className="pipeline-card-header">
                  <div className="pipeline-card-icon" style={{ color, background: `${color}15` }}>
                    {icon}
                  </div>
                  <div className="pipeline-card-info">
                    <span className="pipeline-card-label">{step.label}</span>
                    <span className="pipeline-card-type">
                      {step.step_type}
                      {(() => {
                        const connId = step.config?.connector_id as string | undefined;
                        if (!connId) return null;
                        const conn = connectors.find(c => c.id === connId);
                        const epKey = step.config?.endpoint_key as string | undefined;
                        return (
                          <span className="pipeline-connector-badge">
                            <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3"/></svg>
                            {conn?.name || 'connector'}{epKey ? ` / ${epKey}` : ''}
                          </span>
                        );
                      })()}
                    </span>
                  </div>
                </div>

                {/* Output when done */}
                {output && (status === 'done' || isFinished) && (
                  <div className="pipeline-card-output">{output}</div>
                )}
              </div>
            </div>

            {/* Horizontal connector dot */}
            {mode === 'horizontal' && !isLast && (
              <div className={`pipeline-h-connector ${status === 'done' ? 'filled' : ''}`}>
                <div className="pipeline-h-line" />
                <div className="pipeline-h-dot" />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
