import { useEffect, useCallback, useRef } from 'react';
import './BobToast.css';

export interface BobToastItem {
  id: string;
  message: string;
  type: 'info' | 'success' | 'action' | 'error';
  timestamp: number;
  /** Optional callback fired when the user clicks the toast body. */
  onClick?: () => void;
}

interface Props {
  toasts: BobToastItem[];
  onDismiss: (id: string) => void;
}

const ICONS: Record<BobToastItem['type'], string> = {
  info: '\u2139\uFE0F',
  success: '\u2713',
  action: '\u26A1',
  error: '\u2717',
};

const AUTO_DISMISS_MS = 4000;
const MAX_VISIBLE = 3;

function BobToastEntry({
  toast,
  onDismiss,
}: {
  toast: BobToastItem;
  onDismiss: (id: string) => void;
}) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const elRef = useRef<HTMLDivElement>(null);

  const dismiss = useCallback(() => {
    const el = elRef.current;
    if (!el) {
      onDismiss(toast.id);
      return;
    }
    el.classList.add('bob-toast-exit');
    const onEnd = () => {
      el.removeEventListener('animationend', onEnd);
      onDismiss(toast.id);
    };
    el.addEventListener('animationend', onEnd);
  }, [onDismiss, toast.id]);

  useEffect(() => {
    timerRef.current = setTimeout(dismiss, AUTO_DISMISS_MS);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [dismiss]);

  return (
    <div
      ref={elRef}
      className={`bob-toast ${toast.onClick ? 'clickable' : ''}`}
      data-type={toast.type}
      onClick={() => {
        if (toast.onClick) toast.onClick();
        dismiss();
      }}
      role="status"
      aria-live="polite"
    >
      <span className="bob-toast-icon">{ICONS[toast.type]}</span>
      <div className="bob-toast-content">
        <div className="bob-toast-label">Bob</div>
        <div className="bob-toast-message">{toast.message}</div>
      </div>
      <div className="bob-toast-progress" />
    </div>
  );
}

export default function BobToast({ toasts, onDismiss }: Props) {
  const visible = toasts.slice(-MAX_VISIBLE);

  return (
    <div className="bob-toast-container">
      {visible.map((t) => (
        <BobToastEntry key={t.id} toast={t} onDismiss={onDismiss} />
      ))}
    </div>
  );
}
