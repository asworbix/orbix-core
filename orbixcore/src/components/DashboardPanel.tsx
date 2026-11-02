import { useState, useEffect } from 'react';
import type { DashboardMetrics, TimeRange } from '../types';
import { getDashboardMetrics } from '../hooks/api';
import './DashboardPanel.css';

interface Props { onClose: () => void }

const RANGE_LABELS: Record<TimeRange, string> = {
  today: 'Today',
  week: '7 Days',
  month: '30 Days',
};

function BarChart({ data, maxH = 80 }: { data: { label: string; count: number; color?: string }[]; maxH?: number }) {
  const max = Math.max(...data.map(d => d.count), 1);
  return (
    <div className="dash-bar-chart">
      {data.map((d, i) => (
        <div key={i} className="dash-bar-col">
          <div
            className="dash-bar"
            style={{
              height: `${(d.count / max) * maxH}px`,
              background: d.color || 'var(--accent)',
            }}
            title={`${d.label}: ${d.count}`}
          />
          <span className="dash-bar-label">{d.label}</span>
        </div>
      ))}
    </div>
  );
}

function DonutChart({ segments }: { segments: { label: string; value: number; color: string }[] }) {
  const total = segments.reduce((s, d) => s + d.value, 0);
  if (total === 0) return <div className="dash-donut-empty">No data</div>;

  let cumulative = 0;
  const size = 80;
  const r = 30;
  const cx = size / 2;
  const cy = size / 2;

  return (
    <div className="dash-donut-wrap">
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {segments.filter(s => s.value > 0).map((seg, i) => {
          const frac = seg.value / total;
          const startAngle = cumulative * 2 * Math.PI - Math.PI / 2;
          cumulative += frac;
          const endAngle = cumulative * 2 * Math.PI - Math.PI / 2;
          const large = frac > 0.5 ? 1 : 0;
          const x1 = cx + r * Math.cos(startAngle);
          const y1 = cy + r * Math.sin(startAngle);
          const x2 = cx + r * Math.cos(endAngle);
          const y2 = cy + r * Math.sin(endAngle);
          return (
            <path
              key={i}
              d={`M ${cx} ${cy} L ${x1} ${y1} A ${r} ${r} 0 ${large} 1 ${x2} ${y2} Z`}
              fill={seg.color}
              opacity={0.85}
            />
          );
        })}
        <circle cx={cx} cy={cy} r={16} fill="var(--bg-primary)" />
        <text x={cx} y={cy + 4} textAnchor="middle" fill="var(--text-primary)" fontSize="11" fontWeight="600">
          {total}
        </text>
      </svg>
      <div className="dash-donut-legend">
        {segments.filter(s => s.value > 0).map((s, i) => (
          <div key={i} className="dash-legend-item">
            <span className="dash-legend-dot" style={{ background: s.color }} />
            <span>{s.label}</span>
            <span className="dash-legend-val">{s.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

const SOURCE_COLORS: Record<string, string> = {
  email: '#7b93ff',
  webhook: '#a855f7',
  filewatch: '#fbbf24',
  manual: '#06b6d4',
  workflow: '#34d399',
};

export default function DashboardPanel({ onClose }: Props) {
  const [metrics, setMetrics] = useState<DashboardMetrics | null>(null);
  const [range, setRange] = useState<TimeRange>('week');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    getDashboardMetrics(range)
      .then(setMetrics)
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [range]);

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="3" width="18" height="18" rx="2"/>
            <path d="M3 9h18M9 21V9"/>
          </svg>
          Dashboard
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      {/* Time range selector */}
      <div className="dash-range-bar">
        {(['today', 'week', 'month'] as TimeRange[]).map(r => (
          <button
            key={r}
            className={range === r ? 'active' : ''}
            onClick={() => setRange(r)}
          >
            {RANGE_LABELS[r]}
          </button>
        ))}
      </div>

      <div className="panel-body">
        {loading && !metrics ? (
          <div className="panel-empty"><p>Loading metrics...</p></div>
        ) : metrics ? (
          <>
            {/* ── Summary Cards ── */}
            <div className="dash-cards">
              <div className="dash-card">
                <span className="dash-card-value">{metrics.summary.total_items}</span>
                <span className="dash-card-label">Total Items</span>
              </div>
              <div className="dash-card">
                <span className="dash-card-value">{metrics.summary.active_workflows}</span>
                <span className="dash-card-label">Workflows</span>
              </div>
              <div className="dash-card">
                <span className="dash-card-value">{metrics.summary.events_today}</span>
                <span className="dash-card-label">Events Today</span>
              </div>
              <div className="dash-card">
                <span className="dash-card-value" style={{ color: metrics.summary.stale_items > 0 ? 'var(--warning)' : undefined }}>
                  {metrics.summary.stale_items}
                </span>
                <span className="dash-card-label">Stale Items</span>
              </div>
              <div className="dash-card">
                <span className="dash-card-value">{metrics.summary.active_schedules}</span>
                <span className="dash-card-label">Schedules</span>
              </div>
            </div>

            {/* ── Pipeline Section ── */}
            <div className="panel-section-header">Pipeline</div>
            {metrics.pipeline.items_per_stage.length > 0 ? (
              <div className="dash-chart-box">
                <span className="dash-chart-label">Items per Stage</span>
                <BarChart
                  data={metrics.pipeline.items_per_stage.map(s => ({
                    label: s.stage_name,
                    count: s.count,
                    color: s.color,
                  }))}
                />
              </div>
            ) : (
              <div className="dash-empty-section">No pipeline data yet</div>
            )}

            {metrics.pipeline.items_by_source.length > 0 && (
              <div className="dash-chart-box">
                <span className="dash-chart-label">Items by Source</span>
                <DonutChart
                  segments={metrics.pipeline.items_by_source.map(s => ({
                    label: s.source,
                    value: s.count,
                    color: SOURCE_COLORS[s.source] || '#6b7280',
                  }))}
                />
              </div>
            )}

            {metrics.pipeline.throughput.length > 0 && (
              <div className="dash-chart-box">
                <span className="dash-chart-label">Throughput (completed/day)</span>
                <BarChart data={metrics.pipeline.throughput} />
              </div>
            )}

            {/* ── Triggers Section ── */}
            <div className="panel-section-header">Triggers</div>
            {metrics.triggers.events_by_day.length > 0 ? (
              <div className="dash-chart-box">
                <span className="dash-chart-label">Events per Day</span>
                <BarChart data={metrics.triggers.events_by_day} />
              </div>
            ) : (
              <div className="dash-empty-section">No trigger events yet</div>
            )}

            {metrics.triggers.events_by_source.length > 0 && (
              <div className="dash-chart-box">
                <span className="dash-chart-label">Events by Source</span>
                <DonutChart
                  segments={metrics.triggers.events_by_source.map(s => ({
                    label: s.source,
                    value: s.count,
                    color: SOURCE_COLORS[s.source] || '#6b7280',
                  }))}
                />
              </div>
            )}

            {metrics.triggers.top_rules.length > 0 && (
              <div className="dash-chart-box">
                <span className="dash-chart-label">Top Rules</span>
                <div className="dash-rank-list">
                  {metrics.triggers.top_rules.map((r, i) => (
                    <div key={i} className="dash-rank-item">
                      <span className="dash-rank-pos">{i + 1}</span>
                      <span className="dash-rank-name">{r.rule_name}</span>
                      <span className="dash-rank-val">{r.count}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div className="dash-inline-stat">
              <span>Failure Rate</span>
              <span style={{ color: metrics.triggers.failure_rate > 0.1 ? 'var(--error)' : 'var(--success)' }}>
                {(metrics.triggers.failure_rate * 100).toFixed(1)}%
              </span>
            </div>

            {/* ── Workflows Section ── */}
            <div className="panel-section-header">Workflows</div>
            {metrics.workflows.total_runs > 0 ? (
              <>
                <div className="dash-cards dash-cards-sm">
                  <div className="dash-card">
                    <span className="dash-card-value">{metrics.workflows.total_runs}</span>
                    <span className="dash-card-label">Runs</span>
                  </div>
                  <div className="dash-card">
                    <span className="dash-card-value" style={{ color: 'var(--success)' }}>{metrics.workflows.success_count}</span>
                    <span className="dash-card-label">Success</span>
                  </div>
                  <div className="dash-card">
                    <span className="dash-card-value" style={{ color: metrics.workflows.failure_count > 0 ? 'var(--error)' : undefined }}>
                      {metrics.workflows.failure_count}
                    </span>
                    <span className="dash-card-label">Failed</span>
                  </div>
                </div>

                {metrics.workflows.runs_by_day.length > 0 && (
                  <div className="dash-chart-box">
                    <span className="dash-chart-label">Runs per Day</span>
                    <BarChart data={metrics.workflows.runs_by_day} />
                  </div>
                )}

                {metrics.workflows.most_used.length > 0 && (
                  <div className="dash-chart-box">
                    <span className="dash-chart-label">Most Used</span>
                    <div className="dash-rank-list">
                      {metrics.workflows.most_used.map((w, i) => (
                        <div key={i} className="dash-rank-item">
                          <span className="dash-rank-pos">{i + 1}</span>
                          <span className="dash-rank-name">{w.name}</span>
                          <span className="dash-rank-val">{w.runs}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </>
            ) : (
              <div className="dash-empty-section">No workflow runs yet</div>
            )}
          </>
        ) : (
          <div className="panel-empty"><p>Failed to load metrics</p></div>
        )}
      </div>
    </div>
  );
}
