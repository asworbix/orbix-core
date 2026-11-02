import { useEffect, useRef, useState, useCallback, useMemo, memo } from 'react';
import { marked } from 'marked';
import { open } from '@tauri-apps/plugin-shell';
import type { Message, Agent, ToolCall } from '../types';
import ToolApproval from './ToolApproval';
import './ChatHistory.css';

interface Props {
  messages: Message[];
  agents: Agent[];
  pendingToolCalls: ToolCall[];
  isTyping?: boolean;
}

marked.setOptions({
  breaks: true,
  gfm: true,
});

export function renderMarkdown(text: string): string {
  // Step trail: [[steps:a|b|c]] renders as a quiet what-Bob-did strip
  // above the reply - tool transparency without raw tool calls.
  const withSteps = text.replace(
    /\[\[steps:([^\]]+)\]\]/g,
    (_m, list: string) => {
      const esc = (x: string) => x.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
      const items = list.split('|').map(s => s.trim()).filter(Boolean);
      if (!items.length) return '';
      return `<span class="bob-steps mono">` +
        `<span class="bob-steps-arrow">↳</span>` +
        items.map(i => `<span class="bob-step-item">${esc(i)}</span>`).join('<span class="bob-steps-sep">·</span>') +
        `</span>`;
    },
  );
  // Document chips: [[doc:id|kind|title]] markers (from create_document)
  // become interactive cards with Preview / Edit actions.
  const withChips = withSteps.replace(
    /\[\[doc:([a-zA-Z0-9-]+)\|(\w+)\|([^\]]+)\]\]/g,
    (_m, id, kind, title) => {
      const esc = (x: string) => x.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
      const kindLabel = kind === 'docx' ? 'WORD' : kind === 'pptx' ? 'SLIDES' : kind === 'website' ? 'SITE' : 'HTML';
      return `<span class="doc-chip" data-doc-id="${esc(id)}">`
        + `<span class="doc-chip-kind ${esc(kind)}">${kindLabel}</span>`
        + `<span class="doc-chip-title">${esc(title)}</span>`
        + `<button class="doc-chip-btn" data-doc-action="preview" data-doc-id="${esc(id)}">Preview</button>`
        + `<button class="doc-chip-btn" data-doc-action="edit" data-doc-id="${esc(id)}">Edit</button>`
        + `</span>`;
    },
  );
  return marked.parse(withChips, { async: false }) as string;
}

function formatTime(iso: string) {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch {
    return '';
  }
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });
  }, [text]);

  return (
    <button className={`msg-copy-btn ${copied ? 'copied' : ''}`} onClick={handleCopy} title="Copy" aria-label="Copy">
      {copied ? (
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      ) : (
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <rect x="9" y="9" width="13" height="13" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      )}
    </button>
  );
}

/** Strong signals that a line is the model's internal planning, not a reply
 *  to the user. Small local models (Gemma 4 E4B) often fail to wrap these in
 *  <think> tags even when instructed, so we recover them client-side. */
const PLANNING_LINE_RE = new RegExp(
  '^(' +
    // Self-narration
    'the\\s+user\\b|' +
    'i\\s+(?:must|need|should|will|am|have|\'ll|\'ve|\'m)\\b|' +
    // Labelled blocks
    'plan\\s*:|goal\\s*:|response\\s*tone\\s*:|tone\\s*:|approach\\s*:|' +
    'strategy\\s*:|reasoning\\s*:|step\\s*\\d+\\s*[:.]|action\\s*:|' +
    'context\\s*:|format\\s*:|note\\s*:|bias\\s+toward\\b|' +
    // Decision narration
    'check\\s+existing\\b|assess(?:ing)?\\s+fit\\b|design\\s*\\/\\s*confirm\\b|' +
    'acknowledge\\s+the\\b|propose\\s+a\\b|ask\\s+(?:clarifying|one)\\b|' +
    // Categorisation prose the model writes about the user message
    'this\\s+falls\\s+under\\b|this\\s+is\\s+(?:a|an)\\b|' +
    'here\'?s\\s+the\\s+plan\\b|workflow\\s+architect\\b' +
  ')',
  'i'
);

