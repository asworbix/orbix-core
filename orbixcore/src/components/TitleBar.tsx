import { useState, useEffect, useRef } from 'react';
import type { TabId } from '../types';
import './TitleBar.css';
import orbixLogo from '../assets/orbix-logo.png';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';

interface SessionCost {
  total_tokens_in: number;
  total_tokens_out: number;
  total_cost_usd: number;
  call_count: number;
  local_calls: number;
  cloud_calls: number;
}

interface Props {
  activeTab?: TabId;
  onTabChange?: (tab: TabId) => void;
  onSettingsOpen?: () => void;
  /** Sharing mode: the window shows its ports to drag onto other apps. */
  sharingMode?: boolean;
  onSharingModeToggle?: () => void;
}

// 'run' is the historical internal id; the user-facing name is Chat.
const TABS: { id: TabId; label: string }[] = [
  { id: 'run', label: 'Chat' },
  { id: 'build', label: 'Build' },
];

export default function TitleBar({
  activeTab = 'run',
  onTabChange,
  onSettingsOpen,
  sharingMode = false,
  onSharingModeToggle,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [cost, setCost] = useState<SessionCost | null>(null);

  // Listen for cost updates
  useEffect(() => {
    const unsub = listen<SessionCost>('cost-update', (e) => setCost(e.payload));
    return () => { unsub.then(fn => fn()); };
  }, []);

  // Attach native DOM mousedown listener for window dragging
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      // Skip interactive elements
      if (target.closest('button') || target.closest('.titlebar-tabs')) return;
      e.preventDefault();
      getCurrentWindow().startDragging().catch(() => {});
    };
    el.addEventListener('mousedown', handler);
    return () => el.removeEventListener('mousedown', handler);
  }, []);

  return (
    <div className="titlebar" ref={ref}>
      <div className="titlebar-left">
      </div>
      <div className="titlebar-center">
        <div className="titlebar-tabs">
          {TABS.map(tab => (
            <button
              key={tab.id}
              className={`titlebar-tab ${activeTab === tab.id ? 'active' : ''}`}
              onClick={() => onTabChange?.(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>
      <div className="titlebar-right">
        {cost && cost.call_count > 0 && (
          <div className={`titlebar-cost ${cost.total_cost_usd > 0.1 ? 'warn' : cost.total_cost_usd > 0 ? 'cloud' : 'local'}`}
            title={`${cost.call_count} calls (${cost.local_calls} local, ${cost.cloud_calls} cloud) · ${cost.total_tokens_in + cost.total_tokens_out} tokens`}
          >
            ${cost.total_cost_usd.toFixed(4)}
          </div>
        )}
        {onSharingModeToggle && (
          <button
            className={`titlebar-icon-btn titlebar-share-btn ${sharingMode ? 'active' : ''}`}
            onClick={onSharingModeToggle}
            title={sharingMode ? 'Sharing mode is on: drag an edge onto an app to connect it. Click to turn off.' : 'Sharing mode: connect another app by dragging an edge onto it'}
            aria-label="Sharing mode"
            aria-pressed={sharingMode}
          >
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
              <circle cx="5" cy="12" r="2.4"/>
              <circle cx="19" cy="12" r="2.4"/>
              <path d="M7.4 12c3 0 3-5 6-5s3 5 3.2 5"/>
            </svg>
          </button>
        )}
        {onSettingsOpen && (
          <button className="titlebar-icon-btn" onClick={onSettingsOpen} title="Settings" aria-label="Settings">
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
          </button>
        )}
        <img src={orbixLogo} alt="" className="titlebar-logo-icon" aria-hidden="true" />
        <WindowControls />
      </div>
    </div>
  );
}

// Custom min/maximize/close buttons. On macOS the OS already overlays the
// traffic-light buttons via `titleBarStyle: "Overlay"`, so this stays hidden
// (CSS gates it on [data-platform="win"]). On Windows we render our own
// because the native chrome is disabled (decorations: false). Exported so
// TitleBar-less screens (Login) can still offer minimize/close.
export function WindowControls() {
  const win = getCurrentWindow();
  return (
    <div className="titlebar-winctl">
      <button
        className="titlebar-winctl-btn min"
        title="Minimise"
        onClick={() => { win.minimize().catch(() => {}); }}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      <button
        className="titlebar-winctl-btn max"
        title="Maximise"
        onClick={() => { win.toggleMaximize().catch(() => {}); }}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      <button
        className="titlebar-winctl-btn close"
        title="Close"
        onClick={() => { win.close().catch(() => {}); }}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" strokeWidth="1" />
          <line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
    </div>
  );
}
