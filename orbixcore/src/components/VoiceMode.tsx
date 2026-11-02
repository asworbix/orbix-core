import { useState, useEffect, useRef, useCallback } from 'react';
import type { OrbState, VoiceSettings, Message } from '../types';
import {
  startListening, stopListening, speakText, stopSpeaking,
  bobSend, onMessageChunk,
  onAudioLevel, onSilenceDetected, onSttResult, onSttPartial, onTtsState,
  setOrbState,
} from '../hooks/api';
import OrbCore from './OrbCore';
import './VoiceMode.css';

// Split off any COMPLETE sentences from the front of `buf`, returning the
// spoken sentences and the leftover tail. A sentence ends at . ! ? (optionally
// followed by a closing quote/bracket) and whitespace-or-end.
function takeSentences(buf: string): { sentences: string[]; rest: string } {
  const re = /[^.!?]*[.!?]+["')\]]*(?:\s+|$)/g;
  const sentences: string[] = [];
  let lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(buf)) !== null) {
    const s = m[0].trim();
    if (s) sentences.push(s);
    lastIndex = re.lastIndex;
  }
  return { sentences, rest: buf.slice(lastIndex) };
}

interface Props {
  orbState: OrbState;
  voiceSettings: VoiceSettings;
  onExit: () => void;
  onMessage: (msg: Message) => void;
}

type VoiceModePhase = 'idle' | 'listening' | 'thinking' | 'speaking';

