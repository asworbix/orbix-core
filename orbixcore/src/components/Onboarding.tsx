import { useState, useCallback, useEffect } from 'react';
import {
  completeOnboarding,
  bobSend,
  requestVoicePermissions,
} from '../hooks/api';
import './Onboarding.css';

interface Props {
  onComplete: () => void;
}

const SUGGESTIONS = [
  'Triage my CRM pipeline every morning',
  'Send me a daily Slack standup of agent activity',
  'Build a dashboard of today\'s deals and invoices',
  'Watch my inbox for urgent customer messages',
  'Just let me chat with Bob - I\'ll figure it out',
];

export default function Onboarding({ onComplete }: Props) {
  const [name, setName] = useState('');
  const [intent, setIntent] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // Soft-trigger permission probe in the background - no UI gate. The
  // user can grant permissions later from Settings if they decline now.
  useEffect(() => {
    requestVoicePermissions().catch(() => {});
  }, []);

  const finish = useCallback(
    async (sendIntent: boolean) => {
      if (submitting) return;
      setSubmitting(true);
      try {
        await completeOnboarding(name.trim() || 'there');
        if (sendIntent && intent.trim()) {
          // Fire-and-forget: Bob streams a reply that proposes a starter
          // setup via <action> tags. The conversation will populate via
          // the streaming events App.tsx is already listening for.
          bobSend(intent.trim()).catch(() => {});
        }
        onComplete();
      } catch {
        setSubmitting(false);
      }
    },
    [name, intent, submitting, onComplete],
  );

  const handleSubmit = useCallback(
    (e?: React.FormEvent) => {
      e?.preventDefault();
      if (!intent.trim()) return;
      finish(true);
    },
    [intent, finish],
  );

  return (
    <div className="onboarding-overlay">
      <div className="onboarding-screen">
        <div className="onboarding-mark">◉</div>
        <h1 className="onboarding-title">
          {name.trim() ? `Hi ${name.trim()}.` : 'Hi.'} What do you want Bob to do first?
        </h1>
        <p className="onboarding-sub">
          Tell Bob in plain English. He'll set up the agents, workflows, and
          panels you need - or just chat if you'd rather start small.
        </p>

        <form className="onboarding-form" onSubmit={handleSubmit}>
          <textarea
            className="onboarding-intent"
            placeholder="I want Bob to..."
            value={intent}
            onChange={(e) => setIntent(e.target.value)}
            rows={3}
            autoFocus
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                handleSubmit();
              }
            }}
          />

          <div className="onboarding-chips">
            {SUGGESTIONS.map((s) => (
              <button
                key={s}
                type="button"
                className="onboarding-chip"
                onClick={() => setIntent(s)}
              >
                {s}
              </button>
            ))}
          </div>

          <div className="onboarding-name-row">
            <label className="onboarding-name-label" htmlFor="onboarding-name">
              Your name <span className="onboarding-name-hint">(optional)</span>
            </label>
            <input
              id="onboarding-name"
              className="onboarding-name"
              placeholder="So Bob knows what to call you"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div className="onboarding-actions">
            <button
              type="button"
              className="onboarding-btn ghost"
              onClick={() => finish(false)}
              disabled={submitting}
            >
              Skip for now
            </button>
            <button
              type="submit"
              className="onboarding-btn primary"
              disabled={submitting || !intent.trim()}
            >
              {submitting ? 'Setting up…' : 'Let\'s go'}
            </button>
          </div>
        </form>

        <p className="onboarding-foot">
          You can grant Accessibility &amp; Files access from Settings whenever
          you'd like - voice and local models work best with both.
        </p>
      </div>
    </div>
  );
}