/** Pull <think>…</think> blocks out of a message body, OR - when the model
 *  forgets the tag - auto-detect leaked planning prose at the start and
 *  route it into the same disclosure. The visible reply stays clean; the
 *  user can expand to follow Bob's reasoning. */
function splitThinking(raw: string): { thinking: string; reply: string; streaming: boolean } {
  if (raw.includes('<think')) {
    const parts: string[] = [];
    let reply = raw;
    let streaming = false;
    reply = reply.replace(/<think>([\s\S]*?)<\/think>/gi, (_m, inner) => {
      parts.push(inner.trim());
      return '';
    });
    const openMatch = reply.match(/<think>([\s\S]*)$/i);
    if (openMatch) {
      parts.push(openMatch[1].trim());
      reply = reply.slice(0, openMatch.index ?? 0);
      streaming = true;
    }
    return { thinking: parts.join('\n\n').trim(), reply: reply.trim(), streaming };
  }
  return autoSplitPlanning(raw);
}

/** Heuristic split for messages that have no <think> tags but do start with
 *  obvious planning prose. Conservative: only kicks in when the *first*
 *  non-empty line matches a planning signal, then walks forward until it
 *  finds the first content line that looks like a real reply. */
function autoSplitPlanning(raw: string): { thinking: string; reply: string; streaming: boolean } {
  if (!raw.trim()) return { thinking: '', reply: raw, streaming: false };

  const lines = raw.split('\n');

  // Find the first non-empty line.
  let firstIdx = 0;
  while (firstIdx < lines.length && !lines[firstIdx].trim()) firstIdx++;
  if (firstIdx >= lines.length) return { thinking: '', reply: raw, streaming: false };

  // If it's not a planning line, treat the whole thing as a normal reply -
  // we don't want to hide content the user actually asked for.
  if (!PLANNING_LINE_RE.test(lines[firstIdx].trim())) {
    return { thinking: '', reply: raw, streaming: false };
  }

  // Walk forward through planning lines and blank lines until we find a
  // non-planning content line. That's where the user-facing reply starts.
  const thinkingLines: string[] = [];
  let replyStart = -1;
  for (let i = firstIdx; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (!trimmed) {
      thinkingLines.push(lines[i]);
      continue;
    }
    if (PLANNING_LINE_RE.test(trimmed)) {
      thinkingLines.push(lines[i]);
      continue;
    }
    replyStart = i;
    break;
  }

  if (replyStart === -1) {
    // Stream is mid-planning, or the model never produced a reply. Show
    // everything as live thinking so the user can still follow along.
    return {
      thinking: lines.slice(firstIdx).join('\n').trim(),
      reply: '',
      streaming: true,
    };
  }

  return {
    thinking: [...lines.slice(0, firstIdx), ...thinkingLines].join('\n').trim(),
    reply: lines.slice(replyStart).join('\n').trim(),
    streaming: false,
  };
}

// ─────────────────────────────────────────────
//  Structured input wizard (forces Q&A instead of prose ramble)
// ─────────────────────────────────────────────

type InputKind = 'text' | 'select' | 'yesno' | 'longtext';

interface InputQuestion {
  id: string;
  label: string;
  kind?: InputKind;
  placeholder?: string;
  options?: string[];
  required?: boolean;
}

/** Pull the first <inputs>…</inputs> JSON block out of a reply. Returns the
 *  parsed questions and the reply text with the tag stripped. Tolerant: bad
 *  JSON or unclosed tags fall back to leaving the reply alone. */