export default function VoiceMode({ orbState, voiceSettings, onExit, onMessage }: Props) {
  const [phase, setPhase] = useState<VoiceModePhase>('idle');
  const [subtitle, setSubtitle] = useState('');
  const [subtitleFading, setSubtitleFading] = useState(false);
  const [audioLevel, setAudioLevel] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animFrameRef = useRef<number>(0);
  const levelHistoryRef = useRef<number[]>(new Array(64).fill(0));
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  // Waveform drawing
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = () => {
      const w = canvas.width;
      const h = canvas.height;
      ctx.clearRect(0, 0, w, h);

      const history = levelHistoryRef.current;

      // Draw waveform bars
      const barCount = history.length;
      const barWidth = w / barCount;
      const gap = 2;

      for (let i = 0; i < barCount; i++) {
        const level = history[i];
        const barHeight = Math.max(2, level * h * 0.8);
        const x = i * barWidth + gap / 2;
        const y = (h - barHeight) / 2;

        // Warm terracotta → peach gradient across the bar field, matching
        // the app's clay accent.
        const hue = 18 + (i / barCount) * 16;
        const alpha = 0.4 + level * 0.6;
        ctx.fillStyle = `hsla(${hue}, 60%, 62%, ${alpha})`;
        ctx.beginPath();
        ctx.roundRect(x, y, barWidth - gap, barHeight, 2);
        ctx.fill();
      }

      animFrameRef.current = requestAnimationFrame(draw);
    };

    const resizeCanvas = () => {
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * window.devicePixelRatio;
      canvas.height = rect.height * window.devicePixelRatio;
      ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
    };
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    animFrameRef.current = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(animFrameRef.current);
      window.removeEventListener('resize', resizeCanvas);
    };
  }, []);

  // Update level history when audioLevel changes
  useEffect(() => {
    const history = levelHistoryRef.current;
    history.shift();
    history.push(audioLevel);
  }, [audioLevel]);

  // Listen for audio levels
  useEffect(() => {
    const unsub = onAudioLevel((level) => {
      setAudioLevel(level);
    });
    return () => { unsub.then(fn => fn()); };
  }, []);

  // Auto-stop listening when VAD detects sustained silence
  useEffect(() => {
    const unsubPartial = onSttPartial((text) => {
      if (phaseRef.current === 'listening' && text) {
        setSubtitle(text);
        setSubtitleFading(false);
      }
    });
    const unsub = onSilenceDetected(() => {
      if (phaseRef.current === 'listening') {
        stopListening().catch(() => {});
      }
    });
    return () => { unsub.then(fn => fn()); unsubPartial.then(fn => fn()); };
  }, []);

  // Listen for STT results
  useEffect(() => {
    const unsub = onSttResult(async (text) => {
      if (!text.trim()) return;

      // Show user's speech as subtitle
      setSubtitle(text);
      setSubtitleFading(false);
      setPhase('thinking');
      setOrbState('thinking').catch(() => {});

      // Add user message
      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: 'user',
        content: text,
        timestamp: new Date().toISOString(),
      };
      onMessage(userMsg);

      // Send to AI, speaking each sentence as it streams in instead of
      // waiting for the whole reply. A single sequential queue keeps
      // playback in order (concurrent speakText calls would overlap).
      try {
        setPhase('speaking');
        setOrbState('executing').catch(() => {});

        let lastFull = '';          // sanitized cumulative content of the current model turn
        let spokenLen = 0;          // chars of lastFull already queued for speech
        let buf = '';               // unspoken tail
        const queue: string[] = [];
        let draining = false;
        const drain = async () => {
          if (draining) return;
          draining = true;
          while (queue.length) {
            // Stop if the user barged in (phase left 'speaking').
            if (phaseRef.current !== 'speaking') { queue.length = 0; break; }
            const sentence = queue.shift() as string;
            try { await speakText(sentence); } catch { /* keep going */ }
          }
          draining = false;
        };
        // Never read machinery aloud: <action> tool calls, <think>
        // deliberation (the model's reasoning channel is wrapped in these
        // by the streamer), <inputs> wizards and <plan> blocks are all
        // stripped when complete, and the queue holds at an unclosed tag
        // until the rest of it streams in.
        const MACHINE_TAGS = ['action', 'think', 'inputs', 'plan'];
        const sanitize = (s: string) => {
          let out = s;
          for (const tag of MACHINE_TAGS) {
            out = out.replace(new RegExp(`<${tag}>[\\s\\S]*?</${tag}>`, 'g'), ' ');
            const open = out.indexOf(`<${tag}`);
            if (open !== -1) out = out.slice(0, open);
          }
          return out;
        };
        // Sentence-level dedup so the definitive final can be enqueued in
        // full without repeating anything streaming already spoke.
        const spokenSet = new Set<string>();
        let liveCount = 0; // sentences spoken live from the current turn
        const norm = (s: string) => s.replace(/\s+/g, ' ').trim().toLowerCase();
        const pushSentences = (sentences: string[], live: boolean) => {
          for (const sen of sentences) {
            // Live cap: small local models often open with deliberation the
            // backend will cut from the final. Speak at most two sentences
            // as they stream; the rest arrives (contract-enforced) with the
            // final and is deduped here.
            if (live && liveCount >= 2) continue;
            const key = norm(sen);
            if (!key || spokenSet.has(key)) continue;
            spokenSet.add(key);
            if (live) liveCount++;
            queue.push(sen);
          }
          if (queue.length) void drain();
        };
        const enqueueFrom = (rawContent: string) => {
          const full = sanitize(rawContent);
          // Bob's agentic loop restarts the stream on every model turn:
          // earlier turns are pre-action rationale, the LAST turn is the
          // user-facing answer. A content reset means a new turn began -
          // drop the unspoken rationale and follow the new turn.
          if (!full.startsWith(lastFull)) {
            queue.length = 0;
            buf = '';
            spokenLen = 0;
            liveCount = 0;
          }
          lastFull = full;
          if (full.length <= spokenLen) return;
          buf += full.slice(spokenLen);
          spokenLen = full.length;
          const { sentences, rest } = takeSentences(buf);
          buf = rest;
          pushSentences(sentences, true);
        };

        // message-chunk carries the cumulative content of the current turn.
        const unsubChunk = await onMessageChunk((data) => {
          const c = (data.content ?? '') as string;
          enqueueFrom(c);
          setSubtitle(lastFull);
          setSubtitleFading(false);
        });

        const response = await bobSend(text);
        unsubChunk();
        onMessage(response);
        // The definitive reply: the backend enforces the reply contract on
        // it (deliberation before machinery is cut), and sentence dedup
        // skips whatever streaming already spoke.
        {
          const finalSan = sanitize(response.content);
          const { sentences, rest } = takeSentences(finalSan + ' ');
          const all = rest.trim() ? [...sentences, rest.trim()] : sentences;
          pushSentences(all, false);
        }
        // The whole reply was machinery (e.g. an <inputs> wizard): say so
        // instead of going silent - the questions render in the chat.
        if (queue.length === 0 && !draining && spokenSet.size === 0 && response.content.includes('<inputs')) {
          queue.push('I need a bit more information. Take a look at the questions in the chat.');
          void drain();
        }
        // Wait for the queue to finish speaking before moving on.
        while (queue.length || draining) {
          await new Promise((r) => setTimeout(r, 80));
        }

        // After speaking, either return to listening (auto) or idle
        setSubtitleFading(true);
        setTimeout(() => setSubtitle(''), 2000);

        if (voiceSettings.auto_speak) {
          startConversation();
        } else {
          setPhase('idle');
          setOrbState('idle').catch(() => {});
        }
      } catch {
        setSubtitle('Sorry, something went wrong.');
        setSubtitleFading(true);
        setTimeout(() => setSubtitle(''), 3000);
        setPhase('idle');
        setOrbState('idle').catch(() => {});
      }
    });
    return () => { unsub.then(fn => fn()); };
  }, [voiceSettings.auto_speak]);

  // Listen for TTS state changes
  useEffect(() => {
    const unsub = onTtsState((state) => {
      if (state === 'idle' && phaseRef.current === 'speaking') {
        // TTS finished
      }
    });
    return () => { unsub.then(fn => fn()); };
  }, []);

  const startConversation = useCallback(async () => {
    setPhase('listening');
    setSubtitle('');
    setSubtitleFading(false);
    try {
      await startListening();
    } catch {
      setPhase('idle');
    }
  }, []);

  const stopConversation = useCallback(async () => {
    if (phaseRef.current === 'listening') {
      try {
        await stopListening();
      } catch { /* */ }
    } else if (phaseRef.current === 'speaking') {
      await stopSpeaking().catch(() => {});
      setPhase('idle');
      setOrbState('idle').catch(() => {});
    }
  }, []);

  const handleOrbClick = useCallback(() => {
    if (phase === 'idle') {
      startConversation();
    } else if (phase === 'listening') {
      stopConversation();
    } else if (phase === 'speaking') {
      stopConversation();
    }
  }, [phase, startConversation, stopConversation]);

  // Keyboard: space to toggle, escape to exit
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        // Stop everything and exit
        if (phaseRef.current === 'listening') {
          stopListening().catch(() => {});
        } else if (phaseRef.current === 'speaking') {
          stopSpeaking().catch(() => {});
        }
        setOrbState('idle').catch(() => {});
        onExit();
        return;
      }
      if (e.key === ' ' && e.target === document.body) {
        e.preventDefault();
        handleOrbClick();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [handleOrbClick, onExit]);

  const phaseLabel = {
    idle: 'Tap the orb or press space to start',
    listening: 'Listening...',
    thinking: 'Thinking...',
    speaking: 'Speaking...',
  }[phase];

  return (
    <div className="voice-mode">
      {/* Exit button */}
      <button className="voice-exit-btn" onClick={() => {
        if (phaseRef.current === 'listening') stopListening().catch(() => {});
        if (phaseRef.current === 'speaking') stopSpeaking().catch(() => {});
        setOrbState('idle').catch(() => {});
        onExit();
      }}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
        Exit Voice Mode
      </button>

      {/* Orb */}
      <div className={`voice-orb-wrapper voice-phase-${phase}`} onClick={handleOrbClick}>
        <OrbCore state={orbState} />
      </div>

      {/* Phase label */}
      <div className={`voice-phase-label ${phase}`}>{phaseLabel}</div>

      {/* Waveform */}
      <div className={`voice-waveform ${phase === 'listening' ? 'active' : ''}`}>
        <canvas ref={canvasRef} className="voice-waveform-canvas" />
      </div>

      {/* Subtitle */}
      {subtitle && (
        <div className={`voice-subtitle ${subtitleFading ? 'fading' : ''}`}>
          {subtitle}
        </div>
      )}

      {/* Footer hints */}
      <div className="voice-hints">
        <span><kbd>space</kbd> {phase === 'idle' ? 'start' : 'stop'}</span>
        <span><kbd>esc</kbd> exit</span>
      </div>
    </div>
  );
}
