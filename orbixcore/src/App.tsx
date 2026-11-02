import { useState, useEffect, useRef, useCallback, lazy, Suspense} from 'react';
import type { TabId, PanelType, OrbState, Message, Agent, McpServer, VoiceSettings, ToolCall, Workflow } from './types';
import {
  onOrbStateChange, onNewMessage, getOrbState, getConversation,
  getVoiceSettings, speakText,
  onToolCallPending, getPendingToolCalls,
  onAudioLevel,
  onTtsAmplitude,
  listAgents,
  getSelectedLocalModel,
  runWorkflow,
  getOnboarding,
  onOpenConnectorSetup,
  onDataChanged,
  onMenuAction,
  onBobToast,
  onLocalLlmStatus,
  onBobReview,
  onMessageStreamStart, onMessageChunk, onMessageStreamEnd,
  requestVoicePermissions,
  saveCurrentConversation,
  bobCancel,
  bobSend,
  type BobReviewPayload,
  openRunMonitor,
  onPairEvent,
  peerFollowUp,
} from './hooks/api';
import TitleBar from './components/TitleBar';
import BobReviewModal from './components/BobReviewModal';
import CommandPalette from './components/CommandPalette';
import ContextSidebar from './components/ContextSidebar';
import ConversationsRail from './components/ConversationsRail';
import RunView from './components/RunView';
// Lazy: BuildView (and its workflow/agent editors) is only reached via the
// Build tab, so it should not sit in the first-paint bundle.
const BuildView = lazy(() => import('./components/BuildView'));
const AgentsPanel = lazy(() => import('./components/AgentsPanel'));
const McpPanel = lazy(() => import('./components/McpPanel'));
const VoicePanel = lazy(() => import('./components/VoicePanel'));
const MemoryPanel = lazy(() => import('./components/MemoryPanel'));
const ToolsPanel = lazy(() => import('./components/ToolsPanel'));
const BrowserPanel = lazy(() => import('./components/BrowserPanel'));
// ScreenPanel removed - replaced by WindowIntelligencePanel
const SettingsPanel = lazy(() => import('./components/SettingsPanel'));
const GuidePanel = lazy(() => import('./components/GuidePanel'));
const ConnectorsPanel = lazy(() => import('./components/ConnectorsPanel'));
const TriggersPanel = lazy(() => import('./components/TriggersPanel'));
const InboxPanel = lazy(() => import('./components/InboxPanel'));
const SchedulerPanel = lazy(() => import('./components/SchedulerPanel'));
const OrchestratorPanel = lazy(() => import('./components/OrchestratorPanel'));
const SuggestionsPanel = lazy(() => import('./components/SuggestionsPanel'));
const DashboardPanel = lazy(() => import('./components/DashboardPanel'));
const KnowledgePanel = lazy(() => import('./components/KnowledgePanel'));
const NotificationsPanel = lazy(() => import('./components/NotificationsPanel'));
const SearchPanel = lazy(() => import('./components/SearchPanel'));
const PluginsPanel = lazy(() => import('./components/PluginsPanel'));
const MonitorPanel = lazy(() => import('./components/MonitorPanel'));
const AuditPanel = lazy(() => import('./components/AuditPanel'));
const IncidentsPanel = lazy(() => import('./components/IncidentsPanel'));
const ApprovalQueuePanel = lazy(() => import('./components/ApprovalQueuePanel'));
const WindowIntelligencePanel = lazy(() => import('./components/WindowIntelligencePanel'));
const DataControlPanel = lazy(() => import('./components/DataControlPanel'));
const MyUiPanel = lazy(() => import('./components/MyUiPanel'));
const DocumentsPanel = lazy(() => import('./components/DocumentsPanel'));
import DevDrawer from './components/DevDrawer';
import ContextMenu, { useClipboardCommands, type ContextMenuEntry } from './components/ContextMenu';
import BobToast, { type BobToastItem } from './components/BobToast';
import PairMode from './components/PairMode';
import Imagine from './components/Imagine';
import ErrorBoundary from './components/ErrorBoundary';
const CodeFindingsPanel = lazy(() => import('./components/CodeFindingsPanel'));
import VoiceMode from './components/VoiceMode';
import Onboarding from './components/Onboarding';
import { checkForUpdate, installUpdate } from './updater';
import './styles/app.css';