function extractInputs(reply: string): { questions: InputQuestion[]; reply: string } {
  if (!reply.includes('<inputs')) return { questions: [], reply };
  const closed = reply.match(/<inputs>([\s\S]*?)<\/inputs>/i);
  if (!closed) {
    // Unclosed tag mid-stream: hide the open tag from the visible reply so
    // the user doesn't see raw markup while it's still arriving.
    const open = reply.match(/<inputs>[\s\S]*$/i);
    if (open) {
      return { questions: [], reply: reply.slice(0, open.index ?? 0).trim() };
    }
    return { questions: [], reply };
  }
  let questions: InputQuestion[] = [];
  try {
    const parsed = JSON.parse(closed[1].trim());
    if (Array.isArray(parsed)) {
      questions = parsed.filter((q) => q && typeof q.id === 'string' && typeof q.label === 'string');
    } else if (parsed && Array.isArray(parsed.questions)) {
      questions = parsed.questions.filter((q: any) => q && typeof q.id === 'string' && typeof q.label === 'string');
    }
  } catch {
    // bad JSON - leave reply intact, drop the block silently
  }
  // Hard gate: Bob never solicits secrets through the wizard. A model
  // that hallucinates a credentials form (seen 2026-08-03: 'Calendar
  // Access Credentials - API keys, refresh tokens') gets its question
  // dropped here, mechanically, whatever the prompt said.
  const asksSecret = (q: InputQuestion) =>
    /api.?key|token|password|credential|secret|client.?id|refresh/i.test(
      `${q.label} ${q.placeholder ?? ''} ${q.id}`,
    );
  questions = questions.filter((q) => !asksSecret(q));
  const stripped = (reply.slice(0, closed.index ?? 0) + reply.slice((closed.index ?? 0) + closed[0].length)).trim();
  return { questions, reply: stripped };
}

function InputsWizard({ questions, messageId }: { questions: InputQuestion[]; messageId: string }) {
  const [cursor, setCursor] = useState(0);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [draft, setDraft] = useState('');
  const [submitted, setSubmitted] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Persist completion state across re-renders so the wizard stays collapsed.
  const storageKey = `orbix-inputs-${messageId}`;
  useEffect(() => {
    try {
      if (sessionStorage.getItem(storageKey) === 'done') setSubmitted(true);
    } catch {}
  }, [storageKey]);

  useEffect(() => {
    if (submitted) return;
    const t = setTimeout(() => {
      inputRef.current?.focus();
      textareaRef.current?.focus();
    }, 60);
    return () => clearTimeout(t);
  }, [cursor, submitted]);

  if (!questions.length || submitted) return null;

  const q = questions[cursor];
  const kind: InputKind = q.kind ?? 'text';
  const isLast = cursor === questions.length - 1;
  const canAdvance = q.required === false || draft.trim().length > 0;

  const advance = (value: string) => {
    const nextAnswers = { ...answers, [q.id]: value };
    setAnswers(nextAnswers);
    setDraft('');
    if (isLast) {
      submitAll(nextAnswers);
    } else {
      setCursor((c) => c + 1);
    }
  };

  const submitAll = async (final: Record<string, string>) => {
    setSubmitted(true);
    try { sessionStorage.setItem(storageKey, 'done'); } catch {}
    const lines = questions.map((qq) => `${qq.label}: ${final[qq.id] ?? ''}`).join('\n');
    const payload = `Answers:\n${lines}`;
    // Dispatch through the same path a typed message takes so App.tsx
    // optimistically pushes a visible user bubble + calls bobSend exactly
    // once. Calling bobSend here directly skipped the optimistic step,
    // which is why the user couldn't see their own answers in chat.
    window.dispatchEvent(new CustomEvent('orbix:wizard-answer', { detail: { text: payload } }));
  };

  const skipBack = () => {
    if (cursor === 0) return;
    setCursor((c) => c - 1);
    setDraft(answers[questions[cursor - 1].id] ?? '');
  };

  return (
    <div className="msg-inputs">
      <div className="msg-inputs-head">
        <span className="msg-inputs-eyebrow mono">BOB NEEDS</span>
        <span className="msg-inputs-progress mono">
          {String(cursor + 1).padStart(2, '0')} / {String(questions.length).padStart(2, '0')}
        </span>
      </div>

      <div className="msg-inputs-q">
        <label className="msg-inputs-label">{q.label}</label>

        {kind === 'select' && q.options && q.options.length > 0 ? (
          <div className="msg-inputs-options">
            {q.options.map((opt) => (
              <button
                key={opt}
                type="button"
                className="msg-inputs-option"
                onClick={() => advance(opt)}
              >
                {opt}
              </button>
            ))}
          </div>
        ) : kind === 'yesno' ? (
          <div className="msg-inputs-options">
            <button type="button" className="msg-inputs-option" onClick={() => advance('yes')}>yes</button>
            <button type="button" className="msg-inputs-option" onClick={() => advance('no')}>no</button>
          </div>
        ) : kind === 'longtext' ? (
          <textarea
            ref={textareaRef}
            className="msg-inputs-textarea"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={q.placeholder ?? ''}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && canAdvance) {
                e.preventDefault();
                advance(draft.trim());
              }
            }}
          />
        ) : (
          <input
            ref={inputRef}
            type="text"
            className="msg-inputs-input"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={q.placeholder ?? ''}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && canAdvance) {
                e.preventDefault();
                advance(draft.trim());
              }
            }}
          />
        )}
      </div>

      <div className="msg-inputs-foot">
        <button
          type="button"
          className="msg-inputs-back"
          onClick={skipBack}
          disabled={cursor === 0}
        >
          ← back
        </button>
        {kind !== 'select' && kind !== 'yesno' && (
          <button
            type="button"
            className="msg-inputs-next"
            onClick={() => canAdvance && advance(draft.trim())}
            disabled={!canAdvance}
          >
            {isLast ? 'Send to Bob' : 'Next →'}
          </button>
        )}
      </div>
    </div>
  );
}

