import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { onBobStep, bobCancel, quickBarAsk, quickBarHide, quickBarOpenMain } from '../hooks/api';
import { renderMarkdown } from './ChatHistory';
import './QuickBar.css';

/* The quick bar: Bob from anywhere. A global shortcut shows this window
   over whatever app is in front; Enter runs the same Bob turn as the chat
   (it shows up there too), the bar shows his progress and the answer.
   Esc or clicking away hides it; the window is reused, so a turn keeps
   running out of sight and the shortcut brings the answer back. The
   window grows to fit the answer, up to MAX_HEIGHT, then scrolls. */

/* Vite dev only: ?demo=1 seeds an answered question so the bar can be
   screenshotted in a plain browser, without the app. */
const DEMO = import.meta.env.DEV && new URLSearchParams(window.location.search).get('demo') === '1';
const DEMO_REPLY = '[[steps:Running `git branch --show-current`…]]\n\nYou are on **one-bob**, 4 commits ahead of `headless-cli`.';

/* Outside Tauri (the demo) there is no window to resize or watch. */
function tauriWindow() {
  try { return getCurrentWindow(); } catch { return null; }
}

const WIDTH = 680;
const MIN_HEIGHT = 72;
const MAX_HEIGHT = 520;

export default function QuickBar() {
  const [text, setText] = useState('');
  const [asked, setAsked] = useState(DEMO ? 'what branch is this?' : '');
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<string | null>(null);
  const [reply, setReply] = useState(DEMO ? DEMO_REPLY : '');
  const [error, setError] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  // The window is transparent; only the rounded box is drawn.
  useEffect(() => {
    document.documentElement.classList.add('quick-bar-root');
  }, []);

  // Every time the bar comes forward, the cursor is ready in the input.
  useEffect(() => {
    const focus = () => {
      inputRef.current?.focus();
      inputRef.current?.select();
    };
    focus();
    const un = tauriWindow()?.onFocusChanged(({ payload }) => {
      if (payload) focus();
    });
    return () => { un?.then(f => f()).catch(() => {}); };
  }, []);

  useEffect(() => {
    if (!busy) return;
    const un = onBobStep(label => { if (label) setStep(label); });
    return () => { un.then(f => f()).catch(() => {}); };
  }, [busy]);

  // Fit the window to what is shown.
  useLayoutEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    const h = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.ceil(box.scrollHeight)));
    tauriWindow()?.setSize(new LogicalSize(WIDTH, h)).catch(() => {});
  }, [busy, step, reply, error, asked]);

  const send = useCallback(() => {
    const q = text.trim();
    if (!q || busy) return;
    setAsked(q);
    setText('');
    setReply('');
    setError('');
    setStep('Thinking…');
    setBusy(true);
    quickBarAsk(q)
      .then(msg => setReply(msg.content))
      .catch(e => setError(String(e)))
      .finally(() => { setBusy(false); setStep(null); });
  }, [text, busy]);

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      quickBarHide().catch(() => {});
    }
  };

  const html = useMemo(() => (reply ? renderMarkdown(reply) : ''), [reply]);

  // Anything that needs the full app (a document chip, a panel) opens it.
  const onReplyClick = (e: React.MouseEvent) => {
    const t = e.target as HTMLElement;
    if (t.closest('[data-doc-action], a')) {
      e.preventDefault();
      quickBarOpenMain().catch(() => {});
    }
  };

  const showBody = busy || reply || error;

  return (
    <div className="qb" ref={boxRef}>
      <div className="qb-row">
        <span className={`qb-mark ${busy ? 'busy' : ''}`} aria-hidden />
        <input
          ref={inputRef}
          className="qb-input"
          value={text}
          onChange={e => setText(e.target.value)}
          onKeyDown={onKey}
          placeholder={busy ? 'Bob is working…' : 'Ask Bob anything'}
          spellCheck={false}
          autoFocus
        />
        <span className="qb-hint">esc</span>
      </div>
      {showBody && (
        <div className="qb-body">
          {asked && <div className="qb-asked">{asked}</div>}
          {busy && step && <div className="qb-step">{step}</div>}
          {error && <div className="qb-error">{error}</div>}
          {reply && (
            <div
              className="qb-reply msg-content assistant-content"
              onClick={onReplyClick}
              dangerouslySetInnerHTML={{ __html: html }}
            />
          )}
          <div className="qb-actions">
            {busy && <button className="qb-btn" onClick={() => bobCancel().catch(() => {})}>Stop</button>}
            <button className="qb-btn" onClick={() => quickBarOpenMain().catch(() => {})}>Open in Orbix Core</button>
          </div>
        </div>
      )}
    </div>
  );
}
