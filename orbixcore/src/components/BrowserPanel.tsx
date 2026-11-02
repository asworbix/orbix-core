import { useState, useEffect } from 'react';
import type { BrowserSession } from '../types';
import {
  browserNavigate, browserScreenshot, browserGetContent,
  browserGetSession, browserClose, onBrowserStatusChange
} from '../hooks/api';
import './Panel.css';
import './BrowserPanel.css';

interface Props {
  onClose: () => void;
}

const STATUS_LABELS: Record<string, { label: string; color: string }> = {
  idle: { label: 'Idle', color: '#fbbf24' },
  navigating: { label: 'Navigating', color: '#6c8fff' },
  ready: { label: 'Ready', color: '#34d399' },
  error: { label: 'Error', color: '#f87171' },
  closed: { label: 'Closed', color: 'var(--text-dim)' },
};

export default function BrowserPanel({ onClose }: Props) {
  const [session, setSession] = useState<BrowserSession>({
    active: false,
    current_url: '',
    page_title: '',
    status: 'closed',
  });
  const [urlInput, setUrlInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [screenshot, setScreenshot] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    browserGetSession().then(setSession).catch(() => {});
    const unsub = onBrowserStatusChange(setSession);
    return () => { unsub.then(fn => fn()); };
  }, []);

  const handleNavigate = async () => {
    const url = urlInput.trim();
    if (!url) return;
    setLoading(true);
    setError('');
    setScreenshot(null);
    setContent(null);
    try {
      await browserNavigate(url.startsWith('http') ? url : `https://${url}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleScreenshot = async () => {
    setLoading(true);
    setError('');
    try {
      const result = await browserScreenshot();
      if (result.success) {
        setScreenshot(result.data);
        setContent(null);
      } else {
        setError(result.error || 'Screenshot failed');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleGetContent = async () => {
    setLoading(true);
    setError('');
    try {
      const result = await browserGetContent();
      if (result.success) {
        setContent(result.data);
        setScreenshot(null);
      } else {
        setError(result.error || 'Content extraction failed');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleClose = async () => {
    await browserClose().catch(() => {});
    setScreenshot(null);
    setContent(null);
    setUrlInput('');
  };

  const statusInfo = STATUS_LABELS[session.status] || STATUS_LABELS.closed;

  return (
    <div className="panel">
      <div className="panel-header">
        <div className="panel-title">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
            <line x1="8" y1="21" x2="16" y2="21"/>
            <line x1="12" y1="17" x2="12" y2="21"/>
          </svg>
          Browser
        </div>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body">
        {/* Status */}
        <div className="browser-status-bar">
          <span className="browser-status-dot" style={{ background: statusInfo.color }} />
          <span className="browser-status-label">{statusInfo.label}</span>
          {session.active && (
            <button className="link-btn danger browser-close-btn" onClick={handleClose}>
              Close Browser
            </button>
          )}
        </div>

        {/* URL Bar */}
        <div className="browser-url-bar">
          <input
            className="form-input browser-url-input"
            placeholder="Enter URL..."
            value={urlInput}
            onChange={e => setUrlInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleNavigate()}
            disabled={loading}
          />
          <button
            className="panel-btn accent"
            onClick={handleNavigate}
            disabled={loading || !urlInput.trim()}
          >
            {loading ? (
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="spin">
                <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
              </svg>
            ) : (
              'Go'
            )}
          </button>
        </div>

        {/* Session Info */}
        {session.active && session.current_url && (
          <div className="browser-session-info">
            {session.page_title && (
              <div className="browser-page-title">{session.page_title}</div>
            )}
            <div className="browser-current-url">{session.current_url}</div>
          </div>
        )}

        {/* Quick Actions */}
        {session.active && (
          <div className="browser-actions">
            <button className="panel-btn" onClick={handleScreenshot} disabled={loading}>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="3" y="3" width="18" height="18" rx="2"/>
                <circle cx="8.5" cy="8.5" r="1.5"/>
                <polyline points="21 15 16 10 5 21"/>
              </svg>
              Screenshot
            </button>
            <button className="panel-btn" onClick={handleGetContent} disabled={loading}>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                <polyline points="14 2 14 8 20 8"/>
                <line x1="16" y1="13" x2="8" y2="13"/>
                <line x1="16" y1="17" x2="8" y2="17"/>
              </svg>
              Get Content
            </button>
          </div>
        )}

        {error && <div className="form-error">{error}</div>}

        {/* Screenshot Preview */}
        {screenshot && (
          <div className="browser-preview">
            <div className="panel-section-header">Screenshot</div>
            <img
              src={`data:image/png;base64,${screenshot}`}
              alt="Browser screenshot"
              className="browser-screenshot-img"
            />
          </div>
        )}

        {/* Content Preview */}
        {content && (
          <div className="browser-preview">
            <div className="panel-section-header">Page Content</div>
            <div className="browser-content-text">{content}</div>
          </div>
        )}

        {/* Empty state */}
        {!session.active && !error && (
          <div className="panel-empty">
            <div className="panel-empty-icon">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
                <line x1="8" y1="21" x2="16" y2="21"/>
                <line x1="12" y1="17" x2="12" y2="21"/>
              </svg>
            </div>
            <p>Headless browser</p>
            <p className="panel-empty-sub">Navigate to a URL to start a browser session. The AI can browse, click, type, and extract content.</p>
          </div>
        )}
      </div>
    </div>
  );
}
