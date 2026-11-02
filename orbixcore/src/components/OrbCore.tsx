import { useState, useRef, useCallback, useMemo } from 'react';
import type { OrbState } from '../types';
import './OrbCore.css';

interface Props {
  state: OrbState;
  amplitude?: number;       // 0-1 audio level
  pairingColors?: string[]; // color ring segments for pairing beacon
  onInteract?: (type: 'click' | 'hover-start' | 'hover-end') => void;
}

const ORB_LABELS: Record<OrbState, string> = {
  idle: '',
  listening: 'Listening...',
  thinking: '',
  executing: '',
  learning: '',
  error: '',
};

/** Time-of-day ambient tint */
function getAmbientTint(): { hue: number; warmth: number } {
  const hour = new Date().getHours();
  if (hour >= 6 && hour < 10) return { hue: 30, warmth: 0.15 };   // warm sunrise
  if (hour >= 10 && hour < 17) return { hue: 220, warmth: 0.05 }; // neutral day
  if (hour >= 17 && hour < 20) return { hue: 15, warmth: 0.12 };  // warm sunset
  return { hue: 240, warmth: 0.08 };                                // cool night
}

export default function OrbCore({ state, amplitude = 0, pairingColors, onInteract }: Props) {
  const label = ORB_LABELS[state];
  const wrapperRef = useRef<HTMLDivElement>(null);
  const [ripple, setRipple] = useState<{ x: number; y: number; id: number } | null>(null);
  const [isHovered, setIsHovered] = useState(false);
  const [mousePos, setMousePos] = useState({ x: 0.5, y: 0.5 });

  const ambient = useMemo(() => getAmbientTint(), []);

  // Amplitude-driven CSS custom properties
  const ampScale = 1 + amplitude * 0.08;
  const ampGlow = amplitude * 0.4;

  const handleClick = useCallback((e: React.MouseEvent) => {
    const rect = wrapperRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = ((e.clientX - rect.left) / rect.width) * 100;
    const y = ((e.clientY - rect.top) / rect.height) * 100;
    setRipple({ x, y, id: Date.now() });
    setTimeout(() => setRipple(null), 600);
    onInteract?.('click');
  }, [onInteract]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    const rect = wrapperRef.current?.getBoundingClientRect();
    if (!rect) return;
    setMousePos({
      x: (e.clientX - rect.left) / rect.width,
      y: (e.clientY - rect.top) / rect.height,
    });
  }, []);

  const handleMouseEnter = useCallback(() => {
    setIsHovered(true);
    onInteract?.('hover-start');
  }, [onInteract]);

  const handleMouseLeave = useCallback(() => {
    setIsHovered(false);
    setMousePos({ x: 0.5, y: 0.5 });
    onInteract?.('hover-end');
  }, [onInteract]);

  // Slight tilt toward cursor
  const tiltX = (mousePos.y - 0.5) * (isHovered ? -8 : 0);
  const tiltY = (mousePos.x - 0.5) * (isHovered ? 8 : 0);

  return (
    <div
      ref={wrapperRef}
      className={`orb-wrapper orb-state-${state} ${isHovered ? 'orb-hovered' : ''}`}
      style={{
        '--amp-scale': ampScale,
        '--amp-glow': ampGlow,
        '--tilt-x': `${tiltX}deg`,
        '--tilt-y': `${tiltY}deg`,
        '--ambient-hue': ambient.hue,
        '--ambient-warmth': ambient.warmth,
      } as React.CSSProperties}
      onClick={handleClick}
      onMouseMove={handleMouseMove}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {/* Soft ambient glow - lives outside the sphere so no clipping */}
      <div className="orb-glow" />
      <div className="orb-glow orb-glow-b" />

      {/* Ambient time-of-day tint */}
      <div
        className="orb-ambient-tint"
        style={{
          background: `radial-gradient(circle, hsla(${ambient.hue}, 60%, 50%, ${ambient.warmth}) 0%, transparent 60%)`,
        }}
      />

      {/* The sphere - clips all inner layers to a perfect circle */}
      <div className="orb-sphere">
        <div className="orb-nebula orb-nebula-a" />
        <div className="orb-nebula orb-nebula-b" />
        <div className="orb-nebula orb-nebula-c" />
        <div className="orb-inner-glow" />
        <div className="orb-specular" />

        {/* Click ripple */}
        {ripple && (
          <div
            key={ripple.id}
            className="orb-ripple"
            style={{ left: `${ripple.x}%`, top: `${ripple.y}%` }}
          />
        )}
      </div>

      {/* Rim light */}
      <div className="orb-rim" />

      {/* Amplitude ring - pulses with voice level */}
      {amplitude > 0.01 && (
        <div className="orb-amp-ring" style={{ transform: `translate(-50%, -50%) scale(${ampScale})`, opacity: ampGlow }} />
      )}

      {/* Pairing beacon ring */}
      {pairingColors && pairingColors.length > 0 && (
        <div className="orb-pairing-ring">
          {pairingColors.map((color, i) => (
            <div
              key={i}
              className="orb-pairing-segment"
              style={{
                background: color,
                transform: `rotate(${(i / pairingColors.length) * 360}deg) translateY(-108px)`,
              }}
            />
          ))}
        </div>
      )}

      {label && (
        <div className="orb-label">
          <span className="orb-label-dot" />
          {label}
        </div>
      )}
    </div>
  );
}
