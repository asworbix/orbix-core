import { useState, useEffect } from 'react';
import type { AuditEntry, AuditSeverity, ComplianceRule, ComplianceViolation, ComplianceSummary } from '../types';
import {
  searchAuditLog, listComplianceRules, toggleComplianceRule,
  listComplianceViolations, resolveComplianceViolation, getComplianceSummary,
} from '../hooks/api';
import './AuditPanel.css';

interface Props { onClose: () => void }

type Tab = 'log' | 'compliance' | 'violations';

const scoreClass = (s: number) => s >= 90 ? 'excellent' : s >= 70 ? 'good' : s >= 50 ? 'fair' : 'poor';

const SEVERITY_FILTERS: AuditSeverity[] = ['Info', 'Warning', 'Critical'];
const CATEGORY_FILTERS = ['Device', 'WorkItem', 'Routing', 'Playbook', 'Performance', 'Security', 'System'];

function timeAgo(ts: string): string {
  const diff = Date.now() - new Date(ts).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

export default function AuditPanel({ onClose }: Props) {
  const [tab, setTab] = useState<Tab>('log');
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [rules, setRules] = useState<ComplianceRule[]>([]);
  const [violations, setViolations] = useState<ComplianceViolation[]>([]);
  const [summary, setSummary] = useState<ComplianceSummary | null>(null);
  const [sevFilter, setSevFilter] = useState<string | null>(null);
  const [catFilter, setCatFilter] = useState<string | null>(null);

  useEffect(() => {
    searchAuditLog(undefined, catFilter ?? undefined, sevFilter ?? undefined)
      .then(setEntries).catch(() => {});
  }, [sevFilter, catFilter]);

  useEffect(() => {
    if (tab === 'compliance') {
      listComplianceRules().then(setRules).catch(() => {});
      getComplianceSummary().then(setSummary).catch(() => {});
    }
    if (tab === 'violations') {
      listComplianceViolations(true).then(setViolations).catch(() => {});
    }
  }, [tab]);

  const handleToggleRule = async (id: string) => {
    const updated = await toggleComplianceRule(id).catch(() => null);
    if (updated) setRules(prev => prev.map(r => r.id === id ? updated : r));
  };

  const handleResolve = async (id: string) => {
    const ok = await resolveComplianceViolation(id).catch(() => false);
    if (ok) setViolations(prev => prev.filter(v => v.id !== id));
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <h2 className="panel-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0 1 12 2.944a11.955 11.955 0 0 1-8.618 3.04A12.02 12.02 0 0 0 3 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
          </svg>
          Audit & Compliance
        </h2>
        <div className="panel-header-actions">
          <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <div className="audit-tabs">
        <button className={tab === 'log' ? 'active' : ''} onClick={() => setTab('log')}>Audit Log</button>
        <button className={tab === 'compliance' ? 'active' : ''} onClick={() => setTab('compliance')}>Compliance</button>
        <button className={tab === 'violations' ? 'active' : ''} onClick={() => setTab('violations')}>Violations</button>
      </div>

      <div className="panel-body">
        {tab === 'log' && (
          <>
            <div className="audit-filters">
              {SEVERITY_FILTERS.map(s => (
                <button
                  key={s}
                  className={`audit-filter-btn ${sevFilter === s ? 'active' : ''}`}
                  onClick={() => setSevFilter(sevFilter === s ? null : s)}
                >
                  {s}
                </button>
              ))}
              <span style={{ width: 1, background: 'var(--border)', margin: '0 2px' }} />
              {CATEGORY_FILTERS.map(c => (
                <button
                  key={c}
                  className={`audit-filter-btn ${catFilter === c ? 'active' : ''}`}
                  onClick={() => setCatFilter(catFilter === c ? null : c)}
                >
                  {c}
                </button>
              ))}
            </div>

            {entries.length === 0 ? (
              <div className="panel-empty">
                <p>No audit entries yet</p>
                <p className="panel-empty-sub">Actions across your fleet will be logged here automatically.</p>
              </div>
            ) : (
              entries.map(e => (
                <div key={e.id} className="audit-entry">
                  <div className="audit-entry-header">
                    <div className={`audit-severity ${e.severity}`} />
                    <span className="audit-action">{e.action}</span>
                    <span className="audit-category-badge">{e.category}</span>
                    <span className="audit-time">{timeAgo(e.timestamp)}</span>
                  </div>
                  <div className="audit-details">{e.details}</div>
                  <div className="audit-meta">
                    <span>Actor: {e.actor}</span>
                    {e.target_name && <span>Target: {e.target_name}</span>}
                    <span>{e.target_type}</span>
                  </div>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'compliance' && (
          <>
            {summary && (
              <>
                <div className="compliance-fleet-score">
                  <div className={`compliance-fleet-score-value ${scoreClass(summary.fleet_compliance_score)}`}>
                    {summary.fleet_compliance_score.toFixed(0)}
                  </div>
                  <div className="compliance-fleet-score-label">Fleet Compliance Score</div>
                </div>

                <div className="compliance-stats">
                  <div className="compliance-stat">
                    <div className="compliance-stat-value">{summary.total_rules}</div>
                    <div className="compliance-stat-label">Rules</div>
                  </div>
                  <div className="compliance-stat">
                    <div className="compliance-stat-value" style={{ color: 'var(--success)' }}>{summary.rules_passing}</div>
                    <div className="compliance-stat-label">Passing</div>
                  </div>
                  <div className="compliance-stat">
                    <div className="compliance-stat-value" style={{ color: 'var(--error)' }}>{summary.active_violations}</div>
                    <div className="compliance-stat-label">Violations</div>
                  </div>
                </div>

                {summary.devices.length > 0 && (
                  <>
                    <div className="perf-section-header">Device Compliance</div>
                    {summary.devices.map(d => (
                      <div key={d.device_id} className="device-compliance">
                        <div className="device-compliance-header">
                          <span className="device-compliance-name">{d.device_name}</span>
                          <span className={`device-compliance-score ${scoreClass(d.compliance_score)}`}>
                            {d.compliance_score.toFixed(0)}%
                          </span>
                        </div>
                        <div className="device-compliance-bar">
                          <div
                            className={`device-compliance-fill ${scoreClass(d.compliance_score)}`}
                            style={{ width: `${Math.min(d.compliance_score, 100)}%` }}
                          />
                        </div>
                        <div className="device-compliance-meta">
                          <span>{d.role}</span>
                          <span style={{ color: 'var(--success)' }}>{d.rules_passed} passed</span>
                          {d.rules_failed > 0 && <span style={{ color: 'var(--error)' }}>{d.rules_failed} failed</span>}
                        </div>
                      </div>
                    ))}
                  </>
                )}
              </>
            )}

            <div className="perf-section-header">Rules</div>
            {rules.length === 0 ? (
              <div className="panel-empty">
                <p>No compliance rules</p>
              </div>
            ) : (
              rules.map(r => (
                <div key={r.id} className="compliance-rule">
                  <div
                    className="toggle-track"
                    data-on={r.enabled}
                    onClick={() => handleToggleRule(r.id)}
                    style={{ cursor: 'pointer' }}
                  >
                    <div className="toggle-thumb" />
                  </div>
                  <div className="compliance-rule-info">
                    <div className="compliance-rule-name">{r.name}</div>
                    <div className="compliance-rule-desc">{r.description}</div>
                    <div className="compliance-rule-category">{r.category}</div>
                  </div>
                </div>
              ))
            )}
          </>
        )}

        {tab === 'violations' && (
          <>
            {violations.length === 0 ? (
              <div className="panel-empty">
                <p>No active violations</p>
                <p className="panel-empty-sub">All compliance rules are passing.</p>
              </div>
            ) : (
              violations.map(v => (
                <div key={v.id} className="violation-item">
                  <div className="violation-header">
                    <span className={`violation-severity ${v.severity}`}>{v.severity}</span>
                    <span className="violation-rule">{v.rule_name}</span>
                  </div>
                  <div className="violation-details">{v.details}</div>
                  <div className="violation-meta">
                    {v.device_name && <span>{v.device_name}</span>}
                    <span>{timeAgo(v.detected_at)}</span>
                    <button
                      style={{ marginLeft: 'auto', fontSize: 10, color: 'var(--success)', background: 'none', border: 'none', cursor: 'pointer' }}
                      onClick={() => handleResolve(v.id)}
                    >
                      Resolve
                    </button>
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
