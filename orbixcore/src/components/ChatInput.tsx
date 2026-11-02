import { useState, useRef, useCallback, useEffect } from 'react';
import type { OrbState, Message, Agent, Workflow, WebhookEndpoint, ConnectorInstance } from '../types';
import {
  sendMessage, bobSend, prewarmLlm, voiceConversationSet, listWorkflows, runWorkflow,
  listWebhooks, listConnectorInstances, listGitRepos, listSkills,
  startListening, stopListening, speakText, stopSpeaking,
  onSilenceDetected, onAudioLevel,
  onMessageChunk, onMessageStreamEnd,
  getLocalLlmStatus,
} from '../hooks/api';
import type { RepoInfo, SkillMeta } from '../hooks/api';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import ModelSelector from './ModelSelector';
import './ChatInput.css';

interface Props {
  orbState: OrbState;
  agents: Agent[];
  onMessageSent: (msg: Message) => void;
  prefill?: string | null;
  onPrefillConsumed?: () => void;
  useBob?: boolean;
}

export default function ChatInput({ orbState, agents, onMessageSent, prefill, onPrefillConsumed, useBob }: Props) {
  const [value, setValue] = useState('');
  const [loading, setLoading] = useState(false);
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [webhooks, setWebhooks] = useState<WebhookEndpoint[]>([]);
  const [connectors, setConnectors] = useState<ConnectorInstance[]>([]);
  const [showSlash, setShowSlash] = useState(false);
  const [slashFilter, setSlashFilter] = useState('');
  const [slashIndex, setSlashIndex] = useState(0);
  // Index of the "/" that opened the palette, so a selection replaces the
  // "/token" in place instead of clobbering the whole message.
  const [slashStart, setSlashStart] = useState(-1);
  const [scopedAgent, setScopedAgent] = useState<Agent | null>(null);
  // Repo scope (/repo): subsequent sends carry the repo's path/branch as
  // hidden context so Bob's file/shell tools target that project.
  const [repos, setRepos] = useState<RepoInfo[]>([]);
  const [skillFiles, setSkillFiles] = useState<SkillMeta[]>([]);
  const [scopedRepo, setScopedRepo] = useState<RepoInfo | null>(null);
  // Files dropped onto the chat (absolute paths); attached to the next send
  // so Bob file_reads them. dragActive drives the drop overlay.
  const [attachedFiles, setAttachedFiles] = useState<string[]>([]);
  // Whether the loaded model can see images. Null until we've checked. Drives
  // the pre-send hint when someone attaches an image to a text-only model.
  const [visionCapable, setVisionCapable] = useState<boolean | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const [isRecording, setIsRecording] = useState(false);
  const [conversationActive, setConversationActive] = useState(false);
  const [conversationPhase, setConversationPhase] = useState<'idle' | 'listening' | 'transcribing' | 'thinking' | 'speaking'>('idle');
  const [audioLevel, setAudioLevel] = useState(0);
  const [maxAudioLevel, setMaxAudioLevel] = useState(0);

  // Streaming TTS pipeline state
  const ttsQueueRef = useRef<string[]>([]);
  const ttsProcessingRef = useRef(false);
  const streamBufferRef = useRef('');
  const queuedSentenceCountRef = useRef(0);
  const activeStreamIdRef = useRef<string | null>(null);
  const streamCompleteRef = useRef(false);
  const conversationActiveRef = useRef(false);
  const isRecordingRef = useRef(false);
  const processingRef = useRef(false); // Guard against concurrent silence triggers
  const phaseRef = useRef<'idle' | 'listening' | 'transcribing' | 'thinking' | 'speaking'>('idle');
  const bargeInCounterRef = useRef(0); // consecutive high-energy frames during 'speaking'
  const bargingInRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  conversationActiveRef.current = conversationActive;
  isRecordingRef.current = isRecording;
  phaseRef.current = conversationPhase;

  const activeAgent = agents.find(a => a.enabled);

  // ── Sentence-streaming TTS helpers ──
  // Strip markdown / action JSON / code so TTS doesn't phonemize garbage.
  const cleanForSpeech = (text: string): string => {
    return text
      // Reasoning must NEVER be spoken. The stream keeps a think block
      // OPEN until user-facing content starts, so strip the unclosed
      // trailing form too - that is exactly what TTS sees mid-stream.
      .replace(/<think>[\s\S]*?<\/think>/g, '')
      .replace(/<think>[\s\S]*$/g, '')
      .replace(/<action>[\s\S]*?<\/action>/g, '')
      .replace(/\[\[[^\]]*\]\]/g, '')
      .replace(/```[\s\S]*?```/g, '')
      .replace(/`[^`]*`/g, '')
      .replace(/\*\*([^*]+)\*\*/g, '$1')
      .replace(/\*([^*]+)\*/g, '$1')
      .replace(/[#_~]/g, '')
      .replace(/-\s*Bob\s*◉?/g, '')
      .replace(/Bob\s*◉/g, '')
      .replace(/\n+/g, ' ')
      .replace(/\s{2,}/g, ' ')
      .trim();
  };

  // Extract complete sentences from a buffer. Returns [sentences, remainder].
  // Handles abbreviations (Dr., Mr., Mrs., e.g., i.e., etc.) and ellipsis (...)
  // without splitting mid-word.
  const extractSentences = (buf: string): { sentences: string[]; remainder: string } => {
    const matches: string[] = [];
    let lastEnd = 0;
    // Negative lookbehind for common abbreviations that end with "."
    // Also require the punctuation to be followed by space + capital letter or end of string
    const re = /([^.!?]+(?:(?:Dr|Mr|Mrs|Ms|Jr|Sr|St|vs|etc|e\.g|i\.e|Prof|Inc|Ltd|Corp|Co|No|Vol)\.\s*)*[.!?]+)(?=\s+[A-Z]|\s*$)/g;
    let m;
    while ((m = re.exec(buf)) !== null) {
      const sentence = m[1].trim();
      if (sentence.length > 2) {
        matches.push(sentence);
      }
      lastEnd = m.index + m[0].length;
    }
    return { sentences: matches.filter(s => s.length > 0), remainder: buf.slice(lastEnd) };
  };

  // Spoken acknowledgment for the start of a voice turn - Bob says a
  // short varied "on it" while the model reasons, instead of dead air.
  // Recent picks are remembered so he never repeats himself back to back.
  const ackRecentRef = useRef<string[]>([]);
  const pickAck = useCallback((): string => {
    const POOL = [
      'Let me check.', 'Let me think about that.', 'Sure, looking into it.',
      'On it.', 'Give me a second.', 'Checking now.', 'Let me have a look.',
      'One moment.', 'Digging in.', 'Let me see.', 'Taking a look now.',
      'Right, let me look.', 'Working on it.', 'Let me pull that up.',
      'Hmm, let me think.', 'Good question, checking.',
    ];
    const recent = ackRecentRef.current;
    const candidates = POOL.filter(p => !recent.includes(p));
    const pick = candidates[Math.floor(Math.random() * candidates.length)];
    ackRecentRef.current = [...recent, pick].slice(-4);
    return pick;
  }, []);

  // Process the TTS queue serially - one sentence at a time, blocks on each.
  const processSpeechQueue = useCallback(async () => {
    if (ttsProcessingRef.current) return;
    ttsProcessingRef.current = true;
    while (ttsQueueRef.current.length > 0 && conversationActiveRef.current) {
      const sentence = ttsQueueRef.current.shift()!;
      const cleaned = cleanForSpeech(sentence);
      if (cleaned.length < 2) continue;
      try {
        await speakText(cleaned);
      } catch (e) {
        console.error('[voice] TTS sentence failed:', e);
      }
    }
    ttsProcessingRef.current = false;
  }, []);

  // Push a chunk of streaming text - extract complete sentences and queue them.
  const pushStreamChunk = useCallback((accumulatedContent: string) => {
    streamBufferRef.current = accumulatedContent;
    const cleaned = cleanForSpeech(accumulatedContent);
    const { sentences } = extractSentences(cleaned);
    if (sentences.length > queuedSentenceCountRef.current) {
      const newOnes = sentences.slice(queuedSentenceCountRef.current);
      ttsQueueRef.current.push(...newOnes);
      queuedSentenceCountRef.current = sentences.length;
      processSpeechQueue();
    }
  }, [processSpeechQueue]);

  // Reset the streaming TTS state for a new turn
  const resetStreamingTts = useCallback(() => {
    ttsQueueRef.current = [];
    streamBufferRef.current = '';
    queuedSentenceCountRef.current = 0;
    activeStreamIdRef.current = null;
    streamCompleteRef.current = false;
  }, []);

  // Load referenceable components for the "/" palette.
  useEffect(() => {
    listWorkflows().then(setWorkflows).catch(() => {});
    listWebhooks().then(setWebhooks).catch(() => {});
    listConnectorInstances().then(setConnectors).catch(() => {});
    listGitRepos().then(setRepos).catch(() => {});
    listSkills().then(setSkillFiles).catch(() => {});
  }, []);

  // Native OS drag-and-drop of files onto the window. Tauri intercepts the
  // native drop (the webview never sees an HTML drop event), so we listen to
  // the webview's drag-drop stream and attach the dropped paths.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === 'enter' || p.type === 'over') {
          setDragActive(true);
        } else if (p.type === 'leave') {
          setDragActive(false);
        } else if (p.type === 'drop') {
          setDragActive(false);
          const paths = (p.paths ?? []).filter(Boolean);
          if (paths.length) {
            setAttachedFiles((prev) => Array.from(new Set([...prev, ...paths])));
            requestAnimationFrame(() => textareaRef.current?.focus());
          }
        }
      })
      .then((fn) => { unlisten = fn; })
      .catch(() => {});
    return () => { if (unlisten) unlisten(); };
  }, []);

  const removeAttachment = useCallback((path: string) => {
    setAttachedFiles((prev) => prev.filter((p) => p !== path));
  }, []);

  const baseName = (p: string) => p.split('/').pop() || p;
  const isImagePath = (p: string) => /\.(png|jpe?g|gif|webp|bmp|tiff?|heic|heif)$/i.test(p);
  const hasImageAttached = attachedFiles.some(isImagePath);

  // When an image is attached, check once whether the loaded model can see it,
  // so we can warn before send instead of after. Only fires while an image is
  // pending; clears otherwise.
  useEffect(() => {
    if (!hasImageAttached) { setVisionCapable(null); return; }
    let cancelled = false;
    getLocalLlmStatus()
      .then((s) => { if (!cancelled) setVisionCapable(!!s.vision); })
      .catch(() => { if (!cancelled) setVisionCapable(null); });
    return () => { cancelled = true; };
  }, [hasImageAttached]);

  // Prefill from parent (e.g. suggestion chips)
  useEffect(() => {
    if (prefill) {
      setValue(prefill);
      onPrefillConsumed?.();
      setTimeout(() => textareaRef.current?.focus(), 50);
    }
  }, [prefill, onPrefillConsumed]);

  // Prefill from anywhere (the How-to guide's "Try it" buttons dispatch
  // this). A "/" prefill also opens the slash palette naturally.
  useEffect(() => {
    const onPrefillEvent = (e: Event) => {
      const text = (e as CustomEvent<{ text: string }>).detail?.text;
      if (!text) return;
      setValue(text);
      requestAnimationFrame(() => textareaRef.current?.focus());
    };
    window.addEventListener('orbix:prefill', onPrefillEvent);
    return () => window.removeEventListener('orbix:prefill', onPrefillEvent);
  }, []);

  // Auto-resize textarea - grows up to 40vh then switches to scrollable
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    const maxH = Math.floor(window.innerHeight * 0.4);

    // Temporarily hide overflow so scrollHeight reports full content height
    el.style.overflow = 'hidden';
    el.style.height = '0px';
    const contentH = el.scrollHeight;
    const clampedH = Math.max(22, Math.min(contentH, maxH));
    el.style.height = clampedH + 'px';

    // Switch to scrollable only when content exceeds max
    if (contentH > maxH) {
      el.style.overflow = 'auto';
      el.scrollTop = el.scrollHeight;
    }
  }, [value]);

  // Slash palette: opens on a "/" at line start OR after whitespace, and
  // filters by the token typed after it. This lets the user reference a
  // component mid-sentence ("summarise /new-leads") to direct Bob.
  useEffect(() => {
    const m = value.match(/(^|\s)\/([^\s/]*)$/);
    if (m) {
      setShowSlash(true);
      setSlashFilter((m[2] || '').toLowerCase());
      setSlashStart(value.lastIndexOf('/'));
      setSlashIndex(0);
    } else {
      setShowSlash(false);
      setSlashStart(-1);
    }
  }, [value]);

  const getSlashName = (name: string) =>
    name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');

  const matchesFilter = (name: string) =>
    getSlashName(name).includes(slashFilter) || name.toLowerCase().includes(slashFilter);

  // Every referenceable component, grouped by kind. Selecting one inserts a
  // reference so the user can direct Bob at that exact component; agents
  // keep the scope-chip behaviour.
  type SlashItem =
    | { kind: 'workflow'; key: string; label: string; sub: string; workflow: Workflow }
    | { kind: 'webhook'; key: string; label: string; sub: string; webhook: WebhookEndpoint }
    | { kind: 'connector'; key: string; label: string; sub: string; connector: ConnectorInstance }
    | { kind: 'agent'; key: string; label: string; sub: string; agent: Agent }
    | { kind: 'repo'; key: string; label: string; sub: string; repo: RepoInfo }
    | { kind: 'skillfile'; key: string; label: string; sub: string; skill: SkillMeta };

  // /repo (or /repos) lists every project; a bare "/" shows the five most
  // recently active; any other token filters repos by name like the rest.
  const repoMatches: RepoInfo[] =
    slashFilter === 'repo' || slashFilter === 'repos' ? repos.slice(0, 20)
    : slashFilter === '' ? repos.slice(0, 5)
    : repos.filter(r => matchesFilter(r.name)).slice(0, 20);

  const slashItems: SlashItem[] = [
    ...repoMatches.map(r => ({
      kind: 'repo' as const, key: `rp-${r.path}`,
      label: r.name, sub: `${r.branch} · ${r.path}`, repo: r,
    })),
    ...skillFiles.filter(sk => matchesFilter(sk.name)).map(sk => ({
      kind: 'skillfile' as const, key: `sk-${sk.name}`,
      label: sk.name, sub: sk.description || 'skill', skill: sk,
    })),
    ...workflows.filter(w => matchesFilter(w.name)).map(w => ({
      kind: 'workflow' as const, key: `wf-${w.id}`,
      label: w.name, sub: `${w.steps.length} steps`, workflow: w,
    })),
    ...webhooks.filter(w => matchesFilter(w.name) || w.path.toLowerCase().includes(slashFilter)).map(w => ({
      kind: 'webhook' as const, key: `wh-${w.id}`,
      label: w.name, sub: w.path, webhook: w,
    })),
    ...connectors.filter(c => matchesFilter(c.name)).map(c => ({
      kind: 'connector' as const, key: `cn-${c.id}`,
      label: c.name, sub: String(c.status), connector: c,
    })),
    ...agents.filter(a => a.enabled).filter(a => matchesFilter(a.name)).map(a => ({
      kind: 'agent' as const, key: `ag-${a.id}`,
      label: a.name, sub: a.model, agent: a,
    })),
  ];

  const GROUP_LABEL: Record<SlashItem['kind'], string> = {
    repo: 'Repositories', skillfile: 'Skills', workflow: 'Workflows', webhook: 'Webhooks', connector: 'Connectors', agent: 'Agents',
  };

  // Replace the "/token" at slashStart with an inline reference Bob reads.
  const insertReference = useCallback((token: string) => {
    setValue(prev => {
      const cut = slashStart >= 0 ? slashStart : prev.length;
      return `${prev.slice(0, cut)}${token} `;
    });
    setShowSlash(false);
    setSlashFilter('');
    setSlashStart(-1);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [slashStart]);

  const handleSelectAgent = useCallback((agent: Agent) => {
    setScopedAgent(agent);
    setShowSlash(false);
    setSlashFilter('');
    setSlashStart(-1);
    setSlashIndex(0);
    // Drop the "/token" that opened the palette; the chip carries the scope.
    setValue(prev => (slashStart >= 0 ? prev.slice(0, slashStart) : ''));
    requestAnimationFrame(() => {
      textareaRef.current?.focus();
    });
  }, [slashStart]);

  // A repo scopes the conversation like an agent does: chip on, "/token"
  // dropped, every send carries the repo context.
  const handleSelectRepo = useCallback((repo: RepoInfo) => {
    setScopedRepo(repo);
    setShowSlash(false);
    setSlashFilter('');
    setSlashStart(-1);
    setSlashIndex(0);
    setValue(prev => (slashStart >= 0 ? prev.slice(0, slashStart) : ''));
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [slashStart]);

  // Route a palette selection: agents and repos scope the turn; every other
  // component is inserted as an inline reference so the user can direct Bob
  // at it.
  const activateSlashItem = useCallback((item: SlashItem) => {
    switch (item.kind) {
      case 'agent': handleSelectAgent(item.agent); break;
      case 'repo': handleSelectRepo(item.repo); break;
      case 'skillfile': insertReference(`[skill: ${item.skill.name}]`); break;
      case 'workflow': insertReference(`[workflow: ${item.workflow.name}]`); break;
      case 'webhook': insertReference(`[webhook: ${item.webhook.name}]`); break;
      case 'connector': insertReference(`[connector: ${item.connector.name}]`); break;
    }
  }, [handleSelectAgent, handleSelectRepo, insertReference]);

  const clearScope = useCallback(() => {
    setScopedAgent(null);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, []);

  const handleRunWorkflow = useCallback(async (wf: Workflow) => {
    setValue('');
    setShowSlash(false);
    setLoading(true);

    onMessageSent({
      id: crypto.randomUUID(),
      role: 'user',
      content: `/${getSlashName(wf.name)}`,
      timestamp: new Date().toISOString(),
    });

    try {
      // The engine posts the result in chat as Bob when the run ends.
      await runWorkflow(wf.id);
    } catch (err) {
      onMessageSent({
        id: crypto.randomUUID(),
        role: 'assistant',
        content: `Failed to run workflow: ${typeof err === 'string' ? err : 'Unknown error'}`,
        timestamp: new Date().toISOString(),
      });
    } finally {
      setLoading(false);
    }
  }, [onMessageSent]);

  const handleSend = useCallback(async () => {
    const text = value.trim();
    if ((!text && attachedFiles.length === 0) || loading) return;

    // Quick-run: a message that is exactly one workflow reference (or the
    // legacy bare /slug) runs that workflow instead of chatting.
    const loneRef = text.match(/^\[workflow:\s*(.+?)\]$/i);
    const bareSlug = text.startsWith('/') && !text.includes(' ') ? text.slice(1).toLowerCase() : null;
    if (loneRef || bareSlug) {
      const target = loneRef ? loneRef[1].trim().toLowerCase() : bareSlug!;
      const match = workflows.find(w =>
        w.name.toLowerCase() === target || getSlashName(w.name) === target
      );
      if (match) {
        handleRunWorkflow(match);
        return;
      }
    }

    // "imagine", alone or with a hint, hands the turn to Imagine: Bob
    // with free rein. The App-level overlay runs it. Never also call
    // bobSend here, or Bob runs a parallel chat turn behind the overlay.
    const imagineCmd = text.match(/^imagine\b[\s,:.!\-]*([\s\S]*)$/i);
    if (imagineCmd) {
      window.dispatchEvent(new CustomEvent('orbix:open-imagine', { detail: { seed: imagineCmd[1].trim() } }));
      setValue('');
      return;
    }

    // Scoping: if a chip is set, route to that agent. Otherwise still
    // honour the legacy "@slug rest…" text prefix.
    let messageText = text;
    let routedAgentId: string | undefined = scopedAgent?.id ?? activeAgent?.id;
    const mention = !scopedAgent && text.match(/^@([a-z0-9-]+)\s+([\s\S]+)$/i);
    if (mention) {
      const targetSlug = mention[1].toLowerCase();
      const targetAgent = agents.find(
        a => getSlashName(a.name) === targetSlug || a.name.toLowerCase() === targetSlug,
      );
      if (targetAgent) {
        routedAgentId = targetAgent.id;
        messageText = mention[2].trim();
      }
    }
    const isScoped = !!scopedAgent || !!mention;

    // Repo scope: hidden context block so Bob's tools target the project.
    // The visible bubble stays the user's words; the chip shows the scope.
    if (scopedRepo) {
      messageText = `${messageText}\n\nREPO CONTEXT: The user is working in the git repository "${scopedRepo.name}" at ${scopedRepo.path} (branch: ${scopedRepo.branch}). Run shell commands there by passing "cwd": "${scopedRepo.path}" to shell_exec, and use absolute paths under it for file tools. If asked to create a code agent for this repo, put the path in its charter and grant it the 'tool' toolbox.`;
    }

    // Attach any dropped files. Documents go to Bob as absolute paths with an
    // instruction to file_read them. Images can't be read as text - they go to
    // the vision model as a separate `images` argument so Bob can actually see
    // them. The visible bubble shows every file's name either way.
    const files = attachedFiles;
    const imageFiles = files.filter(isImagePath);
    const docFiles = files.filter((f) => !isImagePath(f));
    if (docFiles.length) {
      const paths = docFiles.map((f) => `- ${f}`).join('\n');
      messageText = `${messageText}${messageText ? '\n\n' : ''}The user attached these files. Read them with file_read before answering:\n${paths}`;
    }
    const displayContent = text
      ? (files.length ? `${text}\n\n📎 ${files.map(baseName).join(', ')}` : text)
      : `📎 ${files.map(baseName).join(', ')}`;

    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      content: displayContent,
      timestamp: new Date().toISOString(),
    };
    onMessageSent(userMsg);
    setValue('');
    setAttachedFiles([]);
    setLoading(true);

    try {
      // When the user has scoped to an agent (chip OR @-prefix), bypass Bob's
      // router and hit that agent directly so the conversation feels scoped.
      const reply = isScoped && routedAgentId
        ? await sendMessage(messageText, routedAgentId)
        : useBob
        ? await bobSend(messageText, imageFiles.length ? imageFiles : undefined)
        : await sendMessage(messageText, routedAgentId);
      onMessageSent(reply);
      // Refresh workflows in case the LLM created one
      listWorkflows().then(setWorkflows).catch(() => {});
    } catch (err: unknown) {
      const errMsg = typeof err === 'string' ? err
        : err instanceof Error ? err.message
        : 'Something went wrong.';
      onMessageSent({
        id: crypto.randomUUID(),
        role: 'assistant',
        content: errMsg,
        timestamp: new Date().toISOString(),
      });
    } finally {
      setLoading(false);
    }
  }, [value, loading, activeAgent, agents, onMessageSent, workflows, handleRunWorkflow, useBob, scopedAgent, scopedRepo, attachedFiles]);

  // ── Conversation Mode ──
  // Click mic → start continuous conversation with Bob
  // User speaks → VAD auto-stops → transcribes → sends to Bob → Bob speaks → mic reopens
  // Click mic again → exit conversation mode
  //
  // This is a natural conversation loop; the user never has to press stop.

  // Audio level visualization + BARGE-IN detection
  // During 'speaking' phase we monitor the mic; if the user starts talking
  // over Bob (sustained energy above threshold), we interrupt TTS instantly.
  useEffect(() => {
    const BARGE_IN_THRESHOLD = 0.15;   // higher than typical speaker self-echo
    const BARGE_IN_FRAMES = 4;         // ~130ms sustained
    const unsub = onAudioLevel((level) => {
      setAudioLevel(level);
      setMaxAudioLevel(prev => Math.max(prev * 0.995, level));

      // Barge-in: only active while Bob is speaking
      if (phaseRef.current === 'speaking' && !bargingInRef.current && conversationActiveRef.current) {
        if (level > BARGE_IN_THRESHOLD) {
          bargeInCounterRef.current += 1;
          if (bargeInCounterRef.current >= BARGE_IN_FRAMES) {
            bargingInRef.current = true;
            bargeInCounterRef.current = 0;
            handleBargeIn();
          }
        } else {
          bargeInCounterRef.current = Math.max(0, bargeInCounterRef.current - 1);
        }
      } else {
        bargeInCounterRef.current = 0;
      }
    });
    return () => { unsub.then(fn => fn()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Barge-in action: kill TTS + clear queue. The drain loop in the silence
  // handler will see the empty queue, exit, and its continuation tears down
  // the barge-in mic and calls startConversationTurn for a fresh turn.
  const handleBargeIn = useCallback(async () => {
    console.log('[voice] 🚨 BARGE-IN detected - interrupting Bob');
    try { await stopSpeaking(); } catch {}
    ttsQueueRef.current = [];
    streamBufferRef.current = '';
    queuedSentenceCountRef.current = 0;
    // bargingInRef is reset once the next turn begins listening
    setTimeout(() => { bargingInRef.current = false; }, 400);
  }, []);

  // Listen for streaming chunks from the LLM - push to TTS queue if we're in conversation mode
  useEffect(() => {
    const unsubChunk = onMessageChunk((data) => {
      if (!conversationActiveRef.current) return;
      // Only stream into TTS for the active turn
      if (activeStreamIdRef.current && data.message_id !== activeStreamIdRef.current) return;
      if (!activeStreamIdRef.current) activeStreamIdRef.current = data.message_id;
      pushStreamChunk(data.content);
    });
    const unsubEnd = onMessageStreamEnd((data) => {
      if (!conversationActiveRef.current) return;
      if (activeStreamIdRef.current && data.message_id !== activeStreamIdRef.current) return;
      // Final flush: any trailing text without sentence terminator gets spoken
      if (data.content) {
        const cleaned = cleanForSpeech(data.content);
        const { sentences, remainder } = extractSentences(cleaned);
        if (sentences.length > queuedSentenceCountRef.current) {
          const newOnes = sentences.slice(queuedSentenceCountRef.current);
          ttsQueueRef.current.push(...newOnes);
          queuedSentenceCountRef.current = sentences.length;
        }
        if (remainder.trim().length > 3) {
          ttsQueueRef.current.push(remainder.trim());
        }
        processSpeechQueue();
      }
      streamCompleteRef.current = true;
    });
    return () => {
      unsubChunk.then(fn => fn());
      unsubEnd.then(fn => fn());
    };
  }, [pushStreamChunk, processSpeechQueue]);

  const startConversationTurn = useCallback(async () => {
    if (!conversationActiveRef.current) return;
    if (processingRef.current) return; // Already handling something
    setMaxAudioLevel(0); // Reset peak tracker for this turn
    try {
      await startListening();
      setIsRecording(true);
      setConversationPhase('listening');
    } catch (err) {
      console.error('[voice] Failed to start listening:', err);
      const errMsg = typeof err === 'string' ? err
        : err instanceof Error ? err.message : 'Failed to access microphone';
      onMessageSent({
        id: crypto.randomUUID(),
        role: 'assistant',
        content: `**Voice error:** ${errMsg}\n\nCheck that microphone access is granted in System Settings → Privacy & Security → Microphone.`,
        timestamp: new Date().toISOString(),
      });
      setConversationActive(false);
      setIsRecording(false);
      setConversationPhase('idle');
      processingRef.current = false;
    }
  }, [onMessageSent]);

  const exitConversation = useCallback(async () => {
    setConversationActive(false);
    conversationActiveRef.current = false;
    setIsRecording(false);
    setConversationPhase('idle');
    processingRef.current = false;
    // Kill any in-flight TTS so Bob stops talking immediately
    resetStreamingTts();
    try { await stopListening(); } catch {}
    try { await stopSpeaking(); } catch {}
  }, [resetStreamingTts]);

  // Auto-stop when VAD detects silence, send to Bob, speak reply, loop
  useEffect(() => {
    const unsubSilence = onSilenceDetected(async () => {
      // Guard: only handle if we're actively recording and not already processing
      if (!conversationActiveRef.current || !isRecordingRef.current) return;
      if (processingRef.current) return;
      // Barge-in protection: ignore silence events during the 'speaking' phase.
      // During speaking the mic is hot for barge-in detection, but the VAD will
      // fire on every pause between Bob's sentences - we must not treat those
      // as end-of-user-turn.
      if (phaseRef.current === 'speaking' || phaseRef.current === 'thinking') return;
      processingRef.current = true;

      // Stop recording and get transcription
      setIsRecording(false);
      setConversationPhase('transcribing');

      // ── Pre-warm the LLM in parallel with whisper transcription ──
      // By the time the user's text reaches Bob, the model is loaded and
      // the HTTP keep-alive is open. Saves ~300-800ms of cold-start latency.
      // Fire-and-forget - don't block the turn if it fails.
      prewarmLlm().catch(() => {});

      let transcribed = '';
      let sttError: string | null = null;
      try {
        transcribed = await stopListening();
      } catch (err) {
        sttError = typeof err === 'string' ? err
          : err instanceof Error ? err.message : 'Transcription failed';
        console.error('[voice] STT failed:', sttError);
      }

      if (sttError || !transcribed || !transcribed.trim()) {
        // Surface the issue visibly - never silently restart in a dead loop
        const message = sttError
          ? `I couldn't transcribe that. (${sttError}) - try again`
          : "I didn't hear anything. Try again?";
        onMessageSent({
          id: crypto.randomUUID(),
          role: 'assistant',
          content: message,
          timestamp: new Date().toISOString(),
        });
        processingRef.current = false;
        // Give user visual feedback, then restart listening
        if (conversationActiveRef.current) {
          setTimeout(() => startConversationTurn(), 800);
        }
        return;
      }

      // Add the user's message to the chat IMMEDIATELY so they see it
      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: 'user',
        content: transcribed.trim(),
        timestamp: new Date().toISOString(),
      };
      onMessageSent(userMsg);
      setConversationPhase('thinking');
      setLoading(true);

      // ── Streaming TTS pipeline ──
      // Reset for new turn, then fire LLM call. The chunk listener will
      // push sentences into the TTS queue as they arrive - Bob speaks
      // the first sentence while the rest is still being generated.
      resetStreamingTts();
      bargingInRef.current = false;
      bargeInCounterRef.current = 0;
      setConversationPhase('speaking');

      // Speak an ack only if the model is actually making us wait - a
      // filler phrase before an instant answer feels worse than a short
      // pause. After ~900ms of silence, say something varied.
      setTimeout(() => {
        if (!conversationActiveRef.current) return;
        if (streamBufferRef.current.trim() || queuedSentenceCountRef.current > 0) return;
        ttsQueueRef.current.push(pickAck());
        processSpeechQueue();
      }, 900);

      // BARGE-IN MIC DISABLED.
      // Previously we started a mic stream here so we could detect when the
      // user talks over Bob and interrupt TTS. Problem: on Bluetooth (AirPods),
      // opening the mic forces macOS to switch from A2C (high-quality output,
      // no mic) to HFP (phone-quality bidirectional). That resamples Kokoro's
      // 24kHz output to 8kHz and makes Bob sound terrible.
      //
      // Trade-off: no barge-in, but Bob sounds good. The user can still tap
      // the mic button to stop Bob and start talking. When we find a way to
      // detect barge-in without opening cpal's input stream (e.g. CoreAudio
      // tap or a separate non-Bluetooth mic check), we can re-enable this.

      let replyContent = '';
      let streamingHappened = false;
      try {
        // Voice conversation mode → Bob Lite (fast E4B, port 8848).
        // Lite responds in <400ms with a brief acknowledgment, then
        // escalates to Brain via <think> tags as needed. Brain's result
        // streams back on the same message_id so TTS keeps speaking.
        // Typed mode (or Bob disabled) still uses the heavy Brain path.
        const reply = useBob
          ? await bobSend(transcribed.trim())
          : await sendMessage(transcribed.trim(), activeAgent?.id);
        onMessageSent(reply);
        replyContent = reply.content || '';
        streamingHappened = queuedSentenceCountRef.current > 0;
      } catch (err: unknown) {
        const errMsg = typeof err === 'string' ? err
          : err instanceof Error ? err.message : 'Something went wrong.';
        onMessageSent({
          id: crypto.randomUUID(),
          role: 'assistant',
          content: errMsg,
          timestamp: new Date().toISOString(),
        });
      } finally {
        setLoading(false);
      }

      // Fallback: if streaming didn't fire (cloud agent, no streaming events),
      // speak the full reply now using the truncation strategy.
      if (!streamingHappened && conversationActiveRef.current && replyContent) {
        const cleaned = cleanForSpeech(replyContent);
        const sentences = cleaned.match(/[^.!?]+[.!?]+/g) || [cleaned];
        let spoken = sentences.slice(0, 2).join(' ').trim();
        if (spoken.length > 240) spoken = spoken.slice(0, 240).trimEnd() + '.';
        if (!spoken) spoken = cleaned.slice(0, 240);
        try { await speakText(spoken); } catch (e) { console.error('[voice] TTS failed:', e); }
      }

      // Wait for the streaming TTS queue to drain (sentences still being spoken)
      // - or until barge-in fires (which clears the queue, exiting this loop fast).
      while (ttsQueueRef.current.length > 0 || ttsProcessingRef.current) {
        if (!conversationActiveRef.current) break;
        await new Promise(r => setTimeout(r, 100));
      }

      // Barge-in mic is now disabled (see comment above), so no teardown needed.

      processingRef.current = false;

      // Restart listening fresh for the next turn
      if (conversationActiveRef.current) {
        setTimeout(() => startConversationTurn(), 200);
      }
    });
    return () => { unsubSilence.then(fn => fn()); };
  }, [onMessageSent, useBob, activeAgent, startConversationTurn, pickAck, processSpeechQueue]);

  // Mic button toggle - enter or exit conversation mode
  const handleMicToggle = useCallback(async () => {
    if (conversationActiveRef.current) {
      // Exit conversation mode
      voiceConversationSet(false).catch(() => {});
      await exitConversation();
      return;
    }
    // Enter conversation mode
    setConversationActive(true);
    conversationActiveRef.current = true;
    // Lock the model warm during voice - eviction loop respects this flag
    voiceConversationSet(true).catch(() => {});
    // Pre-warm the LLM now so the first turn doesn't pay cold-load cost
    prewarmLlm().catch(() => {});
    await startConversationTurn();
  }, [exitConversation, startConversationTurn]);

  const handleKey = (e: React.KeyboardEvent) => {
    // Backspace on an empty textarea while scoped → drop the chips first,
    // most recently added kind first (repo, then agent).
    if (e.key === 'Backspace' && !value && scopedRepo) {
      e.preventDefault();
      setScopedRepo(null);
      return;
    }
    if (e.key === 'Backspace' && !value && scopedAgent) {
      e.preventDefault();
      clearScope();
      return;
    }
    if (showSlash && slashItems.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSlashIndex(i => Math.min(i + 1, slashItems.length - 1));
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSlashIndex(i => Math.max(i - 1, 0));
        return;
      }
      if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
        e.preventDefault();
        activateSlashItem(slashItems[slashIndex]);
        return;
      }
      if (e.key === 'Escape') {
        setShowSlash(false);
        return;
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const isDisabled = loading || orbState === 'executing';

  return (
    <div className="chat-input-wrap">
      {/* Drag-and-drop overlay - shown while files hover over the window */}
      {dragActive && (
        <div className="chat-drop-overlay">
          <div className="chat-drop-card">
            <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="7 10 12 15 17 10" />
              <line x1="12" y1="15" x2="12" y2="3" />
            </svg>
            <span>Drop files to attach</span>
          </div>
        </div>
      )}

      {/* "/" palette - reference any component to direct Bob at it */}
      {showSlash && slashItems.length > 0 && (
        <div className="slash-menu">
          {slashItems.map((item, i) => {
            const isGroupStart = i === 0 || slashItems[i - 1].kind !== item.kind;
            const prefix = item.kind === 'agent' ? '@' : item.kind === 'workflow' || item.kind === 'repo' || item.kind === 'skillfile' ? '/' : '#';
            return (
              <div key={item.key}>
                {isGroupStart && <div className="slash-menu-header">{GROUP_LABEL[item.kind]}</div>}
                <button
                  className={`slash-item ${i === slashIndex ? 'active' : ''}`}
                  onClick={() => activateSlashItem(item)}
                  onMouseEnter={() => setSlashIndex(i)}
                >
                  <div className="slash-item-icon">
                    {item.kind === 'repo' && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="6" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="12" r="3" /><path d="M6 9v6" /><path d="M9 6.7c4 0 6 1.8 6 5.3" /></svg>
                    )}
                    {item.kind === 'skillfile' && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 6c-2-1.5-4.5-2-8-2v14c3.5 0 6 .5 8 2 2-1.5 4.5-2 8-2V4c-3.5 0-6 .5-8 2z" /><path d="M12 6v14" /></svg>
                    )}
                    {item.kind === 'workflow' && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polygon points="5 3 19 12 5 21 5 3" /></svg>
                    )}
                    {item.kind === 'webhook' && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M18 8a6 6 0 0 0-9.33-5" /><path d="M10.5 20a6 6 0 0 1-6-6" /><circle cx="18" cy="16" r="3" /><circle cx="6" cy="8" r="3" /></svg>
                    )}
                    {item.kind === 'connector' && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M9 7H6a3 3 0 0 0 0 6h3" /><path d="M15 17h3a3 3 0 0 0 0-6h-3" /><line x1="8" y1="12" x2="16" y2="12" /></svg>
                    )}
                    {item.kind === 'agent' && (
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="8" r="4" /><path d="M4 21v-1a8 8 0 0 1 16 0v1" /></svg>
                    )}
                  </div>
                  <div className="slash-item-info">
                    <span className="slash-item-name">{prefix}{getSlashName(item.label)}</span>
                    <span className="slash-item-desc">{item.label} - {item.sub}</span>
                  </div>
                </button>
              </div>
            );
          })}
        </div>
      )}

      <div className={`chat-input-bar ${isDisabled ? 'disabled' : ''} ${value.trim() ? 'has-text' : ''}`}>
        {(scopedAgent || scopedRepo) && (
          <div className="chat-scope-row">
            {scopedAgent && (
              <span className="chat-scope-chip" title="Scoped to this agent - Backspace to dismiss">
                <span className="chat-scope-dot" />
                <span className="chat-scope-prefix">@</span>
                <span className="chat-scope-name">{scopedAgent.name}</span>
                <button
                  type="button"
                  className="chat-scope-close"
                  onClick={clearScope}
                  aria-label="Remove agent scope"
                >
                  ×
                </button>
              </span>
            )}
            {scopedRepo && (
              <span className="chat-scope-chip repo" title={`${scopedRepo.path} - Backspace to dismiss`}>
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="6" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="12" r="3" /><path d="M6 9v6" /><path d="M9 6.7c4 0 6 1.8 6 5.3" /></svg>
                <span className="chat-scope-name">{scopedRepo.name}</span>
                <span className="chat-scope-branch">{scopedRepo.branch}</span>
                <button
                  type="button"
                  className="chat-scope-close"
                  onClick={() => setScopedRepo(null)}
                  aria-label="Remove repo scope"
                >
                  ×
                </button>
              </span>
            )}
          </div>
        )}
        {attachedFiles.length > 0 && (
          <div className="chat-attach-row">
            {attachedFiles.map((path) => (
              <span key={path} className="chat-attach-chip" title={path}>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
                </svg>
                <span className="chat-attach-name">{baseName(path)}</span>
                <button
                  type="button"
                  className="chat-attach-close"
                  onClick={() => removeAttachment(path)}
                  aria-label={`Remove ${baseName(path)}`}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        {hasImageAttached && visionCapable === false && (
          <div className="chat-vision-hint" role="status">
            The current model can't see images yet. Every Gemma 4 and Qwen 3 model in
            Settings › Models can once its image projector is downloaded.
          </div>
        )}
        <textarea
          ref={textareaRef}
          className="chat-input"
          placeholder={
            conversationActive
              ? (conversationPhase === 'listening' ? 'Listening... speak naturally'
                : conversationPhase === 'transcribing' ? 'Transcribing...'
                : conversationPhase === 'thinking' ? 'Bob is thinking...'
                : conversationPhase === 'speaking' ? 'Bob is speaking...'
                : 'Bob is responding...')
              : isDisabled ? 'Bob is thinking...'
              : scopedAgent ? `Message ${scopedAgent.name} directly…`
              : scopedRepo ? `Message Bob about ${scopedRepo.name} - e.g. "hire a code agent for this repo"…`
              : 'Message Bob'
          }
          value={value}
          onChange={e => setValue(e.target.value)}
          onKeyDown={handleKey}
          disabled={isDisabled}
          rows={1}
          autoFocus
        />
        <div className="chat-input-footer">
          <div className="input-hints">
            {conversationActive ? (
              <div className="voice-live-indicator">
                <div className="voice-level-bar">
                  <div
                    className={`voice-level-fill ${audioLevel > 0.05 ? 'active' : ''}`}
                    style={{ width: `${Math.min(100, audioLevel * 400)}%` }}
                  />
                </div>
                <span className="voice-phase-label">
                  {conversationPhase === 'listening' && (maxAudioLevel > 0.03 ? 'speech detected' : 'listening for speech...')}
                  {conversationPhase === 'transcribing' && 'transcribing'}
                  {conversationPhase === 'thinking' && 'bob is thinking'}
                  {conversationPhase === 'speaking' && 'bob is speaking'}
                </span>
              </div>
            ) : (
              <>
                <ModelSelector compact />
                
              </>
            )}
          </div>
          <div className="input-actions">
            {/* Imagine - Bob with free rein; the drafted text becomes his hint */}
            <button
              className="imagine-launch-btn"
              onClick={() => {
                window.dispatchEvent(new CustomEvent('orbix:open-imagine', { detail: { seed: value.trim() } }));
                setValue('');
              }}
              title="Imagine: let Bob look around and make something"
              aria-label="Imagine: let Bob look around and make something"
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                <path d="M12 3l1.9 4.6L18.5 9.5l-4.6 1.9L12 16l-1.9-4.6L5.5 9.5l4.6-1.9z"/>
                <path d="M19 15l.9 2.1 2.1.9-2.1.9L19 21l-.9-2.1-2.1-.9 2.1-.9z"/>
              </svg>
            </button>
            {/* Mic button - toggles conversation mode */}
            <button
              className={`mic-btn ${isRecording ? 'recording' : ''} ${conversationActive ? 'conversation-active' : ''}`}
              onClick={handleMicToggle}
              title={conversationActive ? 'End conversation' : 'Start voice conversation'}
            >
              {conversationActive ? (
                /* Stop icon when in conversation mode */
                <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                  <rect x="6" y="6" width="12" height="12" rx="2" />
                </svg>
              ) : (
                /* Mic icon */
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M12 2a3 3 0 0 1 3 3v7a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3z"/>
                  <path d="M19 10v2a7 7 0 0 1-14 0v-2"/>
                  <line x1="12" y1="19" x2="12" y2="22"/>
                  <line x1="8" y1="22" x2="16" y2="22"/>
                </svg>
              )}
              {isRecording && <span className="mic-pulse" />}
            </button>

            {/* Send button */}
            <button
              className={`send-btn ${(value.trim() || attachedFiles.length > 0) && !isDisabled ? 'active' : ''}`}
              onClick={handleSend}
              disabled={(!value.trim() && attachedFiles.length === 0) || isDisabled}
              title="Send" aria-label="Send"
            >
              {loading ? (
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="spin">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                </svg>
              ) : (
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M5 12h14M12 5l7 7-7 7"/>
                </svg>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
