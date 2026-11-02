import { useState, useEffect, useRef } from 'react';
import type { Agent, Workflow, Message, OrbState } from '../types';
import { listWorkflows, onBobStep, bobCancel, listGeneratedDocuments } from '../hooks/api';
import ChatHistory from './ChatHistory';
import ChatInput from './ChatInput';
import PairedApps from './PairedApps';
import ActionsBar from './ActionsBar';
import './RunView.css';

interface Props {
  agents: Agent[];
  messages: Message[];
  orbState: OrbState;
  audioLevel?: number;
  onOpenPanel: (panel: string, workflowId?: string) => void;
  onMessageSent: (msg: Message) => void;
  onRunWorkflow: (wf: Workflow) => void;
}

const SUGGESTIONS = [
  'Status overview',
  'What changed today?',
  'Run the morning briefing',
];

/* Calm, dry, time-aware. One word - the status line does the talking. */
function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return 'Late.';
  if (h < 12) return 'Morning.';
  if (h < 18) return 'Afternoon.';
  return 'Evening.';
}

export default function RunView({
  agents,
  messages,
  orbState,
  onMessageSent,
  onOpenPanel,
}: Props) {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [suggestion, setSuggestion] = useState<string | null>(null);
  const [bobStep, setBobStep] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const [docsThisWeek, setDocsThisWeek] = useState(0);
  useEffect(() => {
    const weekAgo = Date.now() - 7 * 24 * 3600 * 1000;
    listGeneratedDocuments()
      .then(docs => setDocsThisWeek(docs.filter(d => new Date(d.created_at).getTime() > weekAgo).length))
      .catch(() => {});
  }, []);
  useEffect(() => {
    listWorkflows().then(setWorkflows).catch(() => {});
  }, []);

  useEffect(() => {
    const unsub = onBobStep(setBobStep);
    return () => {
      unsub.then((fn) => fn());
    };
  }, []);

  // Autoscroll on new messages
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [messages.length, bobStep]);

  const isEmpty = messages.length === 0;

  return (
    <div className="run-view">
      {/* Steps waiting on the user (restart Claude, allow Accessibility).
          Stays until done; Bob sees the same list. */}
      <ActionsBar />
      {/* Scrolling content area: empty state or chat history */}
      <div className="run-scroll" ref={scrollRef}>
        {isEmpty ? (
          <div className="run-empty">
            <div className="run-empty-mark">◉</div>
            <h1 className="run-empty-title">{greeting()}</h1>
            {/* The one thing Claude-likes cannot do: Bob knows the state of
                your operation. One dry line; each segment opens its surface.
                Nothing to report = nothing shown. */}
            {(() => {
              const onShift = agents.filter(a => a.enabled && (a.charter ?? '').trim()).length;
              const segments: { label: string; panel: string }[] = [];
              if (onShift > 0) segments.push({ label: `${onShift} agent${onShift === 1 ? '' : 's'} on shift`, panel: 'agents' });
              if (workflows.length > 0) segments.push({ label: `${workflows.length} workflow${workflows.length === 1 ? '' : 's'}`, panel: 'teach' });
              if (docsThisWeek > 0) segments.push({ label: `${docsThisWeek} document${docsThisWeek === 1 ? '' : 's'} this week`, panel: 'documents' });
              if (segments.length === 0) return null;
              return (
                <p className="run-empty-status">
                  {segments.map((seg, i) => (
                    <span key={seg.panel}>
                      {i > 0 && <span className="run-empty-status-sep"> &middot; </span>}
                      <button className="run-empty-status-link" onClick={() => onOpenPanel(seg.panel)}>
                        {seg.label}
                      </button>
                    </span>
                  ))}
                </p>
              );
            })()}
            <div className="run-empty-suggestions">
              {SUGGESTIONS.map((s) => (
                <button
                  key={s}
                  className="run-empty-chip"
                  onClick={() => setSuggestion(s)}
                >
                  {s}
                </button>
              ))}
            </div>
            {/* Apps the user dragged a bridge onto. Nothing paired = nothing shown. */}
            <PairedApps />
          </div>
        ) : (
          <ChatHistory
            messages={messages}
            agents={agents}
            pendingToolCalls={[]}
            isTyping={orbState === 'thinking'}
          />
        )}
      </div>

      {/* Bob progress pill */}
      {bobStep && (
        <div className="bob-step-pill">
          <span className="bob-step-spinner" />
          <span className="bob-step-label">{bobStep}</span>
          <button
            className="bob-step-stop"
            onClick={() => {
              bobCancel().catch(() => {});
              setBobStep(null);
            }}
            title="Stop Bob"
          >
            Stop
          </button>
        </div>
      )}

      {/* Always-visible input */}
      <div className="run-input-dock">
        <ChatInput
          orbState={orbState}
          agents={agents}
          onMessageSent={onMessageSent}
          prefill={suggestion}
          onPrefillConsumed={() => setSuggestion(null)}
          useBob
        />
      </div>
    </div>
  );
}
