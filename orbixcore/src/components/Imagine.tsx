import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import {
  imagineStart, imagineStatus, imagineNudge, imagineStop,
  onImagineStarted, onImagineEnded, onImagineStep, onMessageChunk,
  listGeneratedDocuments, previewGeneratedDocument,
  type ImagineRun,
} from '../hooks/api';
import { renderMarkdown } from './ChatHistory';
import './Imagine.css';

/**
 * Imagine - Bob with free rein. One click starts a run of Bob's agent
 * loop with no request: he looks at what is open, reads what matters,
 * and makes one real thing. This overlay shows what he saw, what he is
 * doing, and what he made. A hint typed while he works reaches him
 * between turns; the finished run also lands in the chat.
 */

type Status = 'starting' | 'running' | 'done' | 'stopped' | 'failed';
interface Block { role: 'bob' | 'you'; text: string }

interface Props {
  open: boolean;
  /** Optional hint from the chat box ("imagine something for sales"). */
  seed: string;
  onClose: () => void;
}

/* The stream carries the model's raw turn text; hide think blocks and
   action tag bodies so the trail reads as Bob thinking out loud. */
function stripMachinery(s: string): string {
  return s
    .replace(/<think>[\s\S]*?(<\/think>|$)/g, '')
    .replace(/<action>[\s\S]*?(<\/action>|$)/g, '')
    .trim();
}

const STATUS_LINE: Record<Exclude<Status, 'running' | 'starting'>, string> = {
  done: 'Done',
  stopped: 'Stopped',
  failed: 'That did not work',
};

