import { useEffect, useRef } from 'react';
import { onPairDrag } from '../hooks/api';
import type { PairDragFrame } from '../types';
import './PairOverlay.css';

/*
 * The drag overlay: a transparent, click-through window that covers the
 * screen while the user drags Orbix's pair handle onto another app. Rust
 * polls the cursor and the window under it (pair_drag.rs) and streams
 * frames here; this component only draws: the bridge from the handle to
 * the cursor, a glow around the window it would connect to, and one
 * label saying what a release will do.
 */

const PINK = '#ff5fa2';
const ORANGE = '#ff9a3c';

export default function PairOverlay() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRef = useRef<PairDragFrame | null>(null);
  const releasedAtRef = useRef<number | null>(null);

  useEffect(() => {
    document.documentElement.style.background = 'transparent';
    document.body.style.background = 'transparent';
    document.body.classList.add('pair-overlay-body');

    const unsub = onPairDrag((f) => {
      frameRef.current = f;
      if (!f.active && releasedAtRef.current === null) releasedAtRef.current = performance.now();
    });

    let raf = 0;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const canvas = canvasRef.current;
      if (!canvas) return;
      const dpr = window.devicePixelRatio || 1;
      const W = window.innerWidth;
      const H = window.innerHeight;
      if (canvas.width !== Math.round(W * dpr) || canvas.height !== Math.round(H * dpr)) {
        canvas.width = Math.round(W * dpr);
        canvas.height = Math.round(H * dpr);
        canvas.style.width = `${W}px`;
        canvas.style.height = `${H}px`;
      }
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, W, H);
      const f = frameRef.current;
      if (!f) return;

      const t = performance.now() / 1000;
      const released = releasedAtRef.current !== null;
      const fade = released ? Math.max(0, 1 - (performance.now() - (releasedAtRef.current ?? 0)) / 260) : 1;
      ctx.globalAlpha = fade;

      // The bridge: a bezier that leaves the handle horizontally and lands
      // horizontally on the cursor, like a wire between two ports.
      const dx = f.x - f.ox;
      const bend = Math.max(60, Math.abs(dx) * 0.45);
      const grad = ctx.createLinearGradient(f.ox, f.oy, f.x, f.y);
      grad.addColorStop(0, PINK);
      grad.addColorStop(1, ORANGE);
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      ctx.beginPath();
      ctx.moveTo(f.ox, f.oy);
      ctx.bezierCurveTo(f.ox + bend * Math.sign(dx || 1), f.oy, f.x - bend * Math.sign(dx || 1), f.y, f.x, f.y);
      ctx.shadowColor = PINK;
      ctx.shadowBlur = 18;
      ctx.strokeStyle = grad;
      ctx.lineWidth = 3;
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Ports at both ends.
      for (const [px, py, color] of [[f.ox, f.oy, PINK], [f.x, f.y, ORANGE]] as const) {
        ctx.beginPath();
        ctx.arc(px, py, 6, 0, Math.PI * 2);
        ctx.fillStyle = '#1b1c1e';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = color;
        ctx.stroke();
      }

      // The window we would connect to: a soft gradient frame that breathes.
      if (f.target) {
        const { x, y, w, h } = f.target;
        const pulse = 0.75 + 0.25 * Math.sin(t * 4);
        const r = 14;
        const g2 = ctx.createLinearGradient(x, y, x + w, y + h);
        g2.addColorStop(0, PINK);
        g2.addColorStop(1, ORANGE);
        roundRect(ctx, x + 1.5, y + 1.5, w - 3, h - 3, r);
        ctx.shadowColor = ORANGE;
        ctx.shadowBlur = 28 * pulse;
        ctx.strokeStyle = g2;
        ctx.lineWidth = 3;
        ctx.stroke();
        ctx.shadowBlur = 0;
        ctx.globalAlpha = fade * 0.08 * pulse;
        ctx.fillStyle = ORANGE;
        ctx.fill();
        ctx.globalAlpha = fade;
      }

      // One label: what a release does.
      const label = f.target
        ? released ? `Connecting to ${f.target.name}…` : `Release to connect to ${f.target.name}`
        : 'Drag onto an app window';
      ctx.font = '500 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif';
      const tw = ctx.measureText(label).width;
      const padX = 12;
      const lw = tw + padX * 2;
      const lh = 28;
      let lx = f.x + 18;
      let ly = f.y - lh / 2;
      if (lx + lw > W - 8) lx = f.x - 18 - lw;
      if (ly < 8) ly = 8;
      if (ly + lh > H - 8) ly = H - 8 - lh;
      roundRect(ctx, lx, ly, lw, lh, lh / 2);
      ctx.fillStyle = 'rgba(28, 29, 32, 0.92)';
      ctx.shadowColor = 'rgba(0,0,0,0.35)';
      ctx.shadowBlur = 12;
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.strokeStyle = f.target ? ORANGE : 'rgba(255,255,255,0.18)';
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.fillStyle = '#f2efe9';
      ctx.textBaseline = 'middle';
      ctx.fillText(label, lx + padX, ly + lh / 2 + 0.5);
      ctx.globalAlpha = 1;
    };
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf);
      unsub.then((fn) => fn());
    };
  }, []);

  return <canvas ref={canvasRef} className="pair-overlay-canvas" />;
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}
