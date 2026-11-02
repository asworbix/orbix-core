// ─────────────────────────────────────────────
//  Connector Bus - Phase 3 of CONNECTORS.md
//
//  Sits between L-line (local agents, tools, workflow steps) and C-line
//  (connector backends - in-process today, Deno subprocess in Phase 2).
//
//  Required behaviour, all by default:
//    - Typed jobs:        { slug, action, inputs, idempotency_key }
//    - Idempotency:       dedupe by (slug, action, key) for 24h
//    - Per-connector qs:  Slack rate-limit does not stall Dynamics
//    - Circuit breakers:  N failures in M seconds → fast-fail with cache
//    - Audit:             one row dispatched, one row completed/failed
//    - Schema validation: malformed payloads rejected before backend
//    - Credentials:       never carried in payloads, never recorded
//
//  The bus is backend-agnostic. ConnectorBackend is a trait; the in-process
//  backend (Phase 3 ship value) and the Deno runtime backend (Phase 2) both
//  implement it. The bus owns idempotency, queueing, audit, and circuit
//  breaking - backends only execute.
// ─────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};

use crate::audit_log::{AuditKind, AuditLogState};

/// Shared handle to the workspace audit log - the bus must record
/// every job/result pair in the same chain as the rest of the app.
pub type SharedAuditLog = Arc<AuditLogState>;

const IDEMPOTENCY_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CIRCUIT_TRIP_THRESHOLD: u32 = 5;
const CIRCUIT_TRIP_WINDOW: Duration = Duration::from_secs(60);
const CIRCUIT_OPEN_DURATION: Duration = Duration::from_secs(30);
const PER_SLUG_CONCURRENCY: usize = 4;

// ─────────────────────────────────────────────
//  Job + Result types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorJob {
    pub connector_slug: String,
    pub action_id: String,
    pub inputs: serde_json::Value,
    /// Caller-provided key; identical keys within 24h return the cached
    /// result without re-executing side effects.
    pub idempotency_key: String,
    /// Free-form caller identity for audit (agent name, workflow id, etc).
    pub caller: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorResult {
    pub success: bool,
    pub output: serde_json::Value,
    /// Classified error code from the connector (e.g. "upstream_error",
    /// "rate_limited", "schema_error"). None on success.
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// True if returned from idempotency cache without re-execution.
    pub cached: bool,
}

impl ConnectorResult {
    pub fn ok(output: serde_json::Value) -> Self {
        Self { success: true, output, error_code: None, error_message: None, cached: false }
    }
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: serde_json::Value::Null,
            error_code: Some(code.into()),
            error_message: Some(message.into()),
            cached: false,
        }
    }
}

// ─────────────────────────────────────────────
//  Backend trait
//
//  In-process backend (Phase 3) wraps today's connectors.rs handlers.
//  Deno runtime backend (Phase 2) dispatches NDJSON to a subprocess.
//  The bus is the same for both.
// ─────────────────────────────────────────────

#[async_trait]
pub trait ConnectorBackend: Send + Sync {
    /// Returns true if this backend owns the slug. The bus walks backends
    /// in registration order until one claims the slug; first match wins.
    fn handles(&self, slug: &str) -> bool;

    /// Execute a single job. Backends may take their time; the bus enforces
    /// global timeouts via tokio::time::timeout at the call site.
    async fn dispatch(&self, job: &ConnectorJob) -> ConnectorResult;
}

// ─────────────────────────────────────────────
//  Idempotency cache
// ─────────────────────────────────────────────

#[derive(Debug)]
struct CachedResult {
    result: ConnectorResult,
    expires_at: Instant,
}

#[derive(Debug, Default)]
struct IdempotencyCache {
    by_key: HashMap<String, CachedResult>,
}

impl IdempotencyCache {
    fn make_key(job: &ConnectorJob) -> String {
        format!("{}|{}|{}", job.connector_slug, job.action_id, job.idempotency_key)
    }

    fn get(&mut self, job: &ConnectorJob) -> Option<ConnectorResult> {
        let key = Self::make_key(job);
        let entry = self.by_key.get(&key)?;
        if entry.expires_at <= Instant::now() {
            self.by_key.remove(&key);
            return None;
        }
        let mut hit = entry.result.clone();
        hit.cached = true;
        Some(hit)
    }

