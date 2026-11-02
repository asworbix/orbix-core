// Data-source registry for the Bob-extensible UI runtime.
//
// A block can reference a named source via { data: { source: "agents" } }
// and the renderer fills in rows/items/value automatically. Sources are
// kept narrow on purpose - Bob only sees what's listed here.

import {
  listAgents,
  listWorkflows,
  listConnectorInstances,
  listNotifications,
  syncActivityFeed,
} from '../hooks/api';

export type SourceFn = () => Promise<unknown[]>;

export const DYNAMIC_SOURCES: Record<string, SourceFn> = {
  agents: async () => {
    const rows = await listAgents();
    return rows.map((a) => ({
      id: a.id,
      name: a.name,
      provider: a.provider,
      model: a.model,
      enabled: a.enabled,
      default: a.is_default,
    }));
  },
  workflows: async () => {
    const rows = await listWorkflows();
    return rows.map((w) => ({
      id: w.id,
      name: w.name,
      description: w.description,
      steps: w.steps.length,
    }));
  },
  connectors: async () => {
    const rows = await listConnectorInstances().catch(() => []);
    return (rows as Array<Record<string, unknown>>).map((c) => ({
      id: c.id,
      name: c.name ?? c.connector_id,
      service: c.service ?? c.connector_id,
      status: c.status ?? 'unknown',
    }));
  },
  notifications: async () => {
    const rows = await listNotifications(50).catch(() => []);
    return (rows as Array<Record<string, unknown>>).map((n) => ({
      id: n.id,
      title: n.title ?? n.message,
      kind: n.kind ?? n.type,
      created_at: n.created_at,
    }));
  },
  activity: async () => {
    const feed = await syncActivityFeed().catch(() => null);
    const rows = Array.isArray(feed)
      ? feed
      : Array.isArray((feed as Record<string, unknown>)?.items)
      ? ((feed as Record<string, unknown>).items as unknown[])
      : [];
    return (rows as Array<Record<string, unknown>>).slice(0, 50).map((e) => ({
      id: e.id ?? e.timestamp,
      kind: e.kind ?? e.type,
      summary: e.summary ?? e.message,
      created_at: e.created_at ?? e.timestamp,
    }));
  },
};

export function isKnownSource(name: string): boolean {
  return Object.prototype.hasOwnProperty.call(DYNAMIC_SOURCES, name);
}

export async function resolveSource(name: string): Promise<unknown[]> {
  const fn = DYNAMIC_SOURCES[name];
  if (!fn) return [];
  try {
    return await fn();
  } catch {
    return [];
  }
}

/** Aggregate a list of rows into a single number for `metric` blocks. */
export function aggregateRows(
  rows: unknown[],
  aggregate: 'count' | 'sum' | 'avg' | 'min' | 'max' = 'count',
  field?: string,
): number {
  if (aggregate === 'count') return rows.length;
  if (!field) return rows.length;
  const nums = rows
    .map((r) => {
      const obj = r as Record<string, unknown>;
      const v = obj[field];
      return typeof v === 'number' ? v : Number(v);
    })
    .filter((n) => !isNaN(n));
  if (nums.length === 0) return 0;
  if (aggregate === 'sum') return nums.reduce((a, b) => a + b, 0);
  if (aggregate === 'avg') return nums.reduce((a, b) => a + b, 0) / nums.length;
  if (aggregate === 'min') return Math.min(...nums);
  if (aggregate === 'max') return Math.max(...nums);
  return rows.length;
}
