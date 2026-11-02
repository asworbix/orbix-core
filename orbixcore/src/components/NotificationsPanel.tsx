import { useState, useEffect } from 'react';
import type { AppNotification, NotificationPreferences, PanelType } from '../types';
import {
  listNotifications, markNotificationRead, markAllNotificationsRead,
  dismissNotification, dismissAllNotifications,
  getNotificationPreferences, updateNotificationPreferences,
} from '../hooks/api';
import './NotificationsPanel.css';

interface Props {
  onClose: () => void;
  onNavigate?: (panel: PanelType) => void;
}

const LEVEL_ICONS: Record<string, string> = {
  info: 'i',
  success: '+',
  warning: '!',
  error: 'x',
};

const SOURCE_LABELS: Record<string, string> = {
  trigger: 'Trigger',
  pipeline: 'Pipeline',
  workflow: 'Workflow',
  schedule: 'Schedule',
  suggestion: 'Suggestion',
  system: 'System',
};

export default function NotificationsPanel({ onClose, onNavigate }: Props) {
  const [tab, setTab] = useState<'all' | 'unread' | 'settings'>('all');
  const [notifs, setNotifs] = useState<AppNotification[]>([]);
  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);

  useEffect(() => {
    listNotifications().then(setNotifs).catch(() => {});
    getNotificationPreferences().then(setPrefs).catch(() => {});
  }, []);

  const displayed = tab === 'unread' ? notifs.filter(n => !n.read) : notifs;
  const unreadCount = notifs.filter(n => !n.read).length;

  const handleRead = async (id: string) => {
    await markNotificationRead(id).catch(() => {});
    setNotifs(prev => prev.map(n => n.id === id ? { ...n, read: true } : n));
  };

  const handleDismiss = async (id: string) => {
    await dismissNotification(id).catch(() => {});
    setNotifs(prev => prev.filter(n => n.id !== id));
  };

  const handleMarkAllRead = async () => {
    await markAllNotificationsRead().catch(() => {});
    setNotifs(prev => prev.map(n => ({ ...n, read: true })));
  };

  const handleDismissAll = async () => {
    await dismissAllNotifications().catch(() => {});
    setNotifs([]);
  };

  const handleClick = (n: AppNotification) => {
    if (!n.read) handleRead(n.id);
    if (n.action_panel && onNavigate) {
      onNavigate(n.action_panel as PanelType);
    }
  };

  const handleToggleSource = async (key: keyof NotificationPreferences['sources']) => {
    if (!prefs) return;
    const updated = { ...prefs, sources: { ...prefs.sources, [key]: !prefs.sources[key] } };
    setPrefs(updated);
    await updateNotificationPreferences(updated).catch(() => {});
  };

  const handleToggle = async (field: 'enabled' | 'do_not_disturb' | 'sound_enabled' | 'desktop_enabled') => {
    if (!prefs) return;
    const updated = { ...prefs, [field]: !prefs[field] };
    setPrefs(updated);
    await updateNotificationPreferences(updated).catch(() => {});
  };

  const formatTime = (iso: string) =>
    new Date(iso).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/>
            <path d="M13.73 21a2 2 0 0 1-3.46 0"/>
          </svg>
          Notifications
          {unreadCount > 0 && <span className="sugg-badge">{unreadCount}</span>}
        </h2>
        <div className="panel-header-actions">
          {tab !== 'settings' && notifs.length > 0 && (
            <>
              <button className="link-btn" onClick={handleMarkAllRead}>Read all</button>
              <button className="link-btn" onClick={handleDismissAll}>Clear</button>
            </>
          )}
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="notif-tabs">
        <button className={tab === 'all' ? 'active' : ''} onClick={() => setTab('all')}>
          All {notifs.length > 0 && <span className="notif-tab-count">{notifs.length}</span>}
        </button>
        <button className={tab === 'unread' ? 'active' : ''} onClick={() => setTab('unread')}>
          Unread {unreadCount > 0 && <span className="notif-tab-count">{unreadCount}</span>}
        </button>
        <button className={tab === 'settings' ? 'active' : ''} onClick={() => setTab('settings')}>
          Settings
        </button>
      </div>

      <div className="panel-body">
        {tab !== 'settings' && (
          <>
            {displayed.length === 0 ? (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                    <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/>
                    <path d="M13.73 21a2 2 0 0 1-3.46 0"/>
                  </svg>
                </div>
                <p>{tab === 'unread' ? 'No unread notifications' : 'No notifications'}</p>
              </div>
            ) : (
              displayed.map(n => (
                <div
                  key={n.id}
                  className={`notif-item ${n.read ? '' : 'unread'} animate-fade-up`}
                  onClick={() => handleClick(n)}
                >
                  <span className="notif-icon">{LEVEL_ICONS[n.level]}</span>
                  <div className="notif-content">
                    <div className="notif-title">{n.title}</div>
                    <div className="notif-message">{n.message}</div>
                    <div className="notif-meta">
                      <span className="notif-source">{SOURCE_LABELS[n.source]}</span>
                      <span className="notif-time">{formatTime(n.created_at)}</span>
                    </div>
                  </div>
                  <button
                    className="notif-dismiss"
                    onClick={e => { e.stopPropagation(); handleDismiss(n.id); }}
                    title="Dismiss"
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                    </svg>
                  </button>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'settings' && prefs && (
          <div className="notif-settings">
            <div className="sugg-setting-row">
              <div>
                <span className="sugg-setting-label">Notifications</span>
                <span className="sugg-setting-sub">Enable all notifications</span>
              </div>
              <button className={`toggle-switch ${prefs.enabled ? 'on' : ''}`} role="switch" aria-checked={prefs.enabled} onClick={() => handleToggle('enabled')}>
                <div className="toggle-thumb" />
              </button>
            </div>
            <div className="sugg-setting-row">
              <div>
                <span className="sugg-setting-label">Do Not Disturb</span>
                <span className="sugg-setting-sub">Silence all alerts</span>
              </div>
              <button className={`toggle-switch ${prefs.do_not_disturb ? 'on' : ''}`} role="switch" aria-checked={prefs.do_not_disturb} onClick={() => handleToggle('do_not_disturb')}>
                <div className="toggle-thumb" />
              </button>
            </div>
            <div className="sugg-setting-row">
              <span className="sugg-setting-label">Sound</span>
              <button className={`toggle-switch ${prefs.sound_enabled ? 'on' : ''}`} role="switch" aria-checked={prefs.sound_enabled} onClick={() => handleToggle('sound_enabled')}>
                <div className="toggle-thumb" />
              </button>
            </div>
            <div className="sugg-setting-row">
              <span className="sugg-setting-label">Desktop Alerts</span>
              <button className={`toggle-switch ${prefs.desktop_enabled ? 'on' : ''}`} role="switch" aria-checked={prefs.desktop_enabled} onClick={() => handleToggle('desktop_enabled')}>
                <div className="toggle-thumb" />
              </button>
            </div>

            <div className="panel-section-header">Sources</div>
            {(Object.keys(prefs.sources) as (keyof typeof prefs.sources)[]).map(key => (
              <div key={key} className="sugg-setting-row">
                <span className="sugg-setting-label">{SOURCE_LABELS[key]}</span>
                <button className={`toggle-switch ${prefs.sources[key] ? 'on' : ''}`} role="switch" aria-checked={prefs.sources[key]} onClick={() => handleToggleSource(key)}>
                  <div className="toggle-thumb" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