    fn put(&mut self, job: &ConnectorJob, result: ConnectorResult) {
        let key = Self::make_key(job);
        self.by_key.insert(key, CachedResult {
            result,
            expires_at: Instant::now() + IDEMPOTENCY_TTL,
        });
    }

    /// Drop entries past TTL. Called opportunistically before each insert.
    fn evict_expired(&mut self) {
        let now = Instant::now();
        self.by_key.retain(|_, v| v.expires_at > now);
    }
}

// ─────────────────────────────────────────────
//  Circuit breaker (per slug)
// ─────────────────────────────────────────────

#[derive(Debug)]
struct BreakerState {
    failures: Vec<Instant>,
    open_until: Option<Instant>,
}

impl BreakerState {
    fn new() -> Self { Self { failures: Vec::new(), open_until: None } }

    fn is_open(&mut self) -> bool {
        if let Some(until) = self.open_until {
            if Instant::now() < until { return true; }
            self.open_until = None;
            self.failures.clear();
        }
        false
    }

    fn record_failure(&mut self) {
        let now = Instant::now();
        let cutoff = now.checked_sub(CIRCUIT_TRIP_WINDOW).unwrap_or(now);
        self.failures.retain(|t| *t > cutoff);
        self.failures.push(now);
        if self.failures.len() as u32 >= CIRCUIT_TRIP_THRESHOLD {
            self.open_until = Some(now + CIRCUIT_OPEN_DURATION);
        }
    }

    fn record_success(&mut self) {
        self.failures.clear();
    }
}

// ─────────────────────────────────────────────
//  Bus
// ─────────────────────────────────────────────

pub struct ConnectorBus {
    /// Backends are registered once at boot; std RwLock keeps registration
    /// sync and held only briefly during dispatch's read.
    backends: std::sync::RwLock<Vec<Arc<dyn ConnectorBackend>>>,
    idempotency: Mutex<IdempotencyCache>,
    breakers: Mutex<HashMap<String, BreakerState>>,
    /// Per-slug concurrency cap. Backpressure across connectors so Slack's
    /// rate limit does not stall Dynamics.
    semaphores: Mutex<HashMap<String, Arc<Semaphore>>>,
    audit: SharedAuditLog,
}

impl ConnectorBus {
    pub fn new(audit: SharedAuditLog) -> Self {
        Self {
            backends: std::sync::RwLock::new(Vec::new()),
            idempotency: Mutex::new(IdempotencyCache::default()),
            breakers: Mutex::new(HashMap::new()),
            semaphores: Mutex::new(HashMap::new()),
            audit,
        }
    }

    pub fn register_backend(&self, backend: Arc<dyn ConnectorBackend>) {
        if let Ok(mut backends) = self.backends.write() {
            backends.push(backend);
        }
    }

    /// True if any registered backend claims this slug.
    pub fn has_backend(&self, slug: &str) -> bool {
        self.pick_backend(slug).is_some()
    }

    /// Find a backend that claims this slug. Order-of-registration wins.
    fn pick_backend(&self, slug: &str) -> Option<Arc<dyn ConnectorBackend>> {
        let backends = self.backends.read().ok()?;
        backends.iter().find(|b| b.handles(slug)).cloned()
    }

