//! Egress Gateway (W4): the single door out of the machine.
//!
//! Every web search, web fetch, and cloud model call funnels through this
//! module. Before anything leaves, the gateway applies the workspace
//! CompliancePolicy (residency and boundary gates) and the secure-relay
//! redaction engine (PII stripping) and records the decision in the
//! tamper-evident audit log. No other module may make an outbound search
//! or model HTTP request; the previously-isolated relay, compliance, and
//! data-control components become enforced here instead of advisory.

use crate::compliance::{CompliancePolicy, RedactionMode};
use crate::secure_relay::{sanitise, RedactionPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EgressKind {
    /// A search query leaving for an external search endpoint (global).
    WebSearch,
    /// A URL fetch to an arbitrary external host (global).
    WebFetch,
    /// A model call to a cloud inference provider. EU-routable via
    /// Vertex AI Frankfurt, so residency blocks reroute instead.
    CloudInference,
}

impl EgressKind {
    fn label(self) -> &'static str {
        match self {
            EgressKind::WebSearch => "web_search",
            EgressKind::WebFetch => "web_fetch",
            EgressKind::CloudInference => "cloud_inference",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EgressDecision {
    pub allowed: bool,
    pub reason: String,
    /// The text to actually send: sanitised when redaction applies,
    /// verbatim otherwise. Empty when blocked.
    pub text: String,
    pub redaction_count: u32,
}

/// The compliance + redaction gate. Pure so it is unit-testable: takes a
/// policy snapshot and the outbound text, returns what (if anything) may
/// leave the machine.
///
/// Semantics, matching the policy structs' own documentation:
/// - `compliance_mode` off is draft mode: everything is allowed verbatim,
///   but callers still audit-log the egress.
/// - `eu_boundary` + `residency.hard_gate`: search and fetch leave ONLY
///   after forced strict redaction - never verbatim, regardless of the
///   redaction mode. They used to be blocked outright, which killed
///   Bob's entire live-info ability the moment EU compliance became the
///   built-in default (a field failure, caught on camera 2026-07-31:
///   "the web search failed due to a network restriction"). A redacted
///   query is the boundary the policy's own Residency doc describes;
///   CloudInference stays allowed because `resolve_cloud_route` reroutes
///   it to Vertex AI in the EU.
/// - `eu_boundary` without `hard_gate`: "the relay attempts redaction
///   first" (Residency doc), so search text is sanitised and allowed.
/// - `redaction` Mask/Strict sanitises outbound search queries and cloud
///   inference text; Off sends verbatim.
pub(crate) fn gate(policy: &CompliancePolicy, kind: EgressKind, text: &str) -> EgressDecision {
    if !policy.toggles.compliance_mode {
        return EgressDecision {
            allowed: true,
            reason: "draft mode: compliance gates off".to_string(),
            text: text.to_string(),
            redaction_count: 0,
        };
    }

    let eu_locked = policy.toggles.eu_boundary;
    if eu_locked && policy.residency.hard_gate && kind != EgressKind::CloudInference {
        let result = sanitise(text, &RedactionPolicy::default());
        return EgressDecision {
            allowed: true,
            reason: format!(
                "EU boundary, hard gate: {} sent with forced strict redaction ({} redaction(s))",
                kind.label(),
                result.redaction_count
            ),
            text: result.sanitised_text,
            redaction_count: result.redaction_count,
        };
    }

    let must_redact = eu_locked || !matches!(policy.redaction, RedactionMode::Off);
    if must_redact {
        let result = sanitise(text, &RedactionPolicy::default());
        EgressDecision {
            allowed: true,
            reason: if result.redaction_count > 0 {
                format!("{} redaction(s) applied before egress", result.redaction_count)
            } else {
                "no sensitive content detected".to_string()
            },
            text: result.sanitised_text,
            redaction_count: result.redaction_count,
        }
    } else {
        EgressDecision {
            allowed: true,
            reason: "redaction off".to_string(),
            text: text.to_string(),
            redaction_count: 0,
        }
    }
}

/// Audit one egress decision into the hash-chained audit log.
fn audit(state: &crate::AppState, kind: EgressKind, decision: &EgressDecision, target: &str) {
    crate::run_log::egress(
        decision.allowed,
        format!(
            "{} {} to {}: {}",
            kind.label(),
            if decision.allowed { "allowed" } else { "blocked" },
            target,
            decision.reason
        ),
    );
    if let Ok(mut log) = state.core.audit_log.lock() {
        log.append(
            "egress",
            if decision.allowed {
                crate::audit_log::AuditKind::ActionExecuted
            } else {
                crate::audit_log::AuditKind::ActionDenied
            },
            serde_json::json!({
                "kind": kind.label(),
                "target": target,
                "allowed": decision.allowed,
                "reason": decision.reason,
                "redactions": decision.redaction_count,
            }),
            None,
        );
    }
}

fn policy_snapshot(state: &crate::AppState) -> CompliancePolicy {
    state
        .core
        .compliance
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

// ─────────────────────────────────────────────
//  Web search funnel
// ─────────────────────────────────────────────

/// The one and only web search path. Applies the gate (redaction +
/// residency) to the query, then queries DuckDuckGo over plain reqwest
/// (no shell, no curl: the old shell pipeline was an injection surface)
/// and returns parsed results as {title, url, snippet} objects.
pub(crate) async fn web_search(
    state: &crate::AppState,
    query: &str,
    max_results: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let policy = policy_snapshot(state);
    let decision = gate(&policy, EgressKind::WebSearch, query);
    audit(state, EgressKind::WebSearch, &decision, "duckduckgo.com");
    if !decision.allowed {
        return Err(format!("Egress blocked: {}", decision.reason));
    }
    let outbound = decision.text.clone();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    // DDG rate-limits bursts of automated queries (field failure
    // 2026-08-01: one rate-limit ended the whole turn). Three layers keep
    // that rare without fighting anyone's bot detection: a short results
    // cache (repeat queries never re-hit the network), polite pacing (a
    // global minimum gap between DDG requests), and one paced retry.
    if let Some(hit) = search_cache_get(&outbound) {
        return Ok(hit);
    }
    search_pace().await;
    match search_duckduckgo(&client, &outbound, max_results).await {
        Ok(r) => {
            search_cache_put(&outbound, &r);
            Ok(r)
        }
        Err(first_err) => {
            // Mechanical fallback: Google News RSS is keyless, bot-friendly,
            // and returns dated headlines with sources - real information
            // even while the general search provider is rate-limiting.
            // (Advisory \"try web_fetch instead\" hints proved too weak for
            // small models - field failure 2026-08-01.)
            audit(state, EgressKind::WebSearch, &decision, "news.google.com");
            if let Ok(results) = search_google_news(&client, &outbound, max_results).await {
                if !results.is_empty() {
                    search_cache_put(&outbound, &results);
                    return Ok(results);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            match ddg_parse_endpoint(&client, "https://lite.duckduckgo.com/lite/?q=", &outbound, max_results).await {
                Ok(results) if !results.is_empty() => {
                    search_cache_put(&outbound, &results);
                    Ok(results)
                }
                _ => Err(first_err),
            }
        }
    }
}

/// Google News RSS fallback: current headlines with source and date per
/// item. News-centric, so it only runs when the general provider fails -
/// dated headlines beat an error for almost any query about the world.
async fn search_google_news(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let url = format!(
        "https://news.google.com/rss/search?q={}&hl=en-US&gl=US&ceid=US:en",
        urlencoding::encode(query)
    );
    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh) OrbixCore/1.0")
        .send()
        .await
        .map_err(|e| format!("News feed request failed: {}", e))?;
    let xml = resp.text().await.map_err(|e| format!("News feed read failed: {}", e))?;

    let re_item = regex::Regex::new(r"(?s)<item>(.*?)</item>").unwrap();
    let field = |block: &str, tag: &str| -> String {
        let re = regex::Regex::new(&format!(r"(?s)<{tag}[^>]*>(.*?)</{tag}>")).unwrap();
        re.captures(block)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .replace("<![CDATA[", "")
            .replace("]]>", "")
            .trim()
            .to_string()
    };

    let mut results = Vec::new();
    for cap in re_item.captures_iter(&xml).take(max_results) {
        let block = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title = field(block, "title");
        let link = field(block, "link");
        let source = field(block, "source");
        let date = field(block, "pubDate");
        if title.is_empty() || !link.starts_with("http") {
            continue;
        }
        let snippet = format!("{} ({})", source, date);
        results.push(serde_json::json!({ "title": title, "url": link, "snippet": snippet }));
    }
    Ok(results)
}

/// Cache of recent search results (query -> results, 10 min TTL) plus the
/// instant of the last outbound search for pacing.
static SEARCH_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, (i64, Vec<serde_json::Value>)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
static LAST_SEARCH_AT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn search_cache_get(query: &str) -> Option<Vec<serde_json::Value>> {
    let cache = SEARCH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let (at, results) = cache.get(query)?;
    if chrono::Utc::now().timestamp() - at < 600 {
        Some(results.clone())
    } else {
        None
    }
}

fn search_cache_put(query: &str, results: &[serde_json::Value]) {
    if results.is_empty() {
        return;
    }
    let mut cache = SEARCH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() > 100 {
        cache.clear();
    }
    cache.insert(query.to_string(), (chrono::Utc::now().timestamp(), results.to_vec()));
}

/// Enforce a 2s minimum gap between outbound searches - a courtesy that
/// keeps agentic multi-search runs from tripping provider rate limits.
async fn search_pace() {
    use std::sync::atomic::Ordering;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let last = LAST_SEARCH_AT.load(Ordering::SeqCst);
    let elapsed = now_ms - last;
    if elapsed < 2000 {
        tokio::time::sleep(std::time::Duration::from_millis((2000 - elapsed) as u64)).await;
    }
    LAST_SEARCH_AT.store(chrono::Utc::now().timestamp_millis(), Ordering::SeqCst);
}

/// DDG HTML endpoint + parser. Private: callable only through web_search.
async fn search_duckduckgo(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<serde_json::Value>, String> {
    // Primary: the html endpoint. When it anomaly-challenges or returns
    // nothing (field failure 2026-07-31: an allowed search came back
    // empty and Bob had nothing to say), fall back to the lite endpoint,
    // whose simpler markup survives DDG's bot heuristics more often.
    if let Ok(results) = ddg_parse_endpoint(client, "https://html.duckduckgo.com/html/?q=", query, max_results).await {
        if !results.is_empty() {
            return Ok(results);
        }
    }
    match ddg_parse_endpoint(client, "https://lite.duckduckgo.com/lite/?q=", query, max_results).await {
        Ok(results) if !results.is_empty() => Ok(results),
        Ok(_) => Err(
            "The search provider returned no results - it may be rate-limiting automated \
             queries right now. Try again in a minute, or rephrase the query."
                .to_string(),
        ),
        Err(e) => Err(e),
    }
}

/// Fetch one DDG endpoint and parse whatever result markup it uses.
/// The two endpoints differ (result__a anchors vs result-link anchors),
/// so both patterns are tried on every body.
async fn ddg_parse_endpoint(
    client: &reqwest::Client,
    base: &str,
    query: &str,
    max_results: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let url = format!("{}{}", base, urlencoding::encode(query));
    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh) OrbixCore/1.0")
        .send()
        .await
        .map_err(|e| format!("Search request failed: {}", e))?;
    let status = resp.status().as_u16();
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Search read failed: {}", e))?;
    if status != 200 && !html.contains("result") {
        return Ok(Vec::new());
    }

    let re_html =
        regex::Regex::new(r#"class="result__a"[^>]*href="([^"]*)"[^>]*>([^<]*)</a>"#).unwrap();
    let re_lite =
        regex::Regex::new(r#"rel="nofollow"[^>]*href="([^"]*)"[^>]*>([^<]+)</a>"#).unwrap();
    let re_snippet = regex::Regex::new(
        r#"class=['"]result[_-]{1,2}snippet['"][^>]*>([\s\S]*?)</(?:a|td)>"#,
    )
    .unwrap();
    let re_tags = regex::Regex::new(r"<[^>]+>").unwrap();

    let snippet_matches: Vec<_> = re_snippet.captures_iter(&html).collect();
    let caps: Vec<regex::Captures> = {
        let primary: Vec<_> = re_html.captures_iter(&html).collect();
        if primary.is_empty() {
            re_lite.captures_iter(&html).collect()
        } else {
            primary
        }
    };
    let mut results = Vec::new();
    for (i, cap) in caps.into_iter().enumerate().take(max_results) {
        let raw_url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim();
        // DDG wraps URLs in a redirect: extract the real target
        let actual_url = if raw_url.contains("uddg=") {
            raw_url
                .split("uddg=")
                .nth(1)
                .and_then(|u| urlencoding::decode(u).ok())
                .map(|u| u.into_owned())
                .unwrap_or_else(|| raw_url.to_string())
        } else {
            raw_url.to_string()
        };
        let clean_url = actual_url.split('&').next().unwrap_or(&actual_url).to_string();
        if !clean_url.starts_with("http") {
            continue;
        }
        let snippet = snippet_matches
            .get(i)
            .and_then(|s| s.get(1))
            .map(|m| re_tags.replace_all(m.as_str(), "").trim().to_string())
            .unwrap_or_default();
        if !title.is_empty() {
            results.push(serde_json::json!({
                "title": title,
                "url": clean_url,
                "snippet": if snippet.len() > 500 {
                    format!("{}...", &snippet[..500])
                } else {
                    snippet
                },
            }));
        }
    }
    Ok(results)
}

// ─────────────────────────────────────────────
//  Web fetch funnel
// ─────────────────────────────────────────────

/// The one and only external URL fetch path. The URL is gated and the
/// decision audited (under the EU hard gate the URL text passes the
/// forced redaction check; a URL that redacts to something else fetches
/// nothing and errors naturally).
pub(crate) async fn web_fetch(
    state: &crate::AppState,
    url: &str,
) -> Result<(u16, String), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    let policy = policy_snapshot(state);
    // Gate on the URL text: blocked terms and custom patterns in the URL
    // (project codenames in a query string, for example) block the fetch
    // rather than silently leaking. A rewritten URL is useless, so any
    // redaction hit means refusal instead of substitution.
    let decision = gate(&policy, EgressKind::WebFetch, url);
    audit(state, EgressKind::WebFetch, &decision, url);
    if !decision.allowed {
        return Err(format!("Egress blocked: {}", decision.reason));
    }
    if decision.redaction_count > 0 {
        return Err(format!(
            "Egress blocked: the URL contains {} redactable item(s); refusing to send it",
            decision.redaction_count
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .header("User-Agent", "OrbixCore/1.0")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status().as_u16();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    Ok((status, body))
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn policy(compliance: bool, eu: bool, hard: bool, redact_off: bool) -> CompliancePolicy {
        let mut p = CompliancePolicy::load_or_default("test-ws", "test@local");
        p.toggles.compliance_mode = compliance;
        p.toggles.eu_boundary = eu;
        p.residency.hard_gate = hard;
        if redact_off {
            p.redaction = RedactionMode::Off;
        }
        p
    }

    #[test]
    fn draft_mode_allows_verbatim() {
        let d = gate(&policy(false, true, true, false), EgressKind::WebSearch, "find alice@corp.com");
        assert!(d.allowed);
        assert_eq!(d.text, "find alice@corp.com");
    }

    /// W4 acceptance, revised: the EU hard gate forces strict redaction
    /// on search/fetch instead of blocking them - unredacted text never
    /// leaves, but Bob keeps his live-info ability under the built-in
    /// EU compliance defaults.
    #[test]
    fn eu_hard_gate_redacts_search_and_fetch() {
        let p = policy(true, true, true, true); // redaction Off must NOT bypass the forced pass
        let s = gate(&p, EgressKind::WebSearch, "salary for alice@corp.com");
        assert!(s.allowed);
        assert!(!s.text.contains("alice@corp.com"), "PII must not egress: {}", s.text);
        assert!(s.redaction_count >= 1);
        assert!(s.reason.contains("EU boundary"));
        let f = gate(&p, EgressKind::WebFetch, "https://x.com");
        assert!(f.allowed);
        // Inference stays allowed: it reroutes to the EU instead.
        assert!(gate(&p, EgressKind::CloudInference, "anything").allowed);
    }

    /// W4 acceptance: a PII-laden query egresses only redacted text.
    #[test]
    fn pii_query_is_redacted() {
        let p = policy(true, false, false, false);
        let d = gate(
            &p,
            EgressKind::WebSearch,
            "salary details for alice@corporation.com and +45 12 34 56 78",
        );
        assert!(d.allowed);
        assert!(d.redaction_count >= 2, "expected email+phone redacted, got {}", d.redaction_count);
        assert!(!d.text.contains("alice@corporation.com"));
        assert!(!d.text.contains("12 34 56 78"));
        assert!(d.text.contains("[REDACTED-"));
    }

    #[test]
    fn eu_soft_gate_redacts_instead_of_blocking() {
        let p = policy(true, true, false, true); // eu on, hard off, redaction Off
        let d = gate(&p, EgressKind::WebSearch, "mail bob@firm.dk about the deal");
        assert!(d.allowed);
        // eu_boundary forces sanitising even with redaction Off
        assert!(!d.text.contains("bob@firm.dk"));
    }
}

// ─────────────────────────────────────────────
//  Cloud inference residency (W4)
// ─────────────────────────────────────────────

/// Base URL for the Anthropic API. Overridable via ORBIX_ANTHROPIC_BASE
/// (e.g. a proxy or the relay), so the endpoint is not hardwired at the
/// call sites (W4). No trailing slash.
pub(crate) fn anthropic_base() -> String {
    std::env::var("ORBIX_ANTHROPIC_BASE")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string())
}
