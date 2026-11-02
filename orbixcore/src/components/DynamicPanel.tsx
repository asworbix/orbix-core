import { useCallback, useEffect, useMemo, useState } from 'react';
import { marked } from 'marked';
import type { DynamicPanel as PanelSpec, DynamicPanelBlock } from '../types';
import { onDataChanged } from '../hooks/api';
import { aggregateRows, isKnownSource, resolveSource } from '../utils/dynamic-sources';
import './DynamicPanel.css';

interface Props {
  panel: PanelSpec;
  onAction?: (block: DynamicPanelBlock) => void;
  onClose?: () => void;
}

function asString(v: unknown, fallback = ''): string {
  return typeof v === 'string' ? v : fallback;
}

function asNumber(v: unknown): number | null {
  if (typeof v === 'number') return v;
  if (typeof v === 'string' && v.trim() !== '' && !isNaN(Number(v))) return Number(v);
  return null;
}

function asArray<T = unknown>(v: unknown): T[] {
  return Array.isArray(v) ? (v as T[]) : [];
}

function renderMd(text: string): string {
  return marked.parse(text, { async: false }) as string;
}

interface SourceMap {
  [source: string]: unknown[];
}

/**
 * Read `block.data.source` and merge resolved rows into the block's render
 * shape. For tables we project rows[] into rows; for lists we project a
 * field; for metrics we aggregate.
 */
function applySource(
  block: DynamicPanelBlock,
  sources: SourceMap,
): DynamicPanelBlock {
  const data = block.data as
    | { source?: string; field?: string; aggregate?: string; limit?: number }
    | undefined;
  if (!data || typeof data.source !== 'string') return block;
  const rows = sources[data.source];
  if (!rows) return block;
  const limited =
    typeof data.limit === 'number' ? rows.slice(0, data.limit) : rows;

  switch (block.kind) {
    case 'metric': {
      const agg = (data.aggregate as 'count' | 'sum' | 'avg' | 'min' | 'max') ?? 'count';
      const value = aggregateRows(limited, agg, data.field);
      return { ...block, value };
    }
    case 'list': {
      const field = data.field ?? 'name';
      const items = limited.map((r) => {
        const obj = r as Record<string, unknown>;
        const v = obj[field];
        return v !== undefined && v !== null ? String(v) : JSON.stringify(r);
      });
      return { ...block, items };
    }
    case 'table': {
      return { ...block, rows: limited };
    }
    default:
      return block;
  }
}