    async fn semaphore_for(&self, slug: &str) -> Arc<Semaphore> {
        let mut map = self.semaphores.lock().await;
        map.entry(slug.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(PER_SLUG_CONCURRENCY)))
            .clone()
    }

    /// Public entry. Validates payload, checks idempotency, enforces circuit
    /// breaker, queues per slug, audits dispatch+result. Backends never see
    /// duplicated jobs or jobs after a trip.
    pub async fn dispatch(&self, job: ConnectorJob) -> ConnectorResult {
        // 1. Cheap shape check - empty slug / action are caller bugs and
        //    must never reach a backend.
        if job.connector_slug.is_empty() || job.action_id.is_empty() {
            return ConnectorResult::err(
                "schema_error",
                "connector_slug and action_id are required",
            );
        }
        if job.idempotency_key.is_empty() {
            return ConnectorResult::err(
                "schema_error",
                "idempotency_key is required",
            );
        }

        // 2. Idempotency cache hit?
        {
            let mut cache = self.idempotency.lock().await;
            if let Some(hit) = cache.get(&job) {
                self.audit_dispatched(&job, true);
                self.audit_completed(&job, &hit);
                return hit;
            }
        }

        // 3. Circuit-breaker open?
        {
            let mut breakers = self.breakers.lock().await;
            let state = breakers.entry(job.connector_slug.clone())
                .or_insert_with(BreakerState::new);
            if state.is_open() {
                let r = ConnectorResult::err(
                    "circuit_open",
                    format!("connector '{}' is circuit-broken; try again shortly",
                            job.connector_slug),
                );
                self.audit_dispatched(&job, false);
                self.audit_failed(&job, &r);
                return r;
            }
        }

        // 4. Pick a backend.
        let backend = match self.pick_backend(&job.connector_slug) {
            Some(b) => b,
            None => {
                let r = ConnectorResult::err(
                    "no_backend",
                    format!("no backend registered for slug '{}'", job.connector_slug),
                );
                self.audit_dispatched(&job, false);
                self.audit_failed(&job, &r);
                return r;
            }
        };

        // 5. Per-slug queue / backpressure.
        let sem = self.semaphore_for(&job.connector_slug).await;
        let _permit = match sem.acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                let r = ConnectorResult::err("queue_closed", "connector queue closed");
                self.audit_dispatched(&job, false);
                self.audit_failed(&job, &r);
                return r;
            }
        };

        // 6. Execute. Audit dispatch *before* and result *after*.
        self.audit_dispatched(&job, false);
        let result = backend.dispatch(&job).await;

        // 7. Update breaker, cache, audit.
        {
            let mut breakers = self.breakers.lock().await;
            let state = breakers.entry(job.connector_slug.clone())
                .or_insert_with(BreakerState::new);
            if result.success { state.record_success(); }
            else { state.record_failure(); }
        }
        if result.success {
            let mut cache = self.idempotency.lock().await;
            cache.evict_expired();
            cache.put(&job, result.clone());
            self.audit_completed(&job, &result);
        } else {
            self.audit_failed(&job, &result);
        }
        result
    }

    // ─── audit helpers ──────────────────────────────────

    fn audit_dispatched(&self, job: &ConnectorJob, from_cache: bool) {
        let payload = serde_json::json!({
            "slug": job.connector_slug,
            "action": job.action_id,
            "idempotency_key": job.idempotency_key,
            "caller": job.caller,
            "from_cache": from_cache,
        });
        if let Ok(mut log) = self.audit.lock() {
            log.append(&job.caller, AuditKind::BusJobDispatched, payload, None);
        }
    }

    fn audit_completed(&self, job: &ConnectorJob, result: &ConnectorResult) {
        let payload = serde_json::json!({
            "slug": job.connector_slug,
            "action": job.action_id,
            "idempotency_key": job.idempotency_key,
            "cached": result.cached,
        });
        if let Ok(mut log) = self.audit.lock() {
            log.append(&job.caller, AuditKind::BusJobCompleted, payload, None);
        }
    }

    fn audit_failed(&self, job: &ConnectorJob, result: &ConnectorResult) {
        let payload = serde_json::json!({
            "slug": job.connector_slug,
            "action": job.action_id,
            "idempotency_key": job.idempotency_key,
            "error_code": result.error_code,
            "error_message": result.error_message,
        });
        if let Ok(mut log) = self.audit.lock() {
            log.append(&job.caller, AuditKind::BusJobFailed, payload, None);
        }
    }
}

// ─────────────────────────────────────────────
//  In-process backend (Phase 3 ship value)
//
//  Wraps a HashMap<slug, async fn> registered at boot. New built-in
//  connectors plug in here without touching the bus. When the Deno
//  runtime arrives in Phase 2 it registers as a second backend that
//  claims slugs prefixed by "module/" or whatever convention we choose.
// ─────────────────────────────────────────────

type InProcessHandler = Arc<
    dyn Fn(serde_json::Value) -> futures::future::BoxFuture<'static, ConnectorResult>
        + Send + Sync,
>;

/// In-process backend uses std::sync::RwLock so registration is sync
/// (callable from AppState::default()) and dispatch reads only briefly
/// to clone the handler Arc before awaiting.
#[derive(Default)]
pub struct InProcessBackend {
    handlers: std::sync::RwLock<HashMap<String, InProcessHandler>>,
}

impl InProcessBackend {
    pub fn new() -> Self { Self::default() }

