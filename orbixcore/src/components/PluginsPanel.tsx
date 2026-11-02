import { useState, useEffect } from 'react';
import type { AppPlugin } from '../types';
import {
  listPlugins, enablePlugin, disablePlugin, uninstallPlugin,
  grantPluginPermission,
} from '../hooks/api';
import './PluginsPanel.css';

interface Props { onClose: () => void }

const TYPE_ICONS: Record<string, string> = {
  tool: 'T',
  connector: 'C',
  workflow_step: 'W',
  widget: 'U',
};

const STATUS_COLORS: Record<string, string> = {
  active: 'var(--success)',
  installed: 'var(--warning)',
  disabled: 'var(--text-dim)',
  error: 'var(--error)',
};

export default function PluginsPanel({ onClose }: Props) {
  const [plugins, setPlugins] = useState<AppPlugin[]>([]);
  const [selected, setSelected] = useState<AppPlugin | null>(null);

  useEffect(() => {
    listPlugins().then(setPlugins).catch(() => {});
  }, []);

  const handleToggle = async (plugin: AppPlugin) => {
    const fn = plugin.status === 'active' ? disablePlugin : enablePlugin;
    const updated = await fn(plugin.id).catch(() => null);
    if (updated) {
      setPlugins(prev => prev.map(p => p.id === updated.id ? updated : p));
      if (selected?.id === updated.id) setSelected(updated);
    }
  };

  const handleUninstall = async (id: string) => {
    await uninstallPlugin(id).catch(() => {});
    setPlugins(prev => prev.filter(p => p.id !== id));
    if (selected?.id === id) setSelected(null);
  };

  const handleGrantPermission = async (pluginId: string, permId: string) => {
    await grantPluginPermission(pluginId, permId).catch(() => {});
    setPlugins(prev => prev.map(p => {
      if (p.id === pluginId) {
        return {
          ...p,
          permissions: p.permissions.map(pm =>
            pm.id === permId ? { ...pm, granted: true } : pm
          ),
        };
      }
      return p;
    }));
  };

  const formatDate = (iso: string) =>
    new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });

  // Detail view
  if (selected) {
    return (
      <div className="panel">
        <div className="panel-header">
          <h2 className="panel-title">
            <button className="kb-back-btn" onClick={() => setSelected(null)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="15 18 9 12 15 6"/>
              </svg>
            </button>
            {selected.manifest.name}
          </h2>
          <div className="panel-header-actions">
            <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        </div>
        <div className="panel-body">
          <div className="plug-detail-meta">
            <span>{TYPE_ICONS[selected.manifest.plugin_type]} {selected.manifest.plugin_type}</span>
            <span>v{selected.manifest.version}</span>
            <span>by {selected.manifest.author}</span>
          </div>
          <p className="plug-detail-desc">{selected.manifest.description}</p>

          <div className="plug-detail-actions">
            <button
              className={`panel-btn ${selected.status === 'active' ? '' : 'accent'}`}
              onClick={() => handleToggle(selected)}
            >
              {selected.status === 'active' ? 'Disable' : 'Enable'}
            </button>
            <button className="link-btn danger" onClick={() => handleUninstall(selected.id)}>
              Uninstall
            </button>
          </div>

          {selected.error && (
            <div className="plug-error">Error: {selected.error}</div>
          )}

          {/* Permissions */}
          <div className="panel-section-header">Permissions</div>
          {selected.permissions.length === 0 ? (
            <div className="plug-no-perms">No permissions required</div>
          ) : (
            selected.permissions.map(pm => (
              <div key={pm.id} className="sugg-setting-row">
                <span className="sugg-setting-label">{pm.label}</span>
                {pm.granted ? (
                  <span className="plug-perm-granted">Granted</span>
                ) : (
                  <button className="link-btn" onClick={() => handleGrantPermission(selected.id, pm.id)}>
                    Grant
                  </button>
                )}
              </div>
            ))
          )}

          <div className="plug-detail-info">
            <span>Installed: {formatDate(selected.installed_at)}</span>
            <span>Path: {selected.install_path}</span>
          </div>
        </div>
      </div>
    );
  }

  // List view
  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M12 2L2 7l10 5 10-5-10-5z"/>
            <path d="M2 17l10 5 10-5"/>
            <path d="M2 12l10 5 10-5"/>
          </svg>
          Plugins
          {plugins.length > 0 && <span className="sugg-badge">{plugins.length}</span>}
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="panel-body">
        {plugins.length === 0 ? (
          <div className="panel-empty">
            <div className="panel-empty-icon">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                <path d="M12 2L2 7l10 5 10-5-10-5z"/>
                <path d="M2 17l10 5 10-5"/>
                <path d="M2 12l10 5 10-5"/>
              </svg>
            </div>
            <p>No plugins installed</p>
            <p className="panel-empty-sub">Install plugins to extend Orbix Core with new tools, connectors, and workflow steps.</p>
          </div>
        ) : (
          plugins.map(p => (
            <div key={p.id} className="plug-card" onClick={() => setSelected(p)}>
              <div className="plug-card-top">
                <span className="plug-card-icon">{TYPE_ICONS[p.manifest.plugin_type]}</span>
                <div className="plug-card-info">
                  <span className="plug-card-name">{p.manifest.name}</span>
                  <span className="plug-card-author">{p.manifest.author} &middot; v{p.manifest.version}</span>
                </div>
                <span className="plug-status-dot" style={{ background: STATUS_COLORS[p.status] }} title={p.status} />
              </div>
              <p className="plug-card-desc">{p.manifest.description}</p>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