function ThinkingBlock({ text, streaming }: { text: string; streaming: boolean }) {
  const [open, setOpen] = useState(false);
  if (!text) return null;
  return (
    <div className={`msg-thinking ${open ? 'open' : ''} ${streaming ? 'streaming' : ''}`}>
      <button
        type="button"
        className="msg-thinking-toggle"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <svg className="msg-thinking-chev" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
          <polyline points="6 9 12 15 18 9" />
        </svg>
        <span>{streaming ? 'Reasoning…' : 'Reasoning'}</span>
      </button>
      {open && (
        <div className="msg-thinking-body">{text}</div>
      )}
    </div>
  );
}

// memo'd so a streaming turn only re-renders the one bubble whose content
// changed: the chunk handler in App rebuilds the messages array but keeps every
// other message's object identity, so the shallow prop compare skips them.
// Without this, every bubble's markdown re-parsed on every token (quadratic in
// a long chat).
const MessageBubble = memo(function MessageBubble({ msg, agent, isContinuation }: { msg: Message; agent: Agent | null; isContinuation?: boolean }) {
  const isUser = msg.role === 'user';
  const senderName = agent?.name || 'Bob';

  const handleLinkClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    // Document chip actions (Preview / Edit) win over plain links.
    const docBtn = target.closest('[data-doc-action]');
    if (docBtn) {
      e.preventDefault();
      const id = docBtn.getAttribute('data-doc-id') ?? '';
      const action = docBtn.getAttribute('data-doc-action');
      if (action === 'preview') {
        import('../hooks/api').then(({ listGeneratedDocuments, previewGeneratedDocument }) =>
          listGeneratedDocuments().then(docs => {
            const d = docs.find(x => x.id === id);
            if (d) previewGeneratedDocument(d.path).catch(() => {});
          }),
        ).catch(() => {});
      } else {
        window.dispatchEvent(new CustomEvent('orbix:open-documents', { detail: { id } }));
      }
      return;
    }
    const anchor = target.closest('a');
    if (anchor && anchor.href) {
      e.preventDefault();
      open(anchor.href).catch(() => {});
    }
  }, []);

  // Computed before the user-message early return so the hooks below always
  // run in the same order (rules of hooks). A user bubble ignores these.
  const { thinking, reply: rawReply, streaming } = splitThinking(msg.content);
  const { questions, reply } = extractInputs(rawReply);
  // Parse markdown only when the reply text actually changes, not on every
  // parent render.
  const replyHtml = useMemo(() => renderMarkdown(reply), [reply]);

  if (isUser) {
    return (
      <div className="chat-message user">
        <div className="msg-bubble user-bubble">{msg.content}</div>
        <div className="msg-time user-time">{formatTime(msg.timestamp)}</div>
      </div>
    );
  }

  // Nothing visible has arrived yet (empty bubble waiting on the first
  // token). Show the loading dots INSIDE this bubble so the single "Bob"
  // label carries them - the parent suppresses its standalone typing row
  // whenever a trailing assistant bubble exists, so "Bob" never appears
  // twice at once.
  const awaitingFirstToken = !thinking && !reply && questions.length === 0;

  return (
    <div className={`chat-message assistant ${isContinuation ? 'is-continuation' : ''}`}>
      {!isContinuation && (
        <div className="msg-label">
          <span className="msg-label-mark" />
          <span className="msg-label-name">{senderName}</span>
          <span className="msg-time">{formatTime(msg.timestamp)}</span>
        </div>
      )}
      {thinking && <ThinkingBlock text={thinking} streaming={streaming} />}
      {reply && (
        <div className="msg-content-wrap">
          <div
            className="msg-content assistant-content"
            onClick={handleLinkClick}
            dangerouslySetInnerHTML={{ __html: replyHtml }}
          />
          <div className="msg-actions">
            <CopyButton text={reply || msg.content} />
          </div>
        </div>
      )}
      {awaitingFirstToken && (
        <div className="msg-typing">
          <span className="typing-dot" />
          <span className="typing-dot" />
          <span className="typing-dot" />
        </div>
      )}
      {questions.length > 0 && <InputsWizard questions={questions} messageId={msg.id} />}
    </div>
  );
});

