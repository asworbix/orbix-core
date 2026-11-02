// Connector Wizard - front-end for the activation pipeline from
// CONNECTORS.md. Lists the bundled reference modules, runs the scanner
// + activator on click, surfaces the scan report and signing key id,
// and offers a one-shot smoke test against the activated module.
//
// This wizard intentionally does NOT do OAuth here - that happens in
// the connector's own auth flow (existing ConnectorsPanel button), or
// later in a Bob-driven sequence. The wizard's job is the
// "scan + sign + activate + smoke" gate.

import { useEffect, useState } from 'react';
import {
  bobAuthorConnector,
  bobSmokeTest,
  connectorActivateModule,
  connectorInvoke,
  connectorModulesSummary, connectorStagingRoot,
  connectorScanStaging,
  connectorSignerKeyId,
  connectorVaultListKeys,
  startOauthForModule,
  type ActivationReport,
  type AuthorReport,
  type ConnectorResult,
  type ConnectorModulesSummary,
  type ExpiryHint,
  type ScanReport,
  type SmokeReport,
} from '../hooks/api';
import './ConnectorWizard.css';

// Resource paths under src-tauri/resources/modules/<slug>/v1 - the
// activator copies from these staging dirs into the live modules root.
// We resolve them via the tauri path API at run time; here we hand the
// user a stable absolute path they can override.
type Bundled = {
  slug: string;
  display: string;
  blurb: string;
  /// Path under <project-root>/orbixcore/src-tauri/resources/modules/
  resourcePath: string;
  smokeAction?: string;
  needsAuth: boolean;
};

const BUNDLED: Bundled[] = [
  {
    slug: 'echo',
    display: 'Echo (raw)',
    blurb: 'Minimum-trust fixture. No network, no env. Returns whatever you send in. Used to verify the runtime end-to-end.',
    resourcePath: 'echo/v1',
    smokeAction: 'ping',
    needsAuth: false,
  },
  {
    slug: 'echo_sdk',
    display: 'Echo (SDK)',
    blurb: 'Same contract as echo, but uses the orbix-sdk.ts shim. Demonstrates the shape every Bob-authored module follows.',
    resourcePath: 'echo_sdk/v1',
    smokeAction: 'ping',
    needsAuth: false,
  },
  {
    slug: 'linkedin',
    display: 'LinkedIn',
    blurb: 'OAuth2 to api.linkedin.com. Three actions: userinfo, draft_post, post. Token reaches the subprocess by env injection only.',
    resourcePath: 'linkedin/v1',
    smokeAction: 'userinfo',
    needsAuth: true,
  },
];

interface Props { onClose: () => void }