function Block({
  block,
  onAction,
  sources,
}: {
  block: DynamicPanelBlock;
  onAction?: (block: DynamicPanelBlock) => void;
  sources: SourceMap;
}) {
  const resolved = applySource(block, sources);
  block = resolved;
  switch (block.kind) {
    case 'header': {
      const level = Math.min(Math.max(asNumber(block.level) ?? 2, 1), 4);
      const text = asString(block.text);
      const Tag = (`h${level}` as unknown) as keyof JSX.IntrinsicElements;
      return <Tag className="dp-header">{text}</Tag>;
    }

    case 'text': {
      const content = asString(block.content);
      return (
        <div
          className="dp-text"
          dangerouslySetInnerHTML={{ __html: renderMd(content) }}
        />
      );
    }

    case 'metric': {
      const label = asString(block.label, 'Metric');
      const value = block.value !== undefined ? String(block.value) : '-';
      const hint = asString(block.hint);
      return (
        <div className="dp-metric">
          <div className="dp-metric-value">{value}</div>
          <div className="dp-metric-label">{label}</div>
          {hint && <div className="dp-metric-hint">{hint}</div>}
        </div>
      );
    }

    case 'metrics': {
      const items = asArray<DynamicPanelBlock>(block.items);
      return (
        <div className="dp-metrics-row">
          {items.map((m, i) => (
            <Block key={i} block={{ ...m, kind: 'metric' }} sources={sources} />
          ))}
        </div>
      );
    }

    case 'list': {
      const items = asArray<unknown>(block.items).map((it) => String(it));
      const ordered = block.ordered === true;
      const Tag: keyof JSX.IntrinsicElements = ordered ? 'ol' : 'ul';
      return (
        <Tag className="dp-list">
          {items.map((t, i) => (
            <li key={i}>{t}</li>
          ))}
        </Tag>
      );
    }

    case 'table': {
      const columns = asArray<unknown>(block.columns).map((c) => {
        if (typeof c === 'string') return { key: c, label: c };
        const obj = c as { key?: string; label?: string };
        return { key: obj.key ?? '', label: obj.label ?? obj.key ?? '' };
      });
      const rows = asArray<unknown>(block.rows);
      return (
        <div className="dp-table-wrap">
          <table className="dp-table">
            <thead>
              <tr>
                {columns.map((c) => (
                  <th key={c.key}>{c.label}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, ri) => {
                if (Array.isArray(row)) {
                  return (
                    <tr key={ri}>
                      {row.map((cell, ci) => (
                        <td key={ci}>{String(cell ?? '')}</td>
                      ))}
                    </tr>
                  );
                }
                if (typeof row === 'object' && row !== null) {
                  const obj = row as Record<string, unknown>;
                  return (
                    <tr key={ri}>
                      {columns.map((c) => (
                        <td key={c.key}>{String(obj[c.key] ?? '')}</td>
                      ))}
                    </tr>
                  );
                }
                return null;
              })}
            </tbody>
          </table>
        </div>
      );
    }

    case 'divider':
      return <hr className="dp-divider" />;

    case 'action': {
      const label = asString(block.label, 'Action');
      return (
        <button className="dp-action-btn" onClick={() => onAction?.(block)}>
          {label}
        </button>
      );
    }

    case 'actions': {
      const items = asArray<DynamicPanelBlock>(block.items);
      return (
        <div className="dp-actions-row">
          {items.map((a, i) => (
            <Block
              key={i}
              block={{ ...a, kind: 'action' }}
              onAction={onAction}
              sources={sources}
            />
          ))}
        </div>
      );
    }

    default:
      return (
        <div className="dp-unknown">
          <code>unknown block: {block.kind}</code>
        </div>
      );
  }
}

function collectSources(blocks: DynamicPanelBlock[]): string[] {
  const seen = new Set<string>();
  const visit = (b: DynamicPanelBlock) => {
    const data = b.data as { source?: string } | undefined;
    if (data && typeof data.source === 'string' && isKnownSource(data.source)) {
      seen.add(data.source);
    }
    const items = (b as { items?: unknown }).items;
    if (Array.isArray(items)) {
      items.forEach((it) => visit(it as DynamicPanelBlock));
    }
  };
  blocks.forEach(visit);
  return Array.from(seen);
}

export default function DynamicPanel({ panel, onAction, onClose }: Props) {
  const [sources, setSources] = useState<SourceMap>({});
  const neededSources = useMemo(() => collectSources(panel.blocks), [panel.blocks]);

  const refreshSources = useCallback(async () => {
    if (neededSources.length === 0) {
      setSources({});
      return;
    }
    const entries = await Promise.all(
      neededSources.map(async (s) => [s, await resolveSource(s)] as const),
    );
    setSources(Object.fromEntries(entries));
  }, [neededSources]);

  useEffect(() => {
    refreshSources();
    const unsub = onDataChanged(() => refreshSources());
    return () => {
      unsub.then((fn) => fn());
    };
  }, [refreshSources]);

  const handleAction = useCallback(
    (block: DynamicPanelBlock) => {
      onAction?.(block);
    },
    [onAction],
  );

  return (
    <div className="dp-root">
      <div className="dp-head">
        <div className="dp-head-text">
          <h2 className="dp-title">{panel.title}</h2>
          {panel.description && (
            <p className="dp-description">{panel.description}</p>
          )}
        </div>
        {onClose && (
          <button className="dp-close" onClick={onClose} title="Close" aria-label="Close">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        )}
      </div>
      <div className="dp-body">
        {panel.blocks.length === 0 ? (
          <div className="dp-empty">This panel is empty.</div>
        ) : (
          panel.blocks.map((block, i) => (
            <Block key={i} block={block} onAction={handleAction} sources={sources} />
          ))
        )}
      </div>
      <div className="dp-foot">
        <span className="dp-foot-meta">
          Built by {panel.created_by} ·{' '}
          {new Date(panel.updated_at || panel.created_at).toLocaleString()}
        </span>
      </div>
    </div>
  );
}