export default function ChatHistory({ messages, agents, pendingToolCalls, isTyping }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  // Instant, not smooth: this fires on every stream chunk, and queuing a smooth
  // animation per token both janks and fights RunView's instant scrollTop on
  // the same container. 'auto' just keeps the view pinned to the latest text.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'auto' });
  }, [messages, pendingToolCalls]);

  if (messages.length === 0 && pendingToolCalls.length === 0 && !isTyping) {
    return null;
  }

  // Hide wizard answer submissions ("Answers: ...") and mark the assistant
  // reply that follows as a continuation of the previous Bob turn so the
  // chat reads as a single resolved interaction instead of two stacked
  // "Bob 19:09" bubbles separated by an Answers blob.
  const hiddenIds = new Set<string>();
  const continuationIds = new Set<string>();
  for (let i = 0; i < messages.length; i++) {
    const m = messages[i];
    if (m.role === 'user' && m.content.startsWith('Answers:')) {
      hiddenIds.add(m.id);
      const next = messages[i + 1];
      if (next && next.role === 'assistant') continuationIds.add(next.id);
    }
  }

  // The standalone typing row (its own "Bob" label + dots) must not show
  // while a trailing assistant bubble is already on screen - that bubble
  // owns the label and, when still empty, its own dots. Showing both is
  // what rendered "Bob" twice during an answer.
  const lastMsg = messages[messages.length - 1];
  const assistantBubblePresent = !!lastMsg && lastMsg.role === 'assistant';
  const showStandaloneTyping = isTyping && !assistantBubblePresent;

  return (
    <div className="chat-history">
      <div className="chat-history-inner">
        {messages.map((msg) => {
          if (hiddenIds.has(msg.id)) return null;
          const agent = msg.agent_id ? agents.find((a) => a.id === msg.agent_id) ?? null : null;
          return <MessageBubble key={msg.id} msg={msg} agent={agent} isContinuation={continuationIds.has(msg.id)} />;
        })}

        {pendingToolCalls.map((call) => (
          <div key={call.id} className="chat-message assistant">
            <div className="msg-label">
              <span className="msg-label-mark" />
              <span className="msg-label-name">Bob</span>
            </div>
            <ToolApproval call={call} onResolved={() => {}} />
          </div>
        ))}

        {showStandaloneTyping && (
          <div className="chat-message assistant">
            <div className="msg-label">
              <span className="msg-label-mark" />
              <span className="msg-label-name">Bob</span>
            </div>
            <div className="msg-typing">
              <span className="typing-dot" />
              <span className="typing-dot" />
              <span className="typing-dot" />
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
