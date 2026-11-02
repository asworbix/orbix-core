import { useState, useEffect } from 'react';
import type { ActivityEvent, FleetMap, FleetHealthSummary, FleetPreset, ActivitySeverity } from '../types';
import {
  listFleetEvents, getFleetMap, getFleetHealth, listFleetPresets,
} from '../hooks/api';
import './MonitorPanel.css';

interface Props { onClose: () => void }

type Tab = 'feed' | 'map' | 'health' | 'presets';

export default function MonitorPanel({ onClose }: Props) {
  const [tab, setTab] = useState<Tab>('feed');
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [fleetMap, setFleetMap] = useState<FleetMap | null>(null);
  const [health, setHealth] = useState<FleetHealthSummary | null>(null);
  const [presets, setPresets] = useState<FleetPreset[]>([]);
  const [severityFilter, setSeverityFilter] = useState<string | null>(null);
  const [selectedPreset, setSelectedPreset] = useState<FleetPreset | null>(null);

  useEffect(() => {
    listFleetEvents(undefined, undefined, undefined, severityFilter ?? undefined, 50).then(setEvents).catch(() => {});
  }, [severityFilter]);

  useEffect(() => {
    if (tab === 'map') getFleetMap().then(setFleetMap).catch(() => {});
    if (tab === 'health') getFleetHealth().then(setHealth).catch(() => {});
    if (tab === 'presets') listFleetPresets().then(setPresets).catch(() => {});
  }, [tab]);

  const formatRelative = (iso: string) => {
    const diff = Date.now() - new Date(iso).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'now';
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h`;
    return `${Math.floor(hrs / 24)}d`;
  };

  const severities: ActivitySeverity[] = ['Info', 'Success', 'Warning', 'Error'];

  // Preset detail view
  if (selectedPreset) {
    return (
      <div className="panel">
        <div className="panel-header">
          <h2 className="panel-title">{selectedPreset.name}</h2>
          <div className="panel-header-actions">
            <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        </div>
        <div className="panel-body">
          <div className="fleet-detail-back" onClick={() => setSelectedPreset(null)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="15 18 9 12 15 6"/>
            </svg>
            Back to Presets
          </div>

          <p style={{ fontSize: 12, color: 'var(--text-secondary)', margin: '0 0 14px' }}>{selectedPreset.description}</p>

          <div className="monitor-section-header">Roles ({selectedPreset.roles.length})</div>
          {selectedPreset.roles.map((r, i) => (
            <div key={i} className="monitor-device-health">
              <div className="monitor-device-health-name">{r.name}</div>
              <div style={{ flex: 1, fontSize: 11, color: 'var(--text-dim)' }}>{r.description}</div>
              <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>Max: {r.max_workload}</span>
            </div>
          ))}

          <div className="monitor-section-header">Routes ({selectedPreset.routes.length})</div>
          {selectedPreset.routes.map((r, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 4px', fontSize: 12 }}>
              <span className="monitor-preset-role">{r.from_role}</span>
              <span style={{ color: 'var(--text-dim)' }}>→</span>
              <span className="monitor-preset-role">{r.to_role}</span>
              <span style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 'auto' }}>{r.condition} · {r.strategy}</span>
            </div>
          ))}

          <div style={{ paddingTop: 14 }}>
            <button className="panel-btn accent" style={{ width: '100%' }}>
              Apply Preset to Fleet
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="3" width="18" height="18" rx="2"/>
            <path d="M3 9h18"/>
            <path d="M9 21V9"/>
          </svg>
          Fleet Monitor
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="monitor-tabs">
        <button className={tab === 'feed' ? 'active' : ''} onClick={() => setTab('feed')}>
          Activity {events.length > 0 && <span className="monitor-tab-count">{events.length}</span>}
        </button>
        <button className={tab === 'map' ? 'active' : ''} onClick={() => setTab('map')}>
          Map
        </button>
        <button className={tab === 'health' ? 'active' : ''} onClick={() => setTab('health')}>
          Health
        </button>
        <button className={tab === 'presets' ? 'active' : ''} onClick={() => setTab('presets')}>
          Presets
        </button>
      </div>

      <div className="panel-body">
        {tab === 'feed' && (
          <>
            <div className="monitor-filters">
              <button
                className={`monitor-filter-btn ${!severityFilter ? 'active' : ''}`}
                onClick={() => setSeverityFilter(null)}
              >All</button>
              {severities.map(s => (
                <button
                  key={s}
                  className={`monitor-filter-btn ${severityFilter === s ? 'active' : ''}`}
                  onClick={() => setSeverityFilter(severityFilter === s ? null : s)}
                >{s}</button>
              ))}
            </div>

            {events.length === 0 ? (
              <div className="panel-empty">
                <div className="panel-empty-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                    <rect x="3" y="3" width="18" height="18" rx="2"/>
                    <path d="M3 9h18"/><path d="M9 21V9"/>
                  </svg>
                </div>
                <p>No fleet activity</p>
                <p className="panel-empty-sub">Events appear here as devices process work items, trigger routes, and communicate.</p>
              </div>
            ) : (
              events.map(e => (
                <div key={e.id} className="monitor-event">
                  <div className={`monitor-event-dot ${e.severity}`}/>
                  <div className="monitor-event-body">
                    <div className="monitor-event-title">{e.title}</div>
                    <div className="monitor-event-details">{e.details}</div>
                    <div className="monitor-event-meta">
                      {e.device_name && <span className="monitor-event-badge">{e.device_name}</span>}
                      {e.role && <span>{e.role}</span>}
                      <span>{e.event_type}</span>
                      <span>{formatRelative(e.timestamp)}</span>
                    </div>
                  </div>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'map' && (
          <>
            <div className="monitor-map-container">
              {fleetMap && (
                <svg width="100%" height="100%" viewBox="0 0 400 300">
                  {/* Edges */}
                  {fleetMap.edges.map((edge, i) => {
                    const fromNode = fleetMap.nodes.find(n => n.role === edge.from_role);
                    const toNode = fleetMap.nodes.find(n => n.role === edge.to_role);
                    if (!fromNode || !toNode) return null;
                    return (
                      <g key={`e-${i}`}>
                        <line
                          x1={fromNode.x} y1={fromNode.y}
                          x2={toNode.x} y2={toNode.y}
                          stroke={edge.active ? 'rgba(99,102,241,0.5)' : 'rgba(255,255,255,0.08)'}
                          strokeWidth={edge.active ? 2 : 1}
                          strokeDasharray={edge.active ? '' : '4 4'}
                        />
                        {edge.items_routed > 0 && (
                          <text
                            x={(fromNode.x + toNode.x) / 2}
                            y={(fromNode.y + toNode.y) / 2 - 6}
                            textAnchor="middle"
                            fill="rgba(255,255,255,0.3)"
                            fontSize="9"
                          >
                            {edge.items_routed} routed
                          </text>
                        )}
                      </g>
                    );
                  })}
                  {/* Nodes */}
                  {fleetMap.nodes.map(node => {
                    const fill = node.status === 'Online' ? 'rgba(52,211,153,0.2)'
                      : node.status === 'Busy' ? 'rgba(99,102,241,0.2)'
                      : node.status === 'Error' ? 'rgba(248,113,113,0.2)'
                      : 'rgba(255,255,255,0.06)';
                    const stroke = node.status === 'Online' ? '#34d399'
                      : node.status === 'Busy' ? '#6366f1'
                      : node.status === 'Error' ? '#f87171'
                      : 'rgba(255,255,255,0.15)';
                    return (
                      <g key={node.device_id}>
                        <circle cx={node.x} cy={node.y} r="28" fill={fill} stroke={stroke} strokeWidth="1.5"/>
                        <text x={node.x} y={node.y - 4} textAnchor="middle" fill="white" fontSize="10" fontWeight="600">
                          {node.name.slice(0, 8)}
                        </text>
                        <text x={node.x} y={node.y + 8} textAnchor="middle" fill="rgba(255,255,255,0.5)" fontSize="8">
                          {node.role.slice(0, 12)}
                        </text>
                        {node.active_items > 0 && (
                          <>
                            <circle cx={node.x + 20} cy={node.y - 20} r="9" fill="#6366f1"/>
                            <text x={node.x + 20} y={node.y - 17} textAnchor="middle" fill="white" fontSize="9" fontWeight="700">
                              {node.active_items}
                            </text>
                          </>
                        )}
                      </g>
                    );
                  })}
                </svg>
              )}
              {(!fleetMap || fleetMap.nodes.length === 0) && (
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-dim)', fontSize: 12 }}>
                  No devices to map
                </div>
              )}
            </div>
            <div className="monitor-map-legend">
              <div className="monitor-map-legend-item"><div className="monitor-map-legend-dot" style={{ background: '#34d399' }}/> Online</div>
              <div className="monitor-map-legend-item"><div className="monitor-map-legend-dot" style={{ background: '#6366f1' }}/> Busy</div>
              <div className="monitor-map-legend-item"><div className="monitor-map-legend-dot" style={{ background: '#f87171' }}/> Error</div>
              <div className="monitor-map-legend-item"><div className="monitor-map-legend-dot" style={{ background: 'rgba(255,255,255,0.15)' }}/> Offline</div>
            </div>
          </>
        )}

        {tab === 'health' && health && (
          <>
            <div className="monitor-health-summary">
              <div className="monitor-health-card">
                <div className="monitor-health-value">{health.total_throughput_today}</div>
                <div className="monitor-health-label">Processed Today</div>
              </div>
              <div className="monitor-health-card">
                <div className="monitor-health-value">{health.avg_items_per_hour.toFixed(1)}</div>
                <div className="monitor-health-label">Items/Hour</div>
              </div>
              <div className="monitor-health-card">
                <div className="monitor-health-value">{health.avg_processing_mins.toFixed(0)}m</div>
                <div className="monitor-health-label">Avg Processing</div>
              </div>
            </div>

            {health.bottleneck_role && (
              <div style={{
                padding: '10px 14px', marginBottom: 12, borderRadius: 'var(--radius-md)',
                background: 'rgba(248,113,113,0.06)', border: '1px solid rgba(248,113,113,0.15)',
                fontSize: 12, color: 'var(--text-secondary)',
              }}>
                <strong style={{ color: 'var(--error)' }}>Bottleneck detected:</strong>{' '}
                {health.bottleneck_device} ({health.bottleneck_role})
              </div>
            )}

            <div className="monitor-section-header">Device Health</div>
            {health.devices.length === 0 ? (
              <div style={{ fontSize: 12, color: 'var(--text-dim)', padding: '8px 0' }}>No devices in fleet</div>
            ) : (
              health.devices.map(d => (
                <div key={d.device_id} className="monitor-device-health">
                  <div className="monitor-device-health-name">
                    {d.device_name}
                    {d.is_bottleneck && <span className="monitor-bottleneck-tag" style={{ marginLeft: 6 }}>Bottleneck</span>}
                  </div>
                  <div className="monitor-device-health-role">{d.role}</div>
                  <div className="monitor-device-health-stats">
                    <span>{d.items_processed_today} done</span>
                    <span>{d.active_items} active</span>
                    <span>{d.failed_items} fail</span>
                  </div>
                  <div className="monitor-device-health-bar">
                    <div
                      className={`monitor-device-health-bar-fill ${d.is_bottleneck ? 'bottleneck' : ''}`}
                      style={{ width: `${Math.min(d.uptime_pct, 100)}%` }}
                    />
                  </div>
                  <span style={{ fontSize: 10, color: 'var(--text-muted)', width: 36, textAlign: 'right' }}>{d.uptime_pct.toFixed(0)}%</span>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'presets' && (
          <>
            {presets.length === 0 ? (
              <div className="panel-empty">
                <p>No fleet presets</p>
              </div>
            ) : (
              presets.map(p => (
                <div key={p.id} className="monitor-preset-card" onClick={() => setSelectedPreset(p)}>
                  <div>
                    <span className="monitor-preset-category">{p.category}</span>
                    <div className="monitor-preset-name">{p.name}</div>
                  </div>
                  <div className="monitor-preset-desc">{p.description}</div>
                  <div className="monitor-preset-roles">
                    {p.roles.map((r, i) => (
                      <span key={i} className="monitor-preset-role">{r.name}</span>
                    ))}
                  </div>
                  <div className="monitor-preset-routes">
                    {p.routes.map((r, i) => (
                      <span key={i}>
                        {i > 0 && <span style={{ margin: '0 2px' }}>·</span>}
                        {r.from_role} → {r.to_role}
                      </span>
                    ))}
                  </div>
                </div>
              ))
            )}
          </>
        )}
      </div>
    </div>
  );
}
