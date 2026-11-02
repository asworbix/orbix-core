// ─────────────────────────────────────────────
//  Connector Runtime - Phase 2 of CONNECTORS.md
//
//  One OS subprocess per active connector module. Each subprocess is a
//  `deno run --allow-net=<hosts> --allow-env=<envs> --no-prompt --quiet`
//  call against the module's TypeScript entry point. Host and env
//  restrictions are enforced by the OS kernel via Deno's permission
//  model, not by JS-level checks - compromise of one connector cannot
//  reach hosts outside its manifest.
//
//  IPC: NDJSON over stdin/stdout. The runtime writes one JSON line per
//  job; the module writes one JSON line per result. Request IDs
//  correlate concurrent jobs even though a given module instance
//  processes them serially (per-slug concurrency is enforced by the
//  bus, not here).
//
//  Supervision:
//    - Restart on crash with exponential backoff 1s → 2s → 4s → 8s → 30s cap.
//    - Circuit break after 5 crashes in 60s; stays open 30s.
//    - Health events emitted to the core_event_bus.
//
//  Deno binary: configurable path. Defaults to the bundled Tauri sidecar
//  location; falls back to `deno` on PATH so dev machines can iterate
//  without rebuilding the installer. (Bundle infra: separate commit.)
// ─────────────────────────────────────────────

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};

use crate::browser_attach;
use crate::connector_bus::{ConnectorBackend, ConnectorJob, ConnectorResult};
use crate::connector_manifest::{deno_spawn_args, LoadedModule};
use crate::connector_vault;

const RESTART_BACKOFF_INITIAL_MS: u64 = 1_000;
const RESTART_BACKOFF_MAX_MS: u64 = 30_000;
const CRASH_TRIP_THRESHOLD: u32 = 5;
const CRASH_TRIP_WINDOW: Duration = Duration::from_secs(60);
const CIRCUIT_OPEN_DURATION: Duration = Duration::from_secs(30);
const JOB_TIMEOUT_DEFAULT: Duration = Duration::from_secs(15);
const MAX_INFLIGHT_PER_MODULE: usize = 32;

// ─────────────────────────────────────────────
//  Wire protocol
// ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct WireRequest<'a> {
    id: String,
    action: &'a str,
    inputs: &'a serde_json::Value,
}

/// Lines from a module subprocess can be either a response to a job
/// (paired by id) or an out-of-band op the supervisor handles inline
/// before reading the next line. The distinguishing field is `op`:
///   - present → out-of-band (vault_put, vault_get, …)
///   - absent  → response, must carry `id`
///
/// All ops scope to the module's OWN slug. The supervisor never reads
/// a slug field from the wire; it uses the LoadedModule's slug it
/// already has. This is what stops a compromised module from poking
/// another connector's vault.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WireFromModule {
    Op(WireOp),
    Response(WireResponse),
}

#[derive(Debug, Deserialize)]
struct WireOp {
    op: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    id: String,
    #[serde(default)]
    success: bool,
    #[serde(default)]
    output: serde_json::Value,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

// ─────────────────────────────────────────────
//  Inbox message - one per pending job
// ─────────────────────────────────────────────

struct InboxMsg {
    job: ConnectorJob,
    reply: oneshot::Sender<ConnectorResult>,
}

// ─────────────────────────────────────────────
//  Per-module supervisor state
// ─────────────────────────────────────────────

#[derive(Debug, Default)]
struct CrashStats {
    crashes: Vec<Instant>,
    open_until: Option<Instant>,
}

impl CrashStats {
    fn is_circuit_open(&mut self) -> bool {
        if let Some(until) = self.open_until {
            if Instant::now() < until { return true; }
            self.open_until = None;
            self.crashes.clear();
        }
        false
    }

