import { useEffect, useState, useRef, useCallback, useMemo } from 'react';
import './ContextMenu.css';

export interface ContextMenuItem {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  separator?: never;
  onSelect: () => void | Promise<void>;
}

export interface ContextMenuSeparator {
  separator: true;
}

export type ContextMenuEntry = ContextMenuItem | ContextMenuSeparator;

interface Props {
  /** Callback invoked when the user opens a context menu somewhere in the
   *  app. Receives the click target so the host can branch on tag/role,
   *  and returns the entries to show. Return an empty array to suppress. */
  buildEntries: (target: HTMLElement | null) => ContextMenuEntry[];
}

interface OpenState {
  x: number;
  y: number;
  entries: ContextMenuEntry[];
}

const MENU_W = 240;
const MENU_GAP = 4;

export default function ContextMenu({ buildEntries }: Props) {
  const [state, setState] = useState<OpenState | null>(null);
  const [active, setActive] = useState(0);
  const ref = useRef<HTMLDivElement>(null);

  // Suppress the WebView default context menu globally and open ours.
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      e.preventDefault();
      const target = e.target as HTMLElement | null;
      const entries = buildEntries(target);
      if (entries.length === 0) {
        setState(null);
        return;
      }

      // Clamp position so the menu stays inside the viewport.
      const vh = window.innerHeight;
      const vw = window.innerWidth;
      const itemRows = entries.filter((it) => !('separator' in it)).length;
      const sepRows = entries.length - itemRows;
      const estH = itemRows * 30 + sepRows * 9 + 8;
      const x = Math.min(e.clientX, vw - MENU_W - MENU_GAP);
      const y = Math.min(e.clientY, vh - estH - MENU_GAP);
      setState({ x, y, entries });
      setActive(firstEnabledIndex(entries));
    };
    window.addEventListener('contextmenu', handler);
    return () => window.removeEventListener('contextmenu', handler);
  }, [buildEntries]);

  // Dismiss on click-outside / escape / scroll / window blur.
  useEffect(() => {
    if (!state) return;
    const close = () => setState(null);
    const onKey = (e: KeyboardEvent) => {
      if (!state) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActive((i) => nextEnabledIndex(state.entries, i, 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActive((i) => nextEnabledIndex(state.entries, i, -1));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const item = state.entries[active];
        if (item && !('separator' in item) && !item.disabled) {
          void item.onSelect();
          close();
        }
      }
    };
    const onMouse = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    document.addEventListener('mousedown', onMouse);
    document.addEventListener('keydown', onKey);
    window.addEventListener('blur', close);
    window.addEventListener('scroll', close, true);
    return () => {
      document.removeEventListener('mousedown', onMouse);
      document.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', close);
      window.removeEventListener('scroll', close, true);
    };
  }, [state, active]);

  if (!state) return null;

  return (
    <div
      ref={ref}
      className="ctx-menu"
      style={{ left: state.x, top: state.y, width: MENU_W }}
      role="menu"
      tabIndex={-1}
    >
      {state.entries.map((entry, i) => {
        if ('separator' in entry) {
          return <div key={`sep-${i}`} className="ctx-menu-sep" />;
        }
        const isActive = i === active && !entry.disabled;
        return (
          <button
            key={entry.id}
            role="menuitem"
            className={`ctx-menu-item ${isActive ? 'active' : ''} ${entry.disabled ? 'disabled' : ''} ${entry.danger ? 'danger' : ''}`}
            onMouseEnter={() => !entry.disabled && setActive(i)}
            onClick={() => {
              if (entry.disabled) return;
              void entry.onSelect();
              setState(null);
            }}
            disabled={entry.disabled}
          >
            <span className="ctx-menu-label">{entry.label}</span>
            {entry.shortcut && <span className="ctx-menu-shortcut">{entry.shortcut}</span>}
          </button>
        );
      })}
    </div>
  );
}

function firstEnabledIndex(entries: ContextMenuEntry[]): number {
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i];
    if (!('separator' in e) && !e.disabled) return i;
  }
  return 0;
}

function nextEnabledIndex(entries: ContextMenuEntry[], from: number, dir: 1 | -1): number {
  const n = entries.length;
  for (let step = 1; step <= n; step++) {
    const i = ((from + dir * step) % n + n) % n;
    const e = entries[i];
    if (!('separator' in e) && !e.disabled) return i;
  }
  return from;
}

/** Hook helpers - exposed for callers building entry lists. */
export function useClipboardCommands() {
  const hasSelection = useMemo(() => {
    return () => {
      const sel = window.getSelection();
      return !!sel && sel.toString().length > 0;
    };
  }, []);

  const copy = useCallback(() => {
    const sel = window.getSelection();
    if (sel && sel.toString()) {
      navigator.clipboard.writeText(sel.toString()).catch(() => {});
    }
  }, []);

  const cut = useCallback(() => {
    const sel = window.getSelection();
    const text = sel?.toString();
    if (!text) return;
    navigator.clipboard.writeText(text).catch(() => {});
    const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
      const start = active.selectionStart ?? 0;
      const end = active.selectionEnd ?? 0;
      active.value = active.value.slice(0, start) + active.value.slice(end);
      active.selectionStart = active.selectionEnd = start;
      active.dispatchEvent(new Event('input', { bubbles: true }));
    }
  }, []);

  const paste = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (!text) return;
      const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
      if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
        const start = active.selectionStart ?? active.value.length;
        const end = active.selectionEnd ?? active.value.length;
        active.value = active.value.slice(0, start) + text + active.value.slice(end);
        const caret = start + text.length;
        active.selectionStart = active.selectionEnd = caret;
        active.dispatchEvent(new Event('input', { bubbles: true }));
      }
    } catch {
      /* permissions denied or clipboard empty */
    }
  }, []);

  const selectAll = useCallback(() => {
    const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
      active.select();
      return;
    }
    const sel = window.getSelection();
    if (!sel) return;
    sel.removeAllRanges();
    const range = document.createRange();
    range.selectNodeContents(document.body);
    sel.addRange(range);
  }, []);

  return { hasSelection, copy, cut, paste, selectAll };
}
