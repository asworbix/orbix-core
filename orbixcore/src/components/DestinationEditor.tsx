import { useState } from 'react';
import { destinationDescribe, type DestSchema } from '../hooks/api';

/** A write destination spec, stored in a workflow step's config.destination.
 *  Mirrors the Rust Destination enum (tagged by `kind`). */
export type DestinationSpec =
  | { kind: 'sqlite'; path: string; table: string }
  | {
      kind: 'http';
      base_url: string;
      path: string;
      method: string;
      auth?: { kind: 'none' } | { kind: 'bearer'; token: string } | { kind: 'api_key'; header: string; value: string };
    };

interface Props {
  destination: DestinationSpec | undefined;
  map: boolean;
  onChange: (destination: DestinationSpec, map: boolean) => void;
}

const BLANK_SQLITE: DestinationSpec = { kind: 'sqlite', path: '', table: '' };
const BLANK_HTTP: DestinationSpec = { kind: 'http', base_url: '', path: '', method: 'POST', auth: { kind: 'none' } };

export default function DestinationEditor({ destination, map, onChange }: Props) {
  const dest = destination ?? BLANK_SQLITE;
  const [schema, setSchema] = useState<DestSchema | null>(null);
  const [describing, setDescribing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = (next: DestinationSpec) => onChange(next, map);

  const readSchema = async () => {
    setDescribing(true);
    setError(null);
    setSchema(null);
    try {
      const s = await destinationDescribe(dest);
      setSchema(s);
    } catch (e) {
      setError(typeof e === 'string' ? e : 'Could not read the schema.');
    } finally {
      setDescribing(false);
    }
  };

  return (
    <div className="build-detail-section">
      <span className="build-detail-label">Write to</span>
      <div className="build-action-mode" role="group" aria-label="Destination kind" style={{ marginBottom: 8 }}>
        {([['sqlite', 'SQLite'], ['http', 'Dataverse / REST']] as const).map(([k, label]) => (
          <button
            key={k}
            className={`build-action-mode-opt ${dest.kind === k ? 'active' : ''}`}
            onClick={() => { if (dest.kind !== k) set(k === 'sqlite' ? BLANK_SQLITE : BLANK_HTTP); setSchema(null); }}
          >
            {label}
          </button>
        ))}
      </div>

      {dest.kind === 'sqlite' && (
        <>
          <input
            className="build-detail-input mono"
            placeholder="/Users/you/data.db"
            value={dest.path}
            onChange={e => set({ ...dest, path: e.target.value })}
          />
          <input
            className="build-detail-input mono"
            style={{ marginTop: 6 }}
            placeholder="table name (e.g. leads)"
            value={dest.table}
            onChange={e => set({ ...dest, table: e.target.value })}
          />
          <span className="build-detail-hint">Local SQLite file. The table must already exist.</span>
        </>
      )}

      {dest.kind === 'http' && (
        <>
          <input
            className="build-detail-input mono"
            placeholder="https://org.crm.dynamics.com"
            value={dest.base_url}
            onChange={e => set({ ...dest, base_url: e.target.value })}
          />
          <input
            className="build-detail-input mono"
            style={{ marginTop: 6 }}
            placeholder="/api/data/v9.2/contacts"
            value={dest.path}
            onChange={e => set({ ...dest, path: e.target.value })}
          />
          <select
            className="build-detail-select"
            style={{ marginTop: 6, width: '100%' }}
            value={dest.method}
            onChange={e => set({ ...dest, method: e.target.value })}
          >
            {['POST', 'PUT', 'PATCH'].map(m => <option key={m} value={m}>{m}</option>)}
          </select>
          <select
            className="build-detail-select"
            style={{ marginTop: 6, width: '100%' }}
            value={dest.auth?.kind ?? 'none'}
            onChange={e => {
              const k = e.target.value;
              set({ ...dest, auth: k === 'bearer' ? { kind: 'bearer', token: '' } : k === 'api_key' ? { kind: 'api_key', header: 'x-api-key', value: '' } : { kind: 'none' } });
            }}
          >
            <option value="none">No auth</option>
            <option value="bearer">Bearer token</option>
            <option value="api_key">API key header</option>
          </select>
          {dest.auth?.kind === 'bearer' && (
            <input
              className="build-detail-input mono"
              style={{ marginTop: 6 }}
              placeholder="token"
              value={dest.auth.token}
              onChange={e => set({ ...dest, auth: { kind: 'bearer', token: e.target.value } })}
            />
          )}
          {dest.auth?.kind === 'api_key' && (
            <div style={{ display: 'flex', gap: 6, marginTop: 6 }}>
              <input
                className="build-detail-input mono"
                placeholder="header"
                value={dest.auth.header}
                onChange={e => set({ ...dest, auth: { kind: 'api_key', header: e.target.value, value: (dest.auth as { value: string }).value } })}
              />
              <input
                className="build-detail-input mono"
                placeholder="value"
                value={dest.auth.value}
                onChange={e => set({ ...dest, auth: { kind: 'api_key', header: (dest.auth as { header: string }).header, value: e.target.value } })}
              />
            </div>
          )}
          <span className="build-detail-hint">Dataverse (OData) or any REST endpoint. One record per request.</span>
        </>
      )}

      <label className="build-detail-check" style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 10 }}>
        <input type="checkbox" checked={map} onChange={e => onChange(dest, e.target.checked)} />
        <span>Let the AI map the incoming data onto the target's columns</span>
      </label>
      <span className="build-detail-hint">
        On: Bob reads the schema and shapes the prior step's data into rows. Off: the prior step must already produce rows keyed by column.
      </span>

      <button className="build-detail-test" style={{ marginTop: 8 }} onClick={readSchema} disabled={describing}>
        {describing ? 'Reading…' : 'Read schema'}
      </button>
      {error && <span className="build-detail-hint" style={{ color: 'var(--error)' }}>{error}</span>}
      {schema && (
        <div className="build-detail-hint" style={{ marginTop: 6 }}>
          <strong>{schema.target}</strong>: {schema.columns.map(c => `${c.name}${c.primary_key ? '*' : ''}`).join(', ')}
        </div>
      )}
    </div>
  );
}