    fn record_crash(&mut self) {
        let now = Instant::now();
        let cutoff = now.checked_sub(CRASH_TRIP_WINDOW).unwrap_or(now);
        self.crashes.retain(|t| *t > cutoff);
        self.crashes.push(now);
        if self.crashes.len() as u32 >= CRASH_TRIP_THRESHOLD {
            self.open_until = Some(now + CIRCUIT_OPEN_DURATION);
        }
    }
}

struct ModuleHandle {
    slug: String,
    inbox: mpsc::Sender<InboxMsg>,
    job_timeout: Duration,
}

// ─────────────────────────────────────────────
//  Supervisor - owns every running module
// ─────────────────────────────────────────────

pub struct RuntimeSupervisor {
    modules: std::sync::RwLock<HashMap<String, ModuleHandle>>,
    /// Where the bundled Deno binary lives, or None to fall back to PATH.
    deno_path: Option<PathBuf>,
}

/// Look for the Tauri-bundled Deno binary alongside the current executable.
/// In a packaged macOS app this lives next to the main binary inside the
/// .app bundle. In dev (cargo run / tauri dev), the externalBin path is
/// `<src-tauri>/binaries/deno-<triple>`.
fn resolve_sidecar() -> Option<PathBuf> {
    let triple = current_triple();
    let exe_name = if cfg!(target_os = "windows") {
        format!("deno-{}.exe", triple)
    } else {
        format!("deno-{}", triple)
    };

    // 1. Packaged app: same dir as the running executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(&exe_name);
            if is_real_deno(&candidate) { return Some(candidate); }
            // Some Tauri targets place externalBin under Resources/.
            let alt = dir.join("../Resources").join(&exe_name);
            if is_real_deno(&alt) { return Some(alt); }
        }
    }

    // 2. Dev: next to the crate's tauri.conf.json. Walk up from current_exe
    //    until we find src-tauri/binaries/. Cheap, works for cargo run.
    let mut here = std::env::current_exe().ok()?;
    for _ in 0..6 {
        let candidate = here.join("binaries").join(&exe_name);
        if is_real_deno(&candidate) { return Some(candidate); }
        let candidate = here.join("src-tauri").join("binaries").join(&exe_name);
        if is_real_deno(&candidate) { return Some(candidate); }
        if !here.pop() { break; }
    }
    None
}

/// True if the file at `path` looks like a real Deno binary, not the
/// 80-byte placeholder build.rs drops in for cargo to satisfy Tauri's
/// externalBin existence check. Real Deno is tens of MB.
fn is_real_deno(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if !meta.is_file() { return false; }
    meta.len() > 1_000_000
}

fn current_triple() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else {
        ""
    }
}

impl RuntimeSupervisor {
    pub fn new(deno_path: Option<PathBuf>) -> Self {
        Self {
            modules: std::sync::RwLock::new(HashMap::new()),
            deno_path,
        }
    }

    /// Resolve the binary used for `deno run`. Resolution order:
    ///   1. Explicit path passed to RuntimeSupervisor::new (tests, etc).
    ///   2. The Tauri sidecar path next to the running executable:
    ///        <exe_dir>/deno-<target-triple>
    ///   3. `deno` on PATH (dev convenience, fallback).
    fn deno_command(&self) -> String {
        if let Some(p) = &self.deno_path {
            if p.exists() { return p.to_string_lossy().into_owned(); }
        }
        // The shipped path: Deno is downloaded on first connector use into the
        // app data dir (see deno_sidecar). Prefer it over the dev sidecar.
        if let Some(managed) = crate::deno_sidecar::existing_managed() {
            return managed.to_string_lossy().into_owned();
        }
        if let Some(sidecar) = resolve_sidecar() {
            return sidecar.to_string_lossy().into_owned();
        }
        "deno".to_string()
    }

    /// Spawn supervised subprocesses for every module passed in. Already-
    /// registered slugs are skipped (caller is expected to restart on
    /// manifest change separately).
    pub fn install_modules(self: &Arc<Self>, loaded: Vec<LoadedModule>) {
        for m in loaded {
            let slug = m.slug.clone();
            if self.modules.read().map(|map| map.contains_key(&slug)).unwrap_or(false) {
                continue;
            }
            let (inbox_tx, inbox_rx) = mpsc::channel::<InboxMsg>(MAX_INFLIGHT_PER_MODULE);
            let job_timeout = Duration::from_secs(m.manifest.resource_limits.job_timeout_seconds as u64);
            let handle = ModuleHandle { slug: slug.clone(), inbox: inbox_tx, job_timeout };
            if let Ok(mut map) = self.modules.write() {
                map.insert(slug.clone(), handle);
            }
            let supervisor = Arc::clone(self);
            // tauri::async_runtime::spawn routes through Tauri's runtime so
            // install_modules() can be called from .setup() without a
            // tokio reactor bound to the calling thread.
            tauri::async_runtime::spawn(async move {
                supervise_module(supervisor, slug, m, inbox_rx).await;
            });
        }
    }
}

#[async_trait]
impl ConnectorBackend for RuntimeSupervisor {
    fn handles(&self, slug: &str) -> bool {
        self.modules.read().map(|m| m.contains_key(slug)).unwrap_or(false)
    }