export default function Imagine({ open: isOpen, seed, onClose }: Props) {
  const [status, setStatus] = useState<Status>('starting');
  const [run, setRun] = useState<ImagineRun | null>(null);
  const [step, setStep] = useState<string | null>(null);
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [live, setLive] = useState('');
  const [result, setResult] = useState('');
  const [error, setError] = useState('');
  const [draft, setDraft] = useState('');
  const liveRef = useRef('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const busy = status === 'starting' || status === 'running';

  const reset = useCallback(() => {
    setRun(null);
    setStep(null);
    setBlocks([]);
    liveRef.current = '';
    setLive('');
    setResult('');
    setError('');
  }, []);

  const begin = useCallback(async (hint: string) => {
    reset();
    setStatus('starting');
    try {
      setRun(await imagineStart(hint));
    } catch (e) {
      setError(String(e));
      setStatus('failed');
    }
  }, [reset]);

  // Opening the overlay re-attaches to a run already in progress (the
  // user closed it mid-run) and otherwise starts one.
  useEffect(() => {
    if (!isOpen) return;
    let stale = false;
    imagineStatus()
      .then(existing => {
        if (stale) return;
        if (existing) {
          reset();
          setRun(existing);
          setStatus('running');
        } else {
          begin(seed);
        }
      })
      .catch(() => { if (!stale) begin(seed); });
    return () => { stale = true; };
  }, [isOpen, seed, begin, reset]);

  useEffect(() => {
    if (!isOpen) return;
    const subs = [
      onImagineStarted(r => { setRun(r); setStatus('running'); }),
      onImagineStep((_id, label) => setStep(label)),
      onMessageChunk(e => {
        if (!e.message_id.startsWith('imagine-')) return;
        const clean = stripMachinery(e.content ?? '');
        const prev = liveRef.current;
        // Each agentic turn restarts the cumulative stream for the same
        // id; a shrink means the previous turn finished - archive it.
        if (prev && clean.length < prev.length) {
          setBlocks(b => [...b, { role: 'bob', text: prev }]);
        }
        liveRef.current = clean;
        setLive(clean);
      }),
      onImagineEnded(e => {
        setStatus(e.stopped ? 'stopped' : e.ok ? 'done' : 'failed');
        setStep(null);
        liveRef.current = '';
        setLive('');
        setResult(e.result);
        setError(e.error ?? '');
      }),
    ];
    return () => { subs.forEach(p => p.then(un => un())); };
  }, [isOpen]);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [live, blocks.length, result, error]);

  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isOpen, onClose]);

  const submit = async () => {
    const text = draft.trim();
    setDraft('');
    if (busy) {
      if (!text) return;
      setBlocks(b => [...b, { role: 'you', text }]);
      try { await imagineNudge(text); } catch { /* the run just ended */ }
    } else {
      begin(text);
    }
  };

  // Document chips and links inside the rendered result behave as they
  // do in chat: preview opens the file, edit opens Documents, links
  // open in the browser.
  const handleResultClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const docBtn = target.closest('[data-doc-action]');
    if (docBtn) {
      e.preventDefault();
      const id = docBtn.getAttribute('data-doc-id') ?? '';
      if (docBtn.getAttribute('data-doc-action') === 'preview') {
        listGeneratedDocuments()
          .then(docs => {
            const d = docs.find(x => x.id === id);
            if (d) previewGeneratedDocument(d.path).catch(() => {});
          })
          .catch(() => {});
      } else {
        onClose();
        window.dispatchEvent(new CustomEvent('orbix:open-documents', { detail: { id } }));
      }
      return;
    }
    const anchor = target.closest('a');
    if (anchor && anchor.href) {
      e.preventDefault();
      openExternal(anchor.href).catch(() => {});
    }
  };

  if (!isOpen) return null;

  const statusLine = busy
    ? (step ?? (status === 'starting' ? 'Looking around…' : 'Working…'))
    : STATUS_LINE[status];
  const seen = run?.scan ?? [];

  return createPortal(
    <div className="imagine-overlay" role="dialog" aria-label="Imagine">
      <div className="imagine-panel">
        <header className="imagine-header">
          <span className={`imagine-dot ${status}`} aria-hidden="true" />
          <div className="imagine-title">
            <span className="imagine-name mono">IMAGINE</span>
            <span className="imagine-status">{statusLine}</span>
          </div>
          {busy && (
            <button className="imagine-stop" onClick={() => imagineStop().catch(() => {})}>Stop</button>
          )}
          <button className="imagine-close" onClick={onClose} aria-label="Close">×</button>
        </header>

        <div className="imagine-seen">
          <span className="imagine-seen-label mono">WHAT BOB SAW</span>
          {seen.length === 0 ? (
            <span className="imagine-muted">
              {status === 'starting' ? 'Looking at what is open…' : 'Nothing notable open'}
            </span>
          ) : (
            <ul className="imagine-seen-list">
              {seen.slice(0, 8).map(a => {
                const hint = a.tabs[0] ?? a.windows[0];
                return (
                  <li key={a.app} className="imagine-seen-app">
                    <span className="imagine-seen-name">{a.app}</span>
                    {hint && <span className="imagine-seen-hint">{hint}</span>}
                    {a.tabs.length > 1 && <span className="imagine-seen-count mono">{a.tabs.length} tabs</span>}
                  </li>
                );
              })}
              {seen.length > 8 && <li className="imagine-seen-app imagine-muted">+{seen.length - 8} more</li>}
            </ul>
          )}
        </div>

        <div className="imagine-transcript" ref={scrollRef}>
          {run?.seed && <div className="imagine-block you">{run.seed}</div>}
          {blocks.map((b, i) => (
            <div key={i} className={`imagine-block ${b.role}`}>{b.text}</div>
          ))}
          {live && <div className="imagine-block bob live">{live}</div>}
          {busy && !live && (
            <div className="imagine-thinking" aria-hidden="true"><span /><span /><span /></div>
          )}
          {!busy && result && (
            <div
              className={`imagine-result ${status}`}
              onClick={handleResultClick}
              dangerouslySetInnerHTML={{ __html: renderMarkdown(result) }}
            />
          )}
          {status === 'failed' && (
            <div className="imagine-result failed">
              <p className="imagine-error">{error || 'Bob could not finish this one.'}</p>
            </div>
          )}
        </div>

        <div className="imagine-composer">
          <input
            value={draft}
            onChange={e => setDraft(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submit(); } }}
            placeholder={busy ? 'Nudge Bob while he works…' : 'Imagine again, with a hint if you like…'}
            autoFocus
          />
          <button className="imagine-btn imagine-btn-primary" onClick={submit} disabled={busy && !draft.trim()}>
            {busy ? 'Nudge' : 'Again'}
          </button>
          {!busy && <button className="imagine-btn" onClick={onClose}>Done</button>}
        </div>
      </div>
    </div>,
    document.body,
  );
}
