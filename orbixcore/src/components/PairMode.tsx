import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { onPairEvent, peerBeginDrag, peerCancelDrag } from '../hooks/api';
import './PairMode.css';

/*
 * Sharing mode: the window grows an animated pink-to-orange frame. The
 * whole glowing edge is the grab surface. Nothing sits on it until the
 * cursor comes near; then a port fades in on the glow under the cursor
 * and trails it along the edge with damped easing (a requestAnimationFrame
 * loop mutating transforms directly, so no re-render per pointer move).
 * Press, and the wire leaves from exactly that point: the window converts
 * the port's position to screen coordinates and hands it to the native
 * overlay (pair_drag.rs), which tracks the rest of the gesture across
 * other apps. The port stays lit as the socket until the backend reports
 * the drag ended. Esc cancels.
 */

type Side = 'left' | 'right' | 'top' | 'bottom';
interface Point { x: number; y: number }

/** Window corner radius: ports never sit on the curve. The grab band's
 *  depth lives in PairMode.css (--pair-band). */
const CORNER = 16;
/** Easing per frame: lower is lazier. */
const FOLLOW = 0.16;
/** How long the port lingers after the cursor leaves the edge. */
const LINGER_MS = 260;

interface Props {
  active: boolean;
  onError?: (message: string) => void;
}

export default function PairMode({ active, onError }: Props) {
  const [dragging, setDragging] = useState(false);
  const [near, setNear] = useState(false);

  const portRef = useRef<HTMLDivElement>(null);
  const hotRef = useRef<HTMLDivElement>(null);
  const target = useRef<Point | null>(null);
  const pos = useRef<Point>({ x: 0, y: 0 });
  const raf = useRef(0);
  const lingerTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const anchored = useRef(false);

  const apply = () => {
    const t = `translate(${pos.current.x}px, ${pos.current.y}px)`;
    if (portRef.current) portRef.current.style.transform = t;
    if (hotRef.current) hotRef.current.style.transform = t;
  };

  const tick = useCallback(() => {
    const t = target.current;
    if (!t) { raf.current = 0; return; }
    const dx = t.x - pos.current.x;
    const dy = t.y - pos.current.y;
    if (Math.abs(dx) < 0.05 && Math.abs(dy) < 0.05) {
      pos.current = { x: t.x, y: t.y };
      apply();
      raf.current = 0;
      return;
    }
    pos.current = { x: pos.current.x + dx * FOLLOW, y: pos.current.y + dy * FOLLOW };
    apply();
    raf.current = requestAnimationFrame(tick);
  }, []);

  const kick = useCallback(() => {
    if (!raf.current) raf.current = requestAnimationFrame(tick);
  }, [tick]);

  // Snap a pointer inside a band onto the edge line, clear of the corners.
  const snap = useCallback((side: Side, clientX: number, clientY: number): Point => {
    const W = window.innerWidth;
    const H = window.innerHeight;
    const along = (v: number, max: number) => Math.min(Math.max(v, CORNER), max - CORNER);
    switch (side) {
      case 'left': return { x: 1, y: along(clientY, H) };
      case 'right': return { x: W - 1, y: along(clientY, H) };
      case 'top': return { x: along(clientX, W), y: 1 };
      case 'bottom': return { x: along(clientX, W), y: H - 1 };
    }
  }, []);

  const show = useCallback((p: Point) => {
    if (lingerTimer.current) { clearTimeout(lingerTimer.current); lingerTimer.current = null; }
    if (!target.current) {
      // First contact: appear where the cursor is, then trail it.
      pos.current = { x: p.x, y: p.y };
      apply();
      setNear(true);
    }
    target.current = p;
    kick();
  }, [kick]);

  const hide = useCallback((immediate = false) => {
    if (lingerTimer.current) { clearTimeout(lingerTimer.current); lingerTimer.current = null; }
    const doHide = () => {
      target.current = null;
      setNear(false);
    };
    if (immediate) doHide();
    else lingerTimer.current = setTimeout(doHide, LINGER_MS);
  }, []);

  const release = useCallback(() => {
    if (!anchored.current) return;
    anchored.current = false;
    setDragging(false);
    // The cursor is wherever the drop was; let the port go.
    hide(true);
  }, [hide]);

  // The backend owns the end of the gesture: connected, cancelled, failed.
  useEffect(() => {
    const unsub = onPairEvent((ev) => {
      if (ev.stage === 'connected' || ev.stage === 'cancelled' || ev.stage === 'failed' || ev.stage === 'identifying') {
        release();
      }
    });
    return () => { unsub.then((fn) => fn()); };
  }, [release]);

  useEffect(() => {
    if (!dragging) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        peerCancelDrag().catch(() => {});
        release();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [dragging, release]);

  useEffect(() => {
    if (!active) {
      if (anchored.current) peerCancelDrag().catch(() => {});
      anchored.current = false;
      setDragging(false);
      hide(true);
    }
  }, [active, hide]);

  useEffect(() => () => {
    if (raf.current) cancelAnimationFrame(raf.current);
    if (lingerTimer.current) clearTimeout(lingerTimer.current);
  }, []);

  if (!active) return null;

  const start = async (side: Side, e: React.PointerEvent) => {
    if (e.button !== 0 || anchored.current) return;
    e.preventDefault();
    const p = snap(side, e.clientX, e.clientY);
    // Anchor exactly where the wire will start: no trailing from here on.
    anchored.current = true;
    target.current = p;
    pos.current = { x: p.x, y: p.y };
    apply();
    setNear(true);
    setDragging(true);
    try {
      // The port's position in global logical points: where the wire starts.
      const win = getCurrentWindow();
      const [wpos, scale] = await Promise.all([win.innerPosition(), win.scaleFactor()]);
      await peerBeginDrag({ x: wpos.x / scale + p.x, y: wpos.y / scale + p.y });
    } catch (err) {
      release();
      onError?.(String(err));
    }
  };

  const hint = dragging
    ? 'Release over an app to connect'
    : near
      ? 'Drag onto an app'
      : 'Drag from the glow onto an app to connect';

  return (
    <div className={`pair-mode ${dragging ? 'dragging' : ''} ${near ? 'near' : ''}`} aria-live="polite">
      <div className="pair-mode-frame" />
      <div className="pair-mode-glow" />

      {/* Grab bands: the inside of each edge. Invisible; they only carry the
          cursor, the hover port, and the press. */}
      {(['left', 'right', 'top', 'bottom'] as Side[]).map((side) => (
        <div
          key={side}
          className={`pair-mode-band ${side}`}
          onPointerMove={(e) => { if (!anchored.current) show(snap(side, e.clientX, e.clientY)); }}
          onPointerLeave={() => { if (!anchored.current) hide(); }}
          onPointerDown={(e) => { void start(side, e); }}
        />
      ))}

      {/* The one port: on the glow under the cursor, or anchored as the
          socket while the wire is out. Position is set imperatively. */}
      <div ref={hotRef} className={`pair-mode-hot ${near ? 'on' : ''}`} />
      <div ref={portRef} className={`pair-mode-port ${near ? 'on' : ''} ${dragging ? 'anchored' : ''}`} />

      <div className="pair-mode-hint">{hint}</div>
    </div>
  );
}