    async fn dispatch(&self, job: &ConnectorJob) -> ConnectorResult {
        let (inbox, timeout) = {
            let Ok(modules) = self.modules.read() else {
                return ConnectorResult::err("runtime_unavailable", "supervisor lock poisoned");
            };
            match modules.get(&job.connector_slug) {
                Some(h) => (h.inbox.clone(), h.job_timeout),
                None => return ConnectorResult::err(
                    "no_module",
                    format!("no runtime module loaded for slug '{}'", job.connector_slug),
                ),
            }
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        let msg = InboxMsg { job: job.clone(), reply: reply_tx };
        if inbox.send(msg).await.is_err() {
            return ConnectorResult::err(
                "module_unreachable",
                "module supervisor task has exited",
            );
        }

        match tokio::time::timeout(timeout, reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => ConnectorResult::err(
                "module_unreachable",
                "supervisor dropped the reply channel before a result arrived",
            ),
            Err(_) => ConnectorResult::err(
                "job_timeout",
                format!("job exceeded {}s", timeout.as_secs()),
            ),
        }
    }
}

// ─────────────────────────────────────────────
//  Per-module supervision loop
//
//  Owns the subprocess lifecycle: spawn → talk → wait → backoff → respawn.
//  The bus serialises jobs per-slug (semaphore=4 in connector_bus.rs); we
//  treat the module as single-flight here for simpler IPC. If we ever
//  need parallel jobs per module, switch to a request-id-keyed map.
// ─────────────────────────────────────────────

async fn supervise_module(
    supervisor: Arc<RuntimeSupervisor>,
    slug: String,
    module: LoadedModule,
    mut inbox: mpsc::Receiver<InboxMsg>,
) {
    let mut crashes = CrashStats::default();
    let mut backoff_ms = RESTART_BACKOFF_INITIAL_MS;

    loop {
        if crashes.is_circuit_open() {
            // Drain inbox with circuit_open errors until the breaker closes.
            tokio::select! {
                _ = tokio::time::sleep(CIRCUIT_OPEN_DURATION) => {}
                Some(msg) = inbox.recv() => {
                    let _ = msg.reply.send(ConnectorResult::err(
                        "circuit_open",
                        format!("module '{}' is circuit-broken; recovering", slug),
                    ));
                }
            }
            continue;
        }

        // Deno is fetched on first connector use (not bundled). Ensure a
        // checksum-verified copy is present before spawning; a failure here
        // just falls through to the normal spawn error and backoff, which
        // re-attempts the download on the next loop (e.g. once the network is
        // back). On non-macOS this is a no-op error and spawn_deno falls back
        // to a sidecar / PATH as before.
        if let Err(e) = crate::deno_sidecar::ensure_deno().await {
            eprintln!("[connector_runtime] {}: deno not ready: {}", slug, e);
        }

        eprintln!("[connector_runtime] {}: spawning Deno subprocess", slug);
        let spawn = spawn_deno(&supervisor, &module);

        let child_result = match spawn {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[connector_runtime] {}: spawn failed: {}", slug, e);
                crashes.record_crash();
                let sleep_ms = backoff_ms;
                backoff_ms = (backoff_ms * 2).min(RESTART_BACKOFF_MAX_MS);
                // Drain inbox with `runtime_unavailable` during the sleep so
                // callers fail fast instead of hitting the bus timeout.
                let deadline = tokio::time::Instant::now() + Duration::from_millis(sleep_ms);
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        Some(msg) = inbox.recv() => {
                            let _ = msg.reply.send(ConnectorResult::err(
                                "runtime_unavailable",
                                format!("module '{}' failed to spawn: {}", slug, e),
                            ));
                        }
                    }
                }
                continue;
            }
        };

        // Spawn succeeded: reset backoff and serve jobs until the child exits.
        backoff_ms = RESTART_BACKOFF_INITIAL_MS;
        let exit_reason = serve_jobs(&slug, child_result, &mut inbox).await;
        eprintln!("[connector_runtime] {}: subprocess exited: {}", slug, exit_reason);
        crashes.record_crash();
        // Brief unconditional backoff so we don't busy-spawn if Deno
        // exits instantly.
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(RESTART_BACKOFF_MAX_MS);
    }
}