export default function App() {
  const [activeTab, setActiveTab] = useState<TabId>('run');
  const [activePanel, setActivePanel] = useState<PanelType>(null);
  // Back-stack of panels so drilling from Settings into a sub-panel
  // (Agents, Connectors, ...) returns to Settings on Back instead of
  // closing the whole overlay. Kept in a ref too so the open/close
  // callbacks stay stable and read the live stack without stale closures.
  const [panelStack, setPanelStack] = useState<PanelType[]>([]);
  const panelStackRef = useRef<PanelType[]>([]);
  const activePanelRef = useRef<PanelType>(null);
  useEffect(() => { panelStackRef.current = panelStack; }, [panelStack]);
  useEffect(() => { activePanelRef.current = activePanel; }, [activePanel]);
  const [orbState, setOrbState] = useState<OrbState>('idle');
  const [messages, setMessages] = useState<Message[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [, setHasLocalModel] = useState(false);
  const [voiceSettings, setVoiceSettings] = useState<VoiceSettings>({
    tts_voice: 'Samantha',
    tts_rate: 175,
    auto_speak: false,
    stt_language: 'en-US',
  });
  // The new-message listener is registered once ([] deps) and would otherwise
  // capture the initial voiceSettings forever - so auto_speak read there was
  // always the false default and speech never fired. Mirror the latest
  // settings into a ref the listener can read live.
  const voiceSettingsRef = useRef(voiceSettings);
  useEffect(() => { voiceSettingsRef.current = voiceSettings; }, [voiceSettings]);
  const [, setPendingToolCalls] = useState<ToolCall[]>([]);
  const [cmdPaletteOpen, setCmdPaletteOpen] = useState(false);
  const [voiceModeOpen, setVoiceModeOpen] = useState(false);
  const [audioLevel, setAudioLevel] = useState(0);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [bobReview, setBobReview] = useState<BobReviewPayload | null>(null);
  // ── Imagine (Bob with free rein) ──
  const [imagineOpen, setImagineOpen] = useState(false);
  const [imagineSeed, setImagineSeed] = useState('');
  // ── Phase 1 shell state ──
  const [activeConversationId, setActiveConversationId] = useState<string | null>(
    () => localStorage.getItem('orbix-active-conversation') || null,
  );
  const [railCollapsed, setRailCollapsed] = useState<boolean>(
    // Collapsed is the default: the calm surface starts with nothing open.
    () => localStorage.getItem('orbix-rail-collapsed') !== '0',
  );
  const [toasts, setToasts] = useState<BobToastItem[]>([]);
  // Which workflow Build should show when something outside it asks.
  const [buildFocusId, setBuildFocusId] = useState<string | null>(null);
  // Sharing mode: the window shows its ports; drag one onto another app.
  // Drag-to-pair needs the native overlay, which exists on macOS only.
  const [sharingMode, setSharingMode] = useState(false);
  const sharingAvailable = document.documentElement.dataset.platform === 'mac';
  const [devDrawerOpen, setDevDrawerOpen] = useState<boolean>(
    () => localStorage.getItem('orbix-devdrawer-open') === '1',
  );
  const [devDrawerHeight, setDevDrawerHeight] = useState<number>(
    () => parseInt(localStorage.getItem('orbix-devdrawer-height') || '260', 10),
  );

  const pushToast = useCallback(
    (
      message: string,
      type: BobToastItem['type'] = 'action',
      onClick?: () => void,
    ) => {
      setToasts((prev) => [
        ...prev,
        { id: crypto.randomUUID(), message, type, timestamp: Date.now(), onClick },
      ]);
    },
    [],
  );

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  // Catch what slips past the app's many .catch() sites: genuinely unhandled
  // promise rejections and uncaught exceptions. Log them (the webview console
  // is capturable) and show one toast, deduped for 8s so a repeating failure
  // can't spam the corner. Resource load errors (e.error is null) are ignored.
  useEffect(() => {
    let lastMsg = '';
    let lastAt = 0;
    const surface = (label: string, detail: unknown) => {
      const msg = detail instanceof Error ? detail.message : String(detail ?? 'Unknown error');
      console.error('[' + label + ']', detail);
      const now = Date.now();
      if (msg === lastMsg && now - lastAt < 8000) return;
      lastMsg = msg;
      lastAt = now;
      const short = msg.length > 80 ? msg.slice(0, 80) + '…' : msg;
      pushToast('Something went wrong. ' + short, 'info');
    };
    const onRej = (e: PromiseRejectionEvent) => surface('unhandledrejection', e.reason);
    const onErr = (e: ErrorEvent) => { if (e.error) surface('error', e.error); };
    window.addEventListener('unhandledrejection', onRej);
    window.addEventListener('error', onErr);
    return () => {
      window.removeEventListener('unhandledrejection', onRej);
      window.removeEventListener('error', onErr);
    };
  }, [pushToast]);

  // Check for an app update shortly after launch. If one is waiting, offer it
  // as a click-to-install toast; the actual download/relaunch is user-driven so
  // we never interrupt. Silent when up to date, offline, or not yet configured.
  useEffect(() => {
    let cancelled = false;
    const install = async (info: import('./updater').UpdateInfo) => {
      pushToast(`Downloading update ${info.version}…`, 'info');
      try {
        await installUpdate(info, (phase, pct) => {
          if (phase === 'installing') pushToast('Installing update, the app will restart…', 'info');
          else if (phase === 'downloading' && pct === 100) pushToast('Update downloaded, installing…', 'info');
        });
      } catch (e) {
        console.error('[updater] install failed', e);
        pushToast('Update failed. Please try again later.', 'info');
      }
    };
    // Small delay so the update check never competes with first-paint work.
    const t = setTimeout(() => {
      checkForUpdate().then((info) => {
        if (cancelled || !info) return;
        pushToast(`Update ${info.version} available - click to install`, 'action', () => install(info));
      });
    }, 4000);
    return () => { cancelled = true; clearTimeout(t); };
  }, [pushToast]);

  // Pre-request voice permissions once, on first launch.
  useEffect(() => {
    const alreadyRequested = localStorage.getItem('orbix-voice-perms-requested');
    if (alreadyRequested) return;
    requestVoicePermissions()
      .then((result) => {
        console.log('[voice] Permission state:', result);
        localStorage.setItem('orbix-voice-perms-requested', '1');
      })
      .catch((err) => {
        console.warn('[voice] Permission probe failed:', err);
      });
  }, []);

  // Load saved theme preference
  useEffect(() => {
    const saved = localStorage.getItem('orbixcore-theme');
    if (saved) {
      document.documentElement.setAttribute('data-theme', saved);
    }
  }, []);

  // Check onboarding + load role profile
  useEffect(() => {
    getOnboarding().then(onb => {
      if (!onb.completed) setShowOnboarding(true);
    }).catch(() => setShowOnboarding(true));
  }, []);

  // Persist current in-memory conversation when the window unloads, so
  // the rail picks it up on next launch.
  useEffect(() => {
    const handler = () => {
      saveCurrentConversation().catch(() => {});
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, []);


  // Audio level listener - mic amplitude OR TTS amplitude, whichever is active.
  // Heavily throttled: only flushes to React state at 15fps (67ms) to avoid
  // a re-render storm that was pinning the main thread / causing beachball.
  // The actual values are cached in refs so nothing is lost between flushes.
  useEffect(() => {
    let ttsActive = false;
    let ttsDecayTimer: number | null = null;
    let pendingLevel = 0;
    let lastFlush = 0;
    let flushRaf: number | null = null;
    const FLUSH_INTERVAL = 67; // ~15fps

    const schedule = () => {
      if (flushRaf != null) return;
      flushRaf = requestAnimationFrame(() => {
        flushRaf = null;
        const now = performance.now();
        if (now - lastFlush < FLUSH_INTERVAL) {
          // Too soon - reschedule for next frame
          schedule();
          return;
        }
        lastFlush = now;
        setAudioLevel(pendingLevel);
      });
    };

    const unsubMic = onAudioLevel((level) => {
      if (ttsActive) return;
      pendingLevel = level;
      schedule();
    });
    const unsubTts = onTtsAmplitude((level) => {
      ttsActive = level > 0.001;
      pendingLevel = level;
      schedule();
      if (ttsDecayTimer) window.clearTimeout(ttsDecayTimer);
      ttsDecayTimer = window.setTimeout(() => { ttsActive = false; }, 200);
    });
    return () => {
      unsubMic.then(fn => fn());
      unsubTts.then(fn => fn());
      if (ttsDecayTimer) window.clearTimeout(ttsDecayTimer);
      if (flushRaf != null) cancelAnimationFrame(flushRaf);
    };
  }, []);

  // Close panel when switching tabs
  const handleTabChange = useCallback((tab: TabId) => {
    setActiveTab(tab);
    setActivePanel(null);
  }, []);

  // Persist rail collapsed state
  useEffect(() => {
    localStorage.setItem('orbix-rail-collapsed', railCollapsed ? '1' : '0');
  }, [railCollapsed]);

  useEffect(() => {
    localStorage.setItem('orbix-devdrawer-open', devDrawerOpen ? '1' : '0');
  }, [devDrawerOpen]);

  useEffect(() => {
    localStorage.setItem('orbix-devdrawer-height', String(devDrawerHeight));
  }, [devDrawerHeight]);

  useEffect(() => {
    if (activeConversationId) {
      localStorage.setItem('orbix-active-conversation', activeConversationId);
    } else {
      localStorage.removeItem('orbix-active-conversation');
    }
  }, [activeConversationId]);

  // Conversation rail handlers
  const handleConversationSelect = useCallback((id: string | null, msgs: Message[]) => {
    setActiveConversationId(id);
    setMessages(msgs);
    setPendingToolCalls([]);
    if (activeTab !== 'run') setActiveTab('run');
  }, [activeTab]);

  const handleNewConversation = useCallback(() => {
    setActiveConversationId(null);
    setMessages([]);
    setPendingToolCalls([]);
    if (activeTab !== 'run') setActiveTab('run');
  }, [activeTab]);

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;

      if (meta && e.key === 'k') { e.preventDefault(); setCmdPaletteOpen(prev => !prev); return; }
      if (meta && e.key === 'n') { e.preventDefault(); setMessages([]); setPendingToolCalls([]); return; }
      if (meta && e.key === 'j') { e.preventDefault(); setDevDrawerOpen(prev => !prev); return; }
      if (meta && e.key === '/') {
        e.preventDefault();
        if (activeTab !== 'run') setActiveTab('run');
        const input = document.querySelector('.chat-input textarea, .chat-input input') as HTMLElement;
        input?.focus();
        return;
      }
      if (meta && e.shiftKey && e.key === 'V') { e.preventDefault(); setVoiceModeOpen(prev => !prev); return; }
      if (meta && e.key === 'f') { e.preventDefault(); setActivePanel(prev => prev === 'search' ? null : 'search'); return; }
      if (e.key === 'Escape') {
        if (cmdPaletteOpen) { setCmdPaletteOpen(false); return; }
        if (voiceModeOpen) { setVoiceModeOpen(false); return; }
        if (activePanel) { closePanel(); return; }
      }
      // Tab switching: Cmd+1 (Chat) / Cmd+2 (Build)
      if (meta && !e.shiftKey && (e.key === '1' || e.key === '2')) {
        e.preventDefault();
        handleTabChange(e.key === '1' ? 'run' : 'build');
        return;
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [activePanel, activeTab, cmdPaletteOpen, voiceModeOpen, handleTabChange]);

  // Run a workflow and show results in chat
  const handleRunWorkflow = useCallback(async (wf: Workflow) => {
    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      content: `Run workflow: ${wf.name}`,
      timestamp: new Date().toISOString(),
    };
    setMessages(prev => [...prev, userMsg]);

    // The engine posts the result (or the failure) in chat as Bob when the
    // run ends, the same for every way a run starts; nothing to render here.
    try {
      await runWorkflow(wf.id);
    } catch (err) {
      setMessages(prev => [...prev, {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: `**${wf.name}** could not start: ${typeof err === 'string' ? err : 'Unknown error'}`,
        timestamp: new Date().toISOString(),
      }]);
    }
  }, []);

  useEffect(() => {
    getOrbState().then(setOrbState).catch(() => {});
    getVoiceSettings().then(setVoiceSettings).catch(() => {});
    getPendingToolCalls().then(setPendingToolCalls).catch(() => {});
    listAgents().then(setAgents).catch(() => {});
    getSelectedLocalModel().then(id => setHasLocalModel(!!id)).catch(() => {});

    const unsubOrb = onOrbStateChange(setOrbState);
    const unsubMsg = onNewMessage((msg) => {
      setMessages(prev => {
        // Deduplicate: skip if a message with this ID already exists
        if (prev.some(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      if (msg.role === 'assistant' && voiceSettingsRef.current.auto_speak) {
        speakText(msg.content).catch(() => {});
      }
    });
    const unsubTool = onToolCallPending((call) => {
      setPendingToolCalls(prev => [...prev, call]);
    });
    const unsubConnector = onOpenConnectorSetup(() => {
      setActiveTab('build');  // connectors live in Build tab
      setActivePanel('connectors');
    });
    // Native menu bar: File/View/Help items land here as one action string.
    // 'new-chat' is handled by ConversationsRail (it owns the thread reset).
    const unsubMenu = onMenuAction((action) => {
      const panelFor: Record<string, PanelType> = {
        'settings': 'settings', 'guide': 'guide', 'agents': 'agents',
        'dashboard': 'dashboard', 'my-ui': 'my-ui', 'audit': 'audit',
        'documents': 'documents',
      };
      const panel = panelFor[action];
      if (panel) {
        setPanelStack([]);
        setActivePanel(prev => (prev === panel ? null : panel));
      }
    });
    const unsubData = onDataChanged((reason) => {
      // Refresh agents when data changes (e.g. orbixdemo seeding)
      listAgents().then(setAgents).catch(() => {});
      // Surface dynamic-panel events as a toast so the user sees what Bob did.
      if (reason === 'dynamic-panel-saved') {
        pushToast('New panel ready - click to open', 'action', () =>
          setActivePanel('my-ui'),
        );
      } else if (reason === 'dynamic-panel-updated') {
        pushToast('Panel updated', 'info', () => setActivePanel('my-ui'));
      } else if (reason === 'dynamic-panel-deleted') {
        pushToast('Panel removed', 'info');
      } else if (reason === 'document-generated') {
        pushToast('Document ready - click to preview and edit', 'action', () =>
          setActivePanel('documents'),
        );
      }
    });
    // Fresh-install guidance: if no local model is downloaded, tell the
    // user how to get started instead of letting messages silently no-op.
    let modelToastShown = false;
    const unsubModel = onLocalLlmStatus((status) => {
      if (status === 'no-model' && !modelToastShown) {
        modelToastShown = true;
        pushToast('No local model yet. Click to download one and start.', 'info', () => {
          setActiveTab('run');
          setPanelStack([]);
          setActivePanel('settings');
        });
      }
    });
    const unsubBobToast = onBobToast((p) => {
      // Proactive suggestion from Bob: show it and let the user open the panel.
      pushToast(p.message || p.title, 'info', () => setActivePanel('suggestions'));
    });
    const unsubBobReview = onBobReview((data) => {
      setBobReview(data);
    });
    // Pair mode: one toast per stage. A pairing that leaves one human step
    // (restart the app, allow Accessibility) shows it as the toast's action.
    const unsubPair = onPairEvent((ev) => {
      // The mode exists to make one connection; a successful one ends it.
      if (ev.stage === 'connected') setSharingMode(false);
      if (ev.stage === 'connecting' || ev.stage === 'identifying') {
        pushToast(ev.text, 'info');
      } else if (ev.stage === 'connected' || ev.stage === 'updated') {
        const peer = ev.peer;
        const fu = peer?.follow_up;
        if (peer && fu) {
          // The step itself lives in the persistent bar above the chat;
          // the toast only announces it. Clicking runs it anyway.
          pushToast(`${peer.app} paired. One step left: ${fu.label}.`, 'action', () => {
            peerFollowUp(peer.app).catch((e) => pushToast(String(e), 'error'));
          });
        } else {
          pushToast(peer ? `${peer.app} paired. ${peer.bob_line}` : ev.text, 'success');
        }
      } else if (ev.stage === 'failed') {
        pushToast(ev.text, 'error');
      } else if (ev.stage === 'unpaired') {
        pushToast(ev.text, 'info');
      }
    });

    // ── Streaming: progressive message rendering ──
    const unsubStreamStart = onMessageStreamStart((data) => {
      // Add a placeholder message that will be updated with chunks
      const placeholder: Message = {
        id: data.message_id,
        role: 'assistant',
        content: '',
        agent_id: data.agent_id ?? undefined,
        timestamp: new Date().toISOString(),
      };
      setMessages(prev => {
        if (prev.some(m => m.id === data.message_id)) return prev;
        return [...prev, placeholder];
      });
    });
    // While a turn streams, hide tool machinery the moment it starts:
    // everything from the first <action> tag or bare tool-JSON onward is
    // execution, not prose. The step trail communicates progress; the
    // stream-end event replaces this with the processed final text.
    const stripStreamMachinery = (s: string): string => {
      const idxTag = s.indexOf('<action>');
      const jm = s.match(/\{"(?:type|tool)":/);
      const idxJson = jm ? (jm.index ?? -1) : -1;
      const cuts = [idxTag, idxJson].filter(i => i >= 0);
      return cuts.length ? s.slice(0, Math.min(...cuts)) : s;
    };
    const unsubChunk = onMessageChunk((data) => {
      // Update the streaming message with accumulated content
      setMessages(prev => prev.map(m =>
        m.id === data.message_id ? { ...m, content: stripStreamMachinery(data.content) } : m
      ));
    });
    const unsubStreamEnd = onMessageStreamEnd((data) => {
      // Final update - replace with the processed content (after action execution)
      if (data.content) {
        setMessages(prev => prev.map(m =>
          m.id === data.message_id ? { ...m, content: data.content! } : m
        ));
      }
    });

    // Wizard answers from <inputs> blocks dispatch through this event so
    // they go through the same optimistic-update path a typed message
    // takes - visible user bubble first, then bobSend.
    const onWizardAnswer = (e: Event) => {
      const text = (e as CustomEvent<{ text: string }>).detail?.text;
      if (!text) return;
      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: 'user',
        content: text,
        timestamp: new Date().toISOString(),
      };
      setMessages(prev => [...prev, userMsg]);
      bobSend(text).catch((err) => console.error('[wizard] bobSend failed', err));
    };
    window.addEventListener('orbix:wizard-answer', onWizardAnswer);

    // The sparkle button and the "imagine" chat command open Imagine,
    // with whatever was typed as Bob's one hint.
    const onOpenImagine = (e: Event) => {
      const detail = (e as CustomEvent<{ seed?: string }>).detail ?? {};
      setImagineSeed(detail.seed ?? '');
      setImagineOpen(true);
    };
    window.addEventListener('orbix:open-imagine', onOpenImagine);

    // Document chips in chat open the Documents panel.
    const onOpenDocuments = () => setActivePanel('documents');
    window.addEventListener('orbix:open-documents', onOpenDocuments);

    return () => {
      unsubOrb.then(fn => fn());
      unsubMsg.then(fn => fn());
      unsubTool.then(fn => fn());
      unsubConnector.then(fn => fn());
      unsubData.then(fn => fn());
      unsubMenu.then(fn => fn());
      unsubBobToast.then(fn => fn());
      unsubPair.then(fn => fn());
      unsubModel.then(fn => fn());
      unsubBobReview.then(fn => fn());
      unsubStreamStart.then(fn => fn());
      unsubChunk.then(fn => fn());
      unsubStreamEnd.then(fn => fn());
      window.removeEventListener('orbix:wizard-answer', onWizardAnswer);
      window.removeEventListener('orbix:open-imagine', onOpenImagine);
      window.removeEventListener('orbix:open-documents', onOpenDocuments);
    };
  }, []);

  const togglePanel = useCallback((panel: PanelType) => {
    setPanelStack([]);
    setActivePanel(prev => prev === panel ? null : panel);
  }, []);

  // Back: pop to the previous panel in the stack (e.g. Settings after
  // leaving Agents), or close the overlay when the stack is empty.
  const closePanel = useCallback(() => {
    const stack = panelStackRef.current;
    if (stack.length > 0) {
      setActivePanel(stack[stack.length - 1]);
      setPanelStack(stack.slice(0, -1));
    } else {
      setActivePanel(null);
    }
  }, []);

  // Dismiss the whole overlay regardless of depth (backdrop click).
  const closeAllPanels = useCallback(() => {
    setPanelStack([]);
    setActivePanel(null);
  }, []);

  // Drill into a panel, remembering the current one so Back returns here.
  const pushPanel = useCallback((panel: PanelType) => {
    const cur = activePanelRef.current;
    if (cur && cur !== panel) {
      setPanelStack(s => [...s, cur]);
    }
    setActivePanel(panel);
  }, []);

  const openPanel = useCallback((panel: string, workflowId?: string) => {
    setPanelStack([]);
    // Workflows live in the Build tab now; the old side panel is gone.
    if (panel === 'teach' || panel === 'workflows') {
      setBuildFocusId(workflowId || null);
      setActivePanel(null);
      setActiveTab('build');
      return;
    }
    setActivePanel(panel as PanelType);
  }, []);

  const clipboard = useClipboardCommands();

  const toggleTheme = useCallback(() => {
    const cur = document.documentElement.getAttribute('data-theme') === 'light' ? 'light' : 'dark';
    const next = cur === 'light' ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', next);
    localStorage.setItem('orbixcore-theme', next);
  }, []);

  const buildContextEntries = useCallback(
    (target: HTMLElement | null): ContextMenuEntry[] => {
      const isInputTarget = !!target?.closest('input, textarea, [contenteditable="true"]');
      const hasSel = clipboard.hasSelection();
      const themeIsLight = document.documentElement.getAttribute('data-theme') === 'light';

      return [
        { id: 'copy', label: 'Copy', shortcut: '⌘C', disabled: !hasSel, onSelect: clipboard.copy },
        { id: 'cut', label: 'Cut', shortcut: '⌘X', disabled: !hasSel || !isInputTarget, onSelect: clipboard.cut },
        { id: 'paste', label: 'Paste', shortcut: '⌘V', disabled: !isInputTarget, onSelect: clipboard.paste },
        { id: 'select-all', label: 'Select all', shortcut: '⌘A', onSelect: clipboard.selectAll },
        { separator: true },
        { id: 'palette', label: 'Command palette…', shortcut: '⌘K', onSelect: () => setCmdPaletteOpen(true) },
        { id: 'new-conv', label: 'New conversation', shortcut: '⌘N', onSelect: handleNewConversation },
        { id: 'voice', label: voiceModeOpen ? 'Exit voice mode' : 'Enter voice mode', shortcut: '⌘⇧V', onSelect: () => setVoiceModeOpen((v) => !v) },
        { separator: true },
        { id: 'theme', label: themeIsLight ? 'Switch to dark mode' : 'Switch to light mode', onSelect: toggleTheme },
        { id: 'rail', label: railCollapsed ? 'Show conversations rail' : 'Hide conversations rail', disabled: activeTab !== 'run', onSelect: () => setRailCollapsed((v) => !v) },
        { separator: true },
        { id: 'monitor', label: 'Run monitor', onSelect: () => openRunMonitor().catch(() => {}) },
        { id: 'stop-bob', label: 'Stop Bob', danger: true, onSelect: () => bobCancel().catch(() => {}) },
      ];
    },
    [clipboard, voiceModeOpen, railCollapsed, activeTab, handleNewConversation, toggleTheme],
  );

  const handleMessageSent = useCallback((msg: Message) => {
    setMessages(prev => {
      if (prev.some(m => m.id === msg.id)) return prev;
      return [...prev, msg];
    });
  }, []);

  const handleImagineClose = useCallback(() => setImagineOpen(false), []);

  if (showOnboarding) {
    return (
      <div className="app-root">
        <div className="app-column">
          <TitleBar />
          <Onboarding onComplete={() => {
            setShowOnboarding(false);
                    // Load Bob's seeded welcome conversation
            getConversation().then(msgs => {
              if (msgs && msgs.length > 0) setMessages(msgs);
            }).catch(() => {});
          }} />
        </div>
      </div>
    );
  }

  return (
    <div className="app-root">
      <div className="app-column">
        <TitleBar
          activeTab={activeTab}
          onTabChange={handleTabChange}
          onSettingsOpen={() => { setPanelStack([]); setActivePanel('settings'); }}
          sharingMode={sharingMode}
          onSharingModeToggle={sharingAvailable ? () => setSharingMode((v) => !v) : undefined}
        />

        <div className="main-layout">
          {/* Conversations rail (left) - only on Run; other tabs have their
              own first-column surfaces (Build's workflows sidebar, etc.) */}
          {activeTab === 'run' && (
            <ConversationsRail
              activeId={activeConversationId}
              onSelect={handleConversationSelect}
              onNew={handleNewConversation}
              collapsed={railCollapsed}
              onToggleCollapsed={() => setRailCollapsed((v) => !v)}
            />
          )}

          {/* Main content area */}
          <div className="main-content">
            {/* ── RUN TAB ── */}
            {activeTab === 'run' && (
              <RunView
                agents={agents}
                messages={messages}
                orbState={orbState}
                audioLevel={audioLevel}
                onOpenPanel={openPanel}
                onMessageSent={handleMessageSent}
                onRunWorkflow={handleRunWorkflow}
              />
            )}

            {/* ── BUILD TAB ── */}
            {activeTab === 'build' && (
              <Suspense fallback={null}>
                <BuildView
                  agents={agents}
                  onOpenPanel={openPanel}
                  focusWorkflowId={buildFocusId}
                />
              </Suspense>
            )}
          </div>

          {/* Context sidebar: Build's panel launcher. The chat surface owns
              nothing - panels open via Cmd+K, the gear, and Bob himself. */}
          {activeTab === 'build' && (
            <ContextSidebar
              activeTab={activeTab}
              activePanel={activePanel}
              onToggle={togglePanel}
            />
          )}
        </div>

        {/* IDE-style dev console: fully hidden until summoned (Cmd+J) -
            no permanent CONSOLE strip on the calm surface. */}
        {devDrawerOpen && (
          <DevDrawer
            open={devDrawerOpen}
            onToggle={() => setDevDrawerOpen((v) => !v)}
            agents={agents}
            height={devDrawerHeight}
            onHeightChange={setDevDrawerHeight}
          />
        )}
      </div>

      {/* Cmd+K Command Palette */}
      <CommandPalette
        open={cmdPaletteOpen}
        onClose={() => setCmdPaletteOpen(false)}
        onOpenPanel={(panel) => { setPanelStack([]); setActivePanel(panel); setCmdPaletteOpen(false); }}
        onRunWorkflow={handleRunWorkflow}
      />

      {/* Voice Mode overlay */}
      {voiceModeOpen && (
        <VoiceMode
          orbState={orbState}
          voiceSettings={voiceSettings}
          onExit={() => setVoiceModeOpen(false)}
          onMessage={handleMessageSent}
        />
      )}

      
      {/* Panel overlay - wrapped in ErrorBoundary so a crash in any panel
           doesn't white-screen the entire app. User sees a friendly "Try again"
           button instead. */}
      {activePanel && (
        <>
          <div className="panel-overlay-backdrop" onClick={closeAllPanels} />
          <div className={`panel-overlay${activePanel === 'settings' ? ' wide' : ''}`}>
          <ErrorBoundary fallbackLabel="Panel">
            <Suspense fallback={null}>
            {activePanel === 'agents' && <AgentsPanel agents={agents} setAgents={setAgents} onClose={closePanel} />}
            {activePanel === 'mcp' && <McpPanel servers={mcpServers} setServers={setMcpServers} onClose={closePanel} />}
            {activePanel === 'voice' && <VoicePanel settings={voiceSettings} setSettings={setVoiceSettings} onClose={closePanel} />}
            {activePanel === 'memory' && <MemoryPanel onClose={closePanel} onLoadConversation={(msgs) => setMessages(msgs)} />}
            {activePanel === 'tools' && <ToolsPanel onClose={closePanel} />}
            {activePanel === 'browser' && <BrowserPanel onClose={closePanel} />}
            {/* Screen panel removed - replaced by Window Intelligence */}
            {activePanel === 'connectors' && <ConnectorsPanel onClose={closePanel} />}
            {activePanel === 'triggers' && <TriggersPanel onClose={closePanel} />}
            {activePanel === 'inbox' && <InboxPanel onClose={closePanel} />}
            {activePanel === 'scheduler' && <SchedulerPanel onClose={closePanel} />}
            {activePanel === 'orchestrator' && <OrchestratorPanel onClose={closePanel} />}
            {activePanel === 'suggestions' && <SuggestionsPanel onClose={closePanel} />}
            {activePanel === 'dashboard' && <DashboardPanel onClose={closePanel} />}
            {activePanel === 'knowledge' && <KnowledgePanel onClose={closePanel} />}
            {activePanel === 'notifications' && <NotificationsPanel onClose={closePanel} onNavigate={(panel) => pushPanel(panel as PanelType)} />}
            {activePanel === 'search' && <SearchPanel onClose={closePanel} onLoadConversation={(msgs) => { setMessages(msgs); closePanel(); }} />}
            {activePanel === 'plugins' && <PluginsPanel onClose={closePanel} />}
            {activePanel === 'monitor' && <MonitorPanel onClose={closePanel} />}
            {activePanel === 'audit' && <AuditPanel onClose={closePanel} />}
            {activePanel === 'incidents' && <IncidentsPanel onClose={closePanel} />}
            {activePanel === 'approvals' && <ApprovalQueuePanel onClose={closePanel} />}
            {activePanel === 'window-intel' && <WindowIntelligencePanel onClose={closePanel} />}
            {activePanel === 'data-control' && <DataControlPanel onClose={closePanel} />}
            {activePanel === 'my-ui' && <MyUiPanel onClose={closePanel} />}
            {activePanel === 'documents' && <DocumentsPanel onClose={closePanel} />}
            {activePanel === 'guide' && <GuidePanel onClose={closePanel} />}
            {activePanel === 'code-findings' && <CodeFindingsPanel onClose={closePanel} />}
            {activePanel === 'settings' && (
              <SettingsPanel
                onClose={closePanel}
                voiceSettings={voiceSettings}
                onVoiceSettingsChange={setVoiceSettings}
                onOpenPanel={(panel) => pushPanel(panel as PanelType)}
              />
            )}
            </Suspense>
          </ErrorBoundary>
          </div>
        </>
      )}

      {/* Imagine - Bob with free rein; the finished run also lands in chat */}
      <Imagine open={imagineOpen} seed={imagineSeed} onClose={handleImagineClose} />

      {/* Sharing mode: animated frame + ports to drag onto another app. */}
      <PairMode active={sharingMode && sharingAvailable} onError={(m) => pushToast(m, 'error')} />

      {/* Bob notifications - fired on data-changed events Bob's responsible for. */}
      <BobToast toasts={toasts} onDismiss={dismissToast} />

      {/* Custom context menu - replaces the WebView's default right-click menu. */}
      <ContextMenu buildEntries={buildContextEntries} />

      {/* Bob Review Modal */}
      {bobReview && (
        <BobReviewModal
          review={bobReview}
          onClose={() => {
            setBobReview(null);
            // Refresh data in case Bob changed files
            listAgents().then(setAgents).catch(() => {});
          }}
        />
      )}

    </div>
  );
}