export default function ConnectorWizard({ onClose }: Props) {
  const [step, setStep] = useState<'pick' | 'review' | 'draft' | 'done'>('pick');
  const [selected, setSelected] = useState<Bundled | null>(null);
  const [stagingDir, setStagingDir] = useState('');
  const [stagingRoot, setStagingRoot] = useState('');
  const [scan, setScan] = useState<ScanReport | null>(null);
  const [activation, setActivation] = useState<ActivationReport | null>(null);
  const [smoke, setSmoke] = useState<ConnectorResult | null>(null);
  const [smokeReport, setSmokeReport] = useState<SmokeReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [signerKid, setSignerKid] = useState<string | null>(null);
  const [installed, setInstalled] = useState<ConnectorModulesSummary | null>(null);

  useEffect(() => {
    connectorModulesSummary().then(setInstalled).catch(() => setInstalled(null));
  }, [step]);

  // W5.7: resolve the bundled staging root via the Tauri resource API
  // instead of a hardcoded developer path.
  useEffect(() => {
    connectorStagingRoot().then(setStagingRoot).catch(() => setStagingRoot(''));
  }, []);

  function pick(b: Bundled) {
    setSelected(b);
    setError(null);
    setScan(null);
    setActivation(null);
    setSmoke(null);
    setSmokeReport(null);
    setStagingDir(stagingRoot ? `${stagingRoot}/${b.resourcePath}` : '');
    setStep('review');
  }

  function pickDraft() {
    setSelected(null);
    setError(null);
    setScan(null);
    setActivation(null);
    setSmoke(null);
    setSmokeReport(null);
    setStep('draft');
  }

  async function runAuthor(draft: {
    manifest_json: string;
    module_ts: string;
    tests_ts?: string;
    readme?: string;
  }) {
    setBusy(true);
    setError(null);
    try {
      const report: AuthorReport = await bobAuthorConnector(draft);
      setActivation(report.activation);
      setSmokeReport(report.smoke);
      try { setSignerKid(await connectorSignerKeyId()); } catch { /* keychain may be cold */ }
      setStep('done');
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runSmokeAgainstInstalled() {
    if (!activation) return;
    setBusy(true);
    setError(null);
    try {
      const r = await bobSmokeTest(activation.slug);
      setSmokeReport(r);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runScan() {
    if (!stagingDir) return;
    setBusy(true);
    setError(null);
    try {
      const r = await connectorScanStaging(stagingDir);
      setScan(r);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runActivate() {
    if (!stagingDir) return;
    setBusy(true);
    setError(null);
    try {
      const a = await connectorActivateModule(stagingDir);
      setActivation(a);
      try { setSignerKid(await connectorSignerKeyId()); } catch { /* keychain may be cold */ }
      setStep('done');
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runSmoke() {
    if (!selected?.smokeAction) return;
    setBusy(true);
    setError(null);
    try {
      const r = await connectorInvoke({
        connectorSlug: selected.slug,
        actionId: selected.smokeAction,
        inputs: {},
        idempotencyKey: `wizard-smoke-${Date.now()}`,
        caller: 'wizard',
      });
      setSmoke(r);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="cwiz-backdrop" onClick={onClose}>
      <div className="cwiz-shell" onClick={(e) => e.stopPropagation()}>
        <header className="cwiz-head">
          <h2>Add connector</h2>
          <button className="cwiz-close" onClick={onClose} aria-label="close">×</button>
        </header>

        {step === 'pick' && (
          <PickStep
            installed={installed?.slugs ?? []}
            onPick={pick}
            onPickDraft={pickDraft}
          />
        )}

        {step === 'draft' && (
          <DraftStep
            busy={busy}
            error={error}
            onSubmit={runAuthor}
            onBack={() => setStep('pick')}
          />
        )}

        {step === 'review' && selected && (
          <ReviewStep
            bundled={selected}
            stagingDir={stagingDir}
            onStagingDir={setStagingDir}
            scan={scan}
            busy={busy}
            error={error}
            onScan={runScan}
            onActivate={runActivate}
            onBack={() => setStep('pick')}
          />
        )}

        {step === 'done' && activation && (
          <DoneStep
            bundled={selected}
            activation={activation}
            signerKid={signerKid}
            smoke={smoke}
            smokeReport={smokeReport}
            busy={busy}
            error={error}
            onSmoke={runSmoke}
            onRerunSmokeTest={runSmokeAgainstInstalled}
            onAddAnother={() => { setSelected(null); setStep('pick'); }}
            onClose={onClose}
            onError={setError}
          />
        )}
      </div>
    </div>
  );
}

function PickStep({ installed, onPick, onPickDraft }: {
  installed: string[];
  onPick: (b: Bundled) => void;
  onPickDraft: () => void;
}) {
  return (
    <div className="cwiz-body">
      <p className="cwiz-lede">
        Activation runs a real TypeScript AST scan against each module's
        manifest, signs it with the Ed25519 key in your macOS Keychain,
        then atomically installs into your modules directory.
      </p>
      <div className="cwiz-grid">
        {BUNDLED.map((b) => {
          const already = installed.includes(b.slug);
          return (
            <button
              key={b.slug}
              className="cwiz-card"
              onClick={() => onPick(b)}
            >
              <div className="cwiz-card-head">
                <strong>{b.display}</strong>
                {already && <span className="cwiz-tag">installed</span>}
                {b.needsAuth && <span className="cwiz-tag cwiz-tag-auth">OAuth</span>}
              </div>
              <p>{b.blurb}</p>
              <span className="cwiz-slug">{b.slug}</span>
            </button>
          );
        })}
        <button className="cwiz-card cwiz-card-custom" onClick={onPickDraft}>
          <div className="cwiz-card-head">
            <strong>Author from drafts</strong>
            <span className="cwiz-tag cwiz-tag-auth">advanced</span>
          </div>
          <p>
            Paste a manifest + module.ts. Runs the full activation pipeline:
            capability check → AST scan → Ed25519 sign → atomic install. If
            you provide tests.ts, the smoke test runs after install.
          </p>
          <span className="cwiz-slug">bob_author_connector</span>
        </button>
      </div>
    </div>
  );
}

function DraftStep({ busy, error, onSubmit, onBack }: {
  busy: boolean;
  error: string | null;
  onSubmit: (d: {
    manifest_json: string;
    module_ts: string;
    tests_ts?: string;
    readme?: string;
  }) => void;
  onBack: () => void;
}) {
  const [manifestJson, setManifestJson] = useState('');
  const [moduleTs, setModuleTs] = useState('');
  const [testsTs, setTestsTs] = useState('');
  const [readme, setReadme] = useState('');

  const canSubmit = manifestJson.trim().length > 0 && moduleTs.trim().length > 0;

  return (
    <div className="cwiz-body">
      <button className="cwiz-back" onClick={onBack}>← Back to picker</button>
      <h3>Author from drafts</h3>
      <p className="cwiz-lede">
        Same pipeline a Bob-driven flow will use. The scanner, signer, and
        activator run against whatever you paste; failures surface here
        instead of being silently dropped.
      </p>

      <label className="cwiz-field">
        <span>manifest.json - required</span>
        <textarea
          className="cwiz-textarea"
          value={manifestJson}
          onChange={(e) => setManifestJson(e.target.value)}
          rows={10}
          placeholder='{"slug":"my-connector","version":"0.1.0","display_name":"My","vendor":"…","driver":{"kind":"api"},"auth":{"kind":"none"},"actions":{}}'
          spellCheck={false}
        />
      </label>

      <label className="cwiz-field">
        <span>module.ts - required</span>
        <textarea
          className="cwiz-textarea"
          value={moduleTs}
          onChange={(e) => setModuleTs(e.target.value)}
          rows={14}
          placeholder='import { defineConnector } from "./orbix-sdk.ts";

defineConnector({
  ping: ({ inputs }) => ({ pong: true, inputs }),
});'
          spellCheck={false}
        />
      </label>

      <label className="cwiz-field">
        <span>tests.ts - optional (drives the smoke test)</span>
        <textarea
          className="cwiz-textarea"
          value={testsTs}
          onChange={(e) => setTestsTs(e.target.value)}
          rows={6}
          placeholder='// Optional. Runs after activation under the same Deno permissions.'
          spellCheck={false}
        />
      </label>

      <label className="cwiz-field">
        <span>README.md - optional</span>
        <textarea
          className="cwiz-textarea"
          value={readme}
          onChange={(e) => setReadme(e.target.value)}
          rows={4}
          spellCheck={false}
        />
      </label>

      {error && <div className="cwiz-error">{error}</div>}

      <div className="cwiz-actions">
        <button onClick={onBack} disabled={busy}>Cancel</button>
        <button
          className="cwiz-primary"
          onClick={() => onSubmit({
            manifest_json: manifestJson,
            module_ts: moduleTs,
            tests_ts: testsTs.trim() ? testsTs : undefined,
            readme: readme.trim() ? readme : undefined,
          })}
          disabled={busy || !canSubmit}
        >
          {busy ? 'Authoring…' : 'Scan + sign + activate'}
        </button>
      </div>
    </div>
  );
}

function ReviewStep({
  bundled, stagingDir, onStagingDir,
  scan, busy, error,
  onScan, onActivate, onBack,
}: {
  bundled: Bundled;
  stagingDir: string;
  onStagingDir: (s: string) => void;
  scan: ScanReport | null;
  busy: boolean;
  error: string | null;
  onScan: () => void;
  onActivate: () => void;
  onBack: () => void;
}) {
  return (
    <div className="cwiz-body">
      <button className="cwiz-back" onClick={onBack}>← Pick a different connector</button>
      <h3>{bundled.display}</h3>
      <p className="cwiz-lede">{bundled.blurb}</p>

      <label className="cwiz-field">
        <span>Staging directory</span>
        <input
          value={stagingDir}
          onChange={(e) => onStagingDir(e.target.value)}
          spellCheck={false}
        />
      </label>

      <div className="cwiz-actions">
        <button onClick={onScan} disabled={busy}>
          {busy && !scan ? 'Scanning…' : 'Run scanner'}
        </button>
        <button
          className="cwiz-primary"
          onClick={onActivate}
          disabled={busy || (scan ? scan.violations.length > 0 : false)}
        >
          {busy && scan ? 'Activating…' : 'Scan + sign + activate'}
        </button>
      </div>

      {scan && (
        <div className="cwiz-scan">
          {scan.violations.length === 0 ? (
            <div className="cwiz-pass">✓ Scan passed</div>
          ) : (
            <div className="cwiz-fail">
              <strong>Scan failed - activation blocked</strong>
              <ul>{scan.violations.map((v, i) => <li key={i}>{v}</li>)}</ul>
            </div>
          )}
          <details>
            <summary>What the scanner saw</summary>
            <dl>
              <dt>Hosts</dt><dd>{scan.hosts_found.join(', ') || '- none -'}</dd>
              <dt>Env reads</dt><dd>{scan.env_reads_found.join(', ') || '- none -'}</dd>
              <dt>Imports</dt><dd>{scan.imports_found.join(', ') || '- none -'}</dd>
              {scan.dynamic_access.length > 0 && (
                <>
                  <dt>Dynamic patterns</dt>
                  <dd>{scan.dynamic_access.join('; ')}</dd>
                </>
              )}
            </dl>
          </details>
        </div>
      )}

      {error && <div className="cwiz-error">{error}</div>}
    </div>
  );
}

function DoneStep({
  bundled, activation, signerKid, smoke, smokeReport, busy, error,
  onSmoke, onRerunSmokeTest, onAddAnother, onClose, onError,
}: {
  bundled: Bundled | null;
  activation: ActivationReport;
  signerKid: string | null;
  smoke: ConnectorResult | null;
  smokeReport: SmokeReport | null;
  busy: boolean;
  error: string | null;
  onSmoke: () => void;
  onRerunSmokeTest: () => void;
  onAddAnother: () => void;
  onClose: () => void;
  onError: (msg: string | null) => void;
}) {
  const [authState, setAuthState] = useState<
    | { kind: 'idle' }
    | { kind: 'connecting' }
    | { kind: 'done'; hint: ExpiryHint }
  >({ kind: 'idle' });
  const [vaultKeys, setVaultKeys] = useState<string[]>([]);

  useEffect(() => {
    connectorVaultListKeys(activation.slug)
      .then(setVaultKeys)
      .catch(() => setVaultKeys([]));
  }, [activation.slug, authState]);

  async function runOauth() {
    setAuthState({ kind: 'connecting' });
    onError(null);
    try {
      const hint = await startOauthForModule(activation.slug);
      setAuthState({ kind: 'done', hint });
    } catch (e: any) {
      onError(String(e));
      setAuthState({ kind: 'idle' });
    }
  }

  const alreadyAuthed = vaultKeys.includes('access_token');

  const title = bundled
    ? `${bundled.display} activated`
    : `${activation.slug} activated`;

  return (
    <div className="cwiz-body">
      <h3>{title}</h3>
      <dl className="cwiz-summary">
        <dt>Slug</dt><dd>{activation.slug}</dd>
        <dt>Version</dt><dd>{activation.version}</dd>
        <dt>Installed at</dt><dd>{activation.installed_at}</dd>
        <dt>Signed key id</dt>
        <dd className="cwiz-mono">{signerKid ?? activation.signed_key_id}</dd>
        <dt>SDK sha-256</dt>
        <dd className="cwiz-mono" title={activation.sdk_sha256}>
          {activation.sdk_sha256.slice(0, 16)}…
        </dd>
        {vaultKeys.length > 0 && (
          <>
            <dt>Vault keys</dt>
            <dd className="cwiz-mono">{vaultKeys.join(', ')}</dd>
          </>
        )}
      </dl>

      {smokeReport && (
        <div className={`cwiz-scan ${smokeReport.passed ? '' : 'cwiz-fail'}`}>
          {smokeReport.passed
            ? <div className="cwiz-pass">✓ Smoke test passed ({smokeReport.elapsed_ms} ms)</div>
            : (
              <div className="cwiz-fail">
                <strong>
                  Smoke test failed
                  {smokeReport.timed_out && ' (timed out)'}
                  {smokeReport.exit_code !== null && ` (exit ${smokeReport.exit_code})`}
                </strong>
              </div>
            )
          }
          <details open={!smokeReport.passed}>
            <summary>tests.ts output</summary>
            {smokeReport.stdout && (
              <>
                <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 8 }}>stdout</div>
                <pre className="cwiz-mono">{smokeReport.stdout}</pre>
              </>
            )}
            {smokeReport.stderr && (
              <>
                <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 8 }}>stderr</div>
                <pre className="cwiz-mono cwiz-fail-pre">{smokeReport.stderr}</pre>
              </>
            )}
          </details>
          <button
            onClick={onRerunSmokeTest}
            disabled={busy}
            style={{ alignSelf: 'flex-start', marginTop: 6 }}
          >
            {busy ? 'Running…' : 'Re-run smoke test'}
          </button>
        </div>
      )}

      {bundled?.needsAuth && !alreadyAuthed && (
        <div className="cwiz-note">
          <p style={{ margin: '0 0 8px' }}>
            This connector needs OAuth before it can do real work. Signing
            in opens your browser; tokens land in the slug vault and are
            injected into the connector subprocess at the next spawn.
          </p>
          <button
            className="cwiz-primary"
            onClick={runOauth}
            disabled={authState.kind === 'connecting'}
            style={{ padding: '8px 14px', fontSize: 13 }}
          >
            {authState.kind === 'connecting'
              ? 'Waiting on browser…'
              : `Sign in with ${bundled?.display ?? activation.slug}`}
          </button>
          {authState.kind === 'done' && (
            <p style={{ margin: '8px 0 0', fontSize: 12, color: 'var(--success)' }}>
              ✓ Token saved
              {authState.hint.has_refresh_token && ' (refresh token included)'}
              {authState.hint.expires_at_unix && (
                <> · expires {new Date(authState.hint.expires_at_unix * 1000).toLocaleString()}</>
              )}
            </p>
          )}
        </div>
      )}

      {bundled?.needsAuth && alreadyAuthed && (
        <div className="cwiz-note" style={{ background: 'rgba(74, 139, 92, 0.10)' }}>
          ✓ OAuth credentials already in the vault for {activation.slug}.
        </div>
      )}

      {bundled?.smokeAction && (
        <div className="cwiz-smoke">
          <button onClick={onSmoke} disabled={busy}>
            {busy ? 'Running…' : `Live invoke - ${bundled.slug}.${bundled.smokeAction}`}
          </button>
          {smoke && (
            <pre className={smoke.success ? 'cwiz-mono cwiz-ok' : 'cwiz-mono cwiz-fail-pre'}>
              {JSON.stringify(smoke, null, 2)}
            </pre>
          )}
        </div>
      )}

      {error && <div className="cwiz-error">{error}</div>}

      <div className="cwiz-actions">
        <button onClick={onAddAnother}>Add another</button>
        <button className="cwiz-primary" onClick={onClose}>Done</button>
      </div>
    </div>
  );
}