/// Construct and spawn the Deno child process. Returns the child and
/// owned stdin/stdout for IPC.
fn spawn_deno(
    supervisor: &RuntimeSupervisor,
    module: &LoadedModule,
) -> Result<SpawnedChild, String> {
    let module_ts = module.version_dir.join("module.ts");
    if !module_ts.exists() {
        return Err(format!("module.ts not found at {}", module_ts.display()));
    }
    let bin = supervisor.deno_command();
    // Phase 6: BrowserAttach modules get 127.0.0.1:9222 added to
    // --allow-net so they can talk CDP. The manifest's host_allowlist
    // is treated as immutable on disk; we extend the spawn args here.
    let mut effective_manifest = module.manifest.clone();
    if let Some(extra) = browser_attach::extra_allow_net(&module.manifest) {
        effective_manifest.host_allowlist.push(extra);
    }
    let args = deno_spawn_args(&effective_manifest, &module_ts);
    let mut cmd = Command::new(&bin);
    cmd.args(&args)
        .current_dir(&module.version_dir)
        // Never inherit ambient env. The vault_env_map below is the only
        // path tokens reach the subprocess; nothing else from the parent
        // process leaks in.
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // ── Vault → env injection (Phase 4) ──
    // For each (env_name → vault_key) the manifest declares, pull the
    // secret out of the slug-keyed vault and export it to the child.
    // Missing vault entries are skipped (the module sees the env var as
    // unset, just like before OAuth has run). Token values are never
    // logged here, even at error.
    for (env_name, vault_key) in &module.manifest.vault_env_map {
        match connector_vault::get(&module.slug, vault_key) {
            Ok(value) => { cmd.env(env_name, value); }
            Err(_) => {
                eprintln!(
                    "[connector_runtime/{}] vault key '{}' missing for env '{}'; \
                     module spawning without it",
                    module.slug, vault_key, env_name,
                );
            }
        }
    }

    let mut child = cmd.spawn().map_err(|e| format!("{}: {}", bin, e))?;
    let stdin = child.stdin.take().ok_or("no stdin handle on child")?;
    let stdout = child.stdout.take().ok_or("no stdout handle on child")?;
    let stderr = child.stderr.take();

    // Drain stderr to the logger so module panics aren't silent.
    if let Some(err) = stderr {
        let slug = module.slug.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("[connector_runtime/{}] stderr: {}", slug, line);
            }
        });
    }

    Ok(SpawnedChild { child, stdin, stdout: BufReader::new(stdout) })
}

struct SpawnedChild {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// Serve jobs from the inbox until either the child exits or the channel
/// closes. Returns a human-readable reason for the loop exit.
async fn serve_jobs(
    slug: &str,
    mut spawned: SpawnedChild,
    inbox: &mut mpsc::Receiver<InboxMsg>,
) -> String {
    let mut request_seq: u64 = 0;
    loop {
        tokio::select! {
            // Subprocess died - leave the loop so the supervisor can decide
            // whether to back off + restart.
            status = spawned.child.wait() => {
                return match status {
                    Ok(s) => format!("child exited with {:?}", s),
                    Err(e) => format!("child wait error: {}", e),
                };
            }
            // New job → write a JSON line, read a JSON line.
            msg = inbox.recv() => {
                let Some(msg) = msg else { return "inbox closed".into() };
                request_seq += 1;
                let req_id = format!("{}-{}", slug, request_seq);
                let req = WireRequest {
                    id: req_id.clone(),
                    action: &msg.job.action_id,
                    inputs: &msg.job.inputs,
                };
                let mut line = match serde_json::to_string(&req) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = msg.reply.send(ConnectorResult::err(
                            "internal_error",
                            format!("serialize request: {}", e),
                        ));
                        continue;
                    }
                };
                line.push('\n');
                if let Err(e) = spawned.stdin.write_all(line.as_bytes()).await {
                    let _ = msg.reply.send(ConnectorResult::err(
                        "module_unreachable",
                        format!("write stdin: {}", e),
                    ));
                    return format!("stdin write failed: {}", e);
                }
                let _ = spawned.stdin.flush().await;

                // Read lines until we get one paired by id. Out-of-band
                // ops (vault_put, …) are dispatched inline before the
                // response arrives.
                match read_one_response(&mut spawned.stdout, slug, &req_id).await {
                    Ok(result) => { let _ = msg.reply.send(result); }
                    Err(e) => {
                        let _ = msg.reply.send(ConnectorResult::err(
                            "module_unreachable",
                            format!("read stdout: {}", e),
                        ));
                        return format!("stdout read failed: {}", e);
                    }
                }
            }
        }
    }
}