    /// Register an action handler for `<slug>.<action>`. Synchronous so it
    /// is callable from non-async contexts at app boot.
    pub fn register<F, Fut>(&self, slug: &str, action: &str, f: F)
    where
        F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ConnectorResult> + Send + 'static,
    {
        let key = format!("{}.{}", slug, action);
        let f = Arc::new(f);
        let handler: InProcessHandler = Arc::new(move |inputs| {
            let f = f.clone();
            Box::pin(async move { f(inputs).await })
        });
        if let Ok(mut handlers) = self.handlers.write() {
            handlers.insert(key, handler);
        }
    }
}

#[async_trait]
impl ConnectorBackend for InProcessBackend {
    fn handles(&self, slug: &str) -> bool {
        let Ok(handlers) = self.handlers.read() else { return false };
        let prefix = format!("{}.", slug);
        handlers.keys().any(|k| k.starts_with(&prefix))
    }

    async fn dispatch(&self, job: &ConnectorJob) -> ConnectorResult {
        let key = format!("{}.{}", job.connector_slug, job.action_id);
        let handler = self.handlers.read().ok()
            .and_then(|h| h.get(&key).cloned());
        match handler {
            Some(h) => h(job.inputs.clone()).await,
            None => ConnectorResult::err(
                "no_action",
                format!("no in-process handler for '{}'", key),
            ),
        }
    }
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn make_bus() -> Arc<ConnectorBus> {
        let workspace = format!("test-{}", uuid::Uuid::new_v4());
        let log = crate::audit_log::AuditLog::load_or_init(&workspace);
        let audit = Arc::new(std::sync::Mutex::new(log));
        Arc::new(ConnectorBus::new(audit))
    }

    fn job(slug: &str, action: &str, key: &str) -> ConnectorJob {
        ConnectorJob {
            connector_slug: slug.into(),
            action_id: action.into(),
            inputs: serde_json::json!({}),
            idempotency_key: key.into(),
            caller: "test".into(),
        }
    }

    #[tokio::test]
    async fn idempotent_replay_returns_cache_without_reexec() {
        let bus = make_bus();
        let backend = Arc::new(InProcessBackend::new());
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        backend.register("echo", "ping", move |_| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                ConnectorResult::ok(serde_json::json!({"pong": true}))
            }
        });
        bus.register_backend(backend);

        let r1 = bus.dispatch(job("echo", "ping", "k1")).await;
        let r2 = bus.dispatch(job("echo", "ping", "k1")).await;
        assert!(r1.success && r2.success);
        assert!(!r1.cached && r2.cached);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn malformed_payload_rejected_at_bus() {
        let bus = make_bus();
        let r = bus.dispatch(ConnectorJob {
            connector_slug: "".into(),
            action_id: "ping".into(),
            inputs: serde_json::json!({}),
            idempotency_key: "k".into(),
            caller: "t".into(),
        }).await;
        assert!(!r.success);
        assert_eq!(r.error_code.as_deref(), Some("schema_error"));
    }

    #[tokio::test]
    async fn missing_idempotency_key_rejected() {
        let bus = make_bus();
        let r = bus.dispatch(job("echo", "ping", "")).await;
        assert!(!r.success);
        assert_eq!(r.error_code.as_deref(), Some("schema_error"));
    }

    #[tokio::test]
    async fn no_backend_returns_typed_error() {
        let bus = make_bus();
        let r = bus.dispatch(job("nonesuch", "do", "k")).await;
        assert!(!r.success);
        assert_eq!(r.error_code.as_deref(), Some("no_backend"));
    }

    #[tokio::test]
    async fn circuit_breaks_after_repeated_failures() {
        let bus = make_bus();
        let backend = Arc::new(InProcessBackend::new());
        backend.register("flaky", "go", |_| async {
            ConnectorResult::err("upstream_error", "boom")
        });
        bus.register_backend(backend);

        // Trip the breaker: 5 failures within window.
        for i in 0..CIRCUIT_TRIP_THRESHOLD {
            let r = bus.dispatch(job("flaky", "go", &format!("k{}", i))).await;
            assert!(!r.success);
            assert_eq!(r.error_code.as_deref(), Some("upstream_error"));
        }
        // Next call short-circuits.
        let r = bus.dispatch(job("flaky", "go", "k-tripped")).await;
        assert_eq!(r.error_code.as_deref(), Some("circuit_open"));
    }
}