async fn read_one_response(
    stdout: &mut BufReader<ChildStdout>,
    slug: &str,
    expected_id: &str,
) -> Result<ConnectorResult, String> {
    // Bound the number of out-of-band ops a single job can emit so a
    // misbehaving module can't keep us in this loop forever instead of
    // returning a result.
    const MAX_OPS_PER_JOB: u32 = 64;
    let mut ops_seen = 0u32;
    loop {
        let mut buf = String::new();
        match stdout.read_line(&mut buf).await {
            Ok(0) => return Err("EOF before response".into()),
            Err(e) => return Err(e.to_string()),
            Ok(_) => {
                let trimmed = buf.trim();
                if trimmed.is_empty() { continue; }
                let parsed: WireFromModule = serde_json::from_str(trimmed)
                    .map_err(|e| format!("parse '{}': {}", trimmed, e))?;
                match parsed {
                    WireFromModule::Op(op) => {
                        ops_seen += 1;
                        if ops_seen > MAX_OPS_PER_JOB {
                            return Err(format!(
                                "module emitted >{} out-of-band ops in a single job",
                                MAX_OPS_PER_JOB,
                            ));
                        }
                        handle_op(slug, op);
                        // Continue reading; we still want the job response.
                    }
                    WireFromModule::Response(wire) => {
                        if wire.id != expected_id {
                            return Err(format!(
                                "response id '{}' did not match expected '{}'",
                                wire.id, expected_id
                            ));
                        }
                        return Ok(if wire.success {
                            ConnectorResult::ok(wire.output)
                        } else {
                            ConnectorResult::err(
                                wire.error_code.unwrap_or_else(|| "unknown_error".into()),
                                wire.error_message.unwrap_or_else(|| "no message".into()),
                            )
                        });
                    }
                }
            }
        }
    }
}

/// Handle an out-of-band op from a module. Currently only vault_put.
/// All ops are scoped to the module's OWN slug - the supervisor never
/// trusts a slug from the wire. Errors are logged but do not abort
/// the running job (the module is the one that asked for the op; if
/// it fails, the module can decide what to do).
fn handle_op(slug: &str, op: WireOp) {
    match op.op.as_str() {
        "vault_put" => {
            let (Some(key), Some(value)) = (op.key, op.value) else {
                eprintln!("[connector_runtime/{}] vault_put missing key/value", slug);
                return;
            };
            // Only allow specific keys via the wire - modules don't
            // get to invent vault key names. access_token + refresh_token
            // + expires_at_unix are the standard set for OAuth refresh.
            if !matches!(key.as_str(),
                "access_token" | "refresh_token" | "expires_at_unix")
            {
                eprintln!(
                    "[connector_runtime/{}] vault_put rejected key '{}': \
                     only access_token / refresh_token / expires_at_unix allowed",
                    slug, key,
                );
                return;
            }
            if let Err(e) = crate::connector_vault::put(slug, &key, &value) {
                eprintln!("[connector_runtime/{}] vault_put failed: {}", slug, e);
            }
        }
        other => {
            eprintln!("[connector_runtime/{}] unknown op '{}'", slug, other);
        }
    }
}

// ─────────────────────────────────────────────
//  Tests
//
//  We can't depend on Deno being installed in CI, so the runtime tests
//  here cover the supervisor's bookkeeping (handles/dispatch path with
//  no module → no_module error) and rely on the connector_manifest
//  tests for spawn-args correctness. End-to-end Deno smoke testing is
//  documented as a manual acceptance step until the sidecar lands.
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatch_unknown_slug_returns_no_module() {
        let s = Arc::new(RuntimeSupervisor::new(None));
        let r = s.dispatch(&ConnectorJob {
            connector_slug: "nope".into(),
            action_id: "go".into(),
            inputs: serde_json::json!({}),
            idempotency_key: "k".into(),
            caller: "t".into(),
        }).await;
        assert_eq!(r.error_code.as_deref(), Some("no_module"));
    }

    #[test]
    fn crash_stats_open_after_threshold() {
        let mut s = CrashStats::default();
        for _ in 0..CRASH_TRIP_THRESHOLD {
            s.record_crash();
        }
        assert!(s.is_circuit_open());
    }

    #[test]
    fn crash_stats_stay_closed_under_threshold() {
        let mut s = CrashStats::default();
        for _ in 0..(CRASH_TRIP_THRESHOLD - 1) {
            s.record_crash();
        }
        assert!(!s.is_circuit_open());
    }
}
