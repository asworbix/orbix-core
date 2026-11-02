// ─────────────────────────────────────────────
//  Browser-Attach Driver - Phase 6 of CONNECTORS.md
//
//  Drives the user's primary Chrome via Chrome DevTools Protocol over
//  HTTP/WebSocket. §12 #1 was decided in favour of the primary browser
//  (best UX, "I'm already logged in" magic) rather than a dedicated
//  Orbix profile. Chrome only exposes its CDP port when launched with
//      open -a "Google Chrome" --args --remote-debugging-port=9222
//
//  Phase 6 ships:
//    - probe(): is Chrome reachable at 127.0.0.1:9222?
//    - list_tabs(): every tab the local Chrome currently shows
//    - find_matching_tabs(cookie_domains): only those whose URL host
//      matches one of the manifest's cookie_domains - the bus refuses
//      to hand any other tab id to a BrowserAttach module
//    - session_probe(manifest): combines the two; this is what Bob's
//      authoring loop calls in §8 step 4 to skip OAuth when an
//      existing session is detected
//
//  Phase 6 does NOT yet wire CDP eval through to module subprocesses
//  (that's the SDK shim work). Modules get a session signal, not yet
//  the ability to drive the tab. The supervisor already pre-authorises
//  127.0.0.1:9222 when the manifest declares Driver::BrowserAttach.
// ─────────────────────────────────────────────

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::connector_manifest::{AuthSpec, ConnectorManifest, Driver};

const CDP_DEFAULT_PORT: u16 = 9222;
const CDP_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    /// CDP type - "page", "iframe", "service_worker", "background_page".
    /// The bus only ever surfaces "page" tabs to modules.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub websocket_debugger_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionProbe {
    pub slug: String,
    pub cdp_reachable: bool,
    pub cookie_domains: Vec<String>,
    /// Matching tabs only - never the full tab listing, so a module
    /// cannot enumerate the user's other tabs from the probe result.
    pub matching_tabs: Vec<TabInfo>,
}

impl SessionProbe {
    pub fn has_active_session(&self) -> bool {
        !self.matching_tabs.is_empty()
    }
}

// ─────────────────────────────────────────────
//  CDP HTTP endpoints
// ─────────────────────────────────────────────

fn cdp_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

/// True if the CDP HTTP root responds. Cheap; ~10ms when Chrome runs
/// with the flag, fails fast when it doesn't.
pub async fn probe(port: u16) -> bool {
    let url = format!("{}/json/version", cdp_base_url(port));
    let client = match reqwest::Client::builder().timeout(CDP_TIMEOUT).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

pub async fn list_tabs(port: u16) -> Result<Vec<TabInfo>, String> {
    let url = format!("{}/json", cdp_base_url(port));
    let client = reqwest::Client::builder().timeout(CDP_TIMEOUT).build()
        .map_err(|e| format!("client: {}", e))?;
    let resp = client.get(&url).send().await
        .map_err(|e| format!("CDP request: {}. Launch Chrome with --remote-debugging-port=9222", e))?;
    if !resp.status().is_success() {
        return Err(format!("CDP HTTP {}", resp.status().as_u16()));
    }
    let tabs: Vec<TabInfo> = resp.json().await
        .map_err(|e| format!("parse CDP response: {}", e))?;
    Ok(tabs)
}

/// Extract the host from a tab URL, lowercased. Returns None for
/// chrome://, about:, file://, etc.
pub fn tab_host(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let host_with_port = rest.split('/').next()?;
    let host = host_with_port.split(':').next()?;
    if host.is_empty() { None } else { Some(host.to_string()) }
}

/// True if `host` ends with any of `cookie_domains` on a label boundary.
/// `cookie_domains = ["linkedin.com"]` matches "www.linkedin.com" and
/// "linkedin.com" but not "evil-linkedin.com".
pub fn host_matches(host: &str, cookie_domains: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    for d in cookie_domains {
        let d = d.to_ascii_lowercase();
        if host == d { return true; }
        if host.ends_with(&format!(".{}", d)) { return true; }
    }
    false
}

pub fn filter_matching_tabs(tabs: &[TabInfo], cookie_domains: &[String]) -> Vec<TabInfo> {
    tabs.iter()
        .filter(|t| t.kind == "page")
        .filter(|t| match tab_host(&t.url) {
            Some(h) => host_matches(&h, cookie_domains),
            None => false,
        })
        .cloned()
        .collect()
}

// ─────────────────────────────────────────────
//  High-level probe used by Bob's authoring loop
// ─────────────────────────────────────────────

/// For a BrowserAttach manifest, check whether the user already has a
/// matching session open. Returns a SessionProbe with cdp_reachable=false
/// if Chrome isn't running with the debug flag - Bob can then prompt the
/// user to relaunch Chrome (or fall back to OAuth if the manifest also
/// declares an OAuth2 AuthSpec).
pub async fn session_probe(manifest: &ConnectorManifest) -> Result<SessionProbe, String> {
    let cookie_domains = match (&manifest.driver, &manifest.auth) {
        (Driver::BrowserAttach { .. }, AuthSpec::BrowserSession { cookie_domains }) => {
            cookie_domains.clone()
        }
        (Driver::BrowserAttach { .. }, _) => {
            return Err(
                "BrowserAttach driver requires AuthSpec::BrowserSession with cookie_domains"
                    .into(),
            );
        }
        _ => return Err("manifest does not use the BrowserAttach driver".into()),
    };
    if cookie_domains.is_empty() {
        return Err("cookie_domains is empty; refusing to probe every tab".into());
    }

    if !probe(CDP_DEFAULT_PORT).await {
        return Ok(SessionProbe {
            slug: manifest.slug.clone(),
            cdp_reachable: false,
            cookie_domains,
            matching_tabs: Vec::new(),
        });
    }

    let tabs = list_tabs(CDP_DEFAULT_PORT).await.unwrap_or_default();
    let matching = filter_matching_tabs(&tabs, &cookie_domains);
    Ok(SessionProbe {
        slug: manifest.slug.clone(),
        cdp_reachable: true,
        cookie_domains,
        matching_tabs: matching,
    })
}

// ─────────────────────────────────────────────
//  Runtime hook - extra spawn flag for BrowserAttach modules
// ─────────────────────────────────────────────

/// When the manifest uses Driver::BrowserAttach, the connector-agent
/// needs to reach CDP at 127.0.0.1:9222 in addition to whatever else
/// is in host_allowlist. The runtime calls this to compute the
/// effective allow-net.
pub fn extra_allow_net(manifest: &ConnectorManifest) -> Option<String> {
    matches!(manifest.driver, Driver::BrowserAttach { .. })
        .then(|| format!("127.0.0.1:{}", CDP_DEFAULT_PORT))
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn browser_attach_manifest(cookie_domains: &[&str]) -> ConnectorManifest {
        ConnectorManifest {
            slug: "linkedin-attach".into(),
            version: "0.1.0".into(),
            display_name: "LinkedIn (browser)".into(),
            vendor: "linkedin.com".into(),
            driver: Driver::BrowserAttach { profile_hint: None },
            auth: AuthSpec::BrowserSession {
                cookie_domains: cookie_domains.iter().map(|s| s.to_string()).collect(),
            },
            host_allowlist: Vec::new(),
            env_allowlist: Vec::new(),
            vault_env_map: BTreeMap::new(),
            capabilities: Vec::new(),
            actions: BTreeMap::new(),
            events: BTreeMap::new(),
            resource_limits: Default::default(),
            signature: None,
        }
    }

    #[test]
    fn tab_host_strips_scheme_path_port() {
        assert_eq!(tab_host("https://www.linkedin.com/feed/").as_deref(), Some("www.linkedin.com"));
        assert_eq!(tab_host("http://localhost:3000/x").as_deref(), Some("localhost"));
        assert_eq!(tab_host("chrome://settings/").as_deref(), None);
        assert_eq!(tab_host("about:blank").as_deref(), None);
        assert_eq!(tab_host("file:///etc/passwd").as_deref(), None);
    }

    #[test]
    fn host_matches_is_label_bounded() {
        // Critical: prevents an attacker tab at "evil-linkedin.com" from
        // matching a "linkedin.com" cookie domain.
        let domains = vec!["linkedin.com".into()];
        assert!(host_matches("linkedin.com", &domains));
        assert!(host_matches("www.linkedin.com", &domains));
        assert!(host_matches("api.linkedin.com", &domains));
        assert!(!host_matches("evil-linkedin.com", &domains));
        assert!(!host_matches("linkedin.com.evil.com", &domains));
        assert!(!host_matches("google.com", &domains));
    }

    #[test]
    fn filter_matching_tabs_keeps_only_pages_in_cookie_domains() {
        let tabs = vec![
            TabInfo { id: "1".into(), url: "https://www.linkedin.com/feed/".into(),
                title: "Feed".into(), kind: "page".into(), websocket_debugger_url: None },
            TabInfo { id: "2".into(), url: "https://gmail.com/inbox".into(),
                title: "Inbox".into(), kind: "page".into(), websocket_debugger_url: None },
            TabInfo { id: "3".into(), url: "https://www.linkedin.com/sw.js".into(),
                title: "sw".into(), kind: "service_worker".into(),
                websocket_debugger_url: None },
            TabInfo { id: "4".into(), url: "https://api.linkedin.com/v2/me".into(),
                title: "API".into(), kind: "page".into(), websocket_debugger_url: None },
        ];
        let domains = vec!["linkedin.com".into()];
        let kept = filter_matching_tabs(&tabs, &domains);
        let ids: Vec<&str> = kept.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "4"]);
    }

    #[test]
    fn extra_allow_net_only_for_browser_attach() {
        let m = browser_attach_manifest(&["linkedin.com"]);
        assert_eq!(extra_allow_net(&m).as_deref(), Some("127.0.0.1:9222"));

        let mut api = m;
        api.driver = Driver::Api;
        assert_eq!(extra_allow_net(&api), None);
    }

    #[tokio::test]
    async fn session_probe_rejects_empty_cookie_domains() {
        let m = browser_attach_manifest(&[]);
        let r = session_probe(&m).await.unwrap_err();
        assert!(r.contains("cookie_domains is empty"));
    }

    #[tokio::test]
    async fn session_probe_rejects_wrong_driver() {
        let mut m = browser_attach_manifest(&["linkedin.com"]);
        m.driver = Driver::Api;
        let r = session_probe(&m).await.unwrap_err();
        assert!(r.contains("BrowserAttach driver"));
    }

    #[tokio::test]
    async fn session_probe_marks_cdp_unreachable_when_chrome_off() {
        // Assumes the test runner is NOT running Chrome with the debug
        // flag on port 9222. If you have Chrome open with the flag,
        // this test will be skipped (cdp_reachable=true is still valid).
        let m = browser_attach_manifest(&["linkedin.com"]);
        let p = session_probe(&m).await.unwrap();
        if !p.cdp_reachable {
            assert!(p.matching_tabs.is_empty());
        }
    }
}

// ─────────────────────────────────────────────
//  CDP driver (W5.4): drive the user's real Chrome
//
//  browser_attach could list tabs but not act on them. This adds a
//  minimal Chrome DevTools Protocol client over the tab's WebSocket so
//  the app (and browser connectors) can navigate, click, type, and read
//  the *real* browser the user is signed into - no headless feature, no
//  separate Chrome launch. The user starts Chrome once with
//  --remote-debugging-port=9222; thereafter these primitives work.
//
//  Click/type/read go through Runtime.evaluate (JS in the page) rather
//  than synthetic Input events: simpler, and enough for automation. The
//  primitives are generic (CSS-selector based) so any browser task
//  inherits them.
// ─────────────────────────────────────────────

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// One CDP request/response round-trip over a tab's WebSocket. Opens a
/// fresh connection, sends `{id, method, params}`, and returns the first
/// matching `result`. Short-lived so we never hold a socket across calls.
async fn cdp_call(ws_url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| format!("CDP connect: {}. Launch Chrome with --remote-debugging-port=9222", e))?;
    let id: u64 = 1;
    let req = serde_json::json!({ "id": id, "method": method, "params": params });
    ws.send(WsMessage::Text(req.to_string()))
        .await
        .map_err(|e| format!("CDP send: {}", e))?;

    // Read messages until we see our id (Chrome interleaves events).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("CDP timeout".to_string());
        }
        let msg = match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(e))) => return Err(format!("CDP recv: {}", e)),
            Ok(None) => return Err("CDP socket closed".to_string()),
            Err(_) => return Err("CDP timeout".to_string()),
        };
        let WsMessage::Text(txt) = msg else { continue };
        let v: serde_json::Value = match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
            if let Some(err) = v.get("error") {
                return Err(format!("CDP error: {}", err));
            }
            return Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
        }
    }
}

/// Resolve a page tab's WebSocket debugger URL by tab id (or the first
/// page tab when `tab_id` is None).
async fn resolve_ws_url(port: u16, tab_id: Option<&str>) -> Result<String, String> {
    let tabs = list_tabs(port).await?;
    let tab = match tab_id {
        Some(id) => tabs.iter().find(|t| t.id == id),
        None => tabs.iter().find(|t| t.kind == "page"),
    }
    .ok_or("no matching page tab")?;
    tab.websocket_debugger_url
        .clone()
        .ok_or_else(|| "tab has no debugger URL".to_string())
}

/// Navigate a tab to `url` (Page.navigate).
pub async fn drive_navigate(port: u16, tab_id: Option<&str>, url: &str) -> Result<String, String> {
    let ws = resolve_ws_url(port, tab_id).await?;
    let r = cdp_call(&ws, "Page.navigate", serde_json::json!({ "url": url })).await?;
    Ok(r.get("frameId").and_then(|x| x.as_str()).unwrap_or("").to_string())
}

/// Evaluate JavaScript in the page and return its result as a JSON value
/// (Runtime.evaluate with returnByValue).
pub async fn drive_evaluate(port: u16, tab_id: Option<&str>, expression: &str) -> Result<serde_json::Value, String> {
    let ws = resolve_ws_url(port, tab_id).await?;
    let r = cdp_call(&ws, "Runtime.evaluate", serde_json::json!({
        "expression": expression,
        "returnByValue": true,
        "awaitPromise": true,
    })).await?;
    if let Some(exc) = r.get("exceptionDetails") {
        return Err(format!("JS exception: {}", exc.get("text").and_then(|t| t.as_str()).unwrap_or("error")));
    }
    Ok(r.pointer("/result/value").cloned().unwrap_or(serde_json::Value::Null))
}

/// JS string literal escape so a user-supplied selector/text cannot break
/// out of the expression.
pub(crate) fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\x3C"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Click the first element matching `selector`. Errors if not found.
pub async fn drive_click(port: u16, tab_id: Option<&str>, selector: &str) -> Result<(), String> {
    let expr = format!(
        "(() => {{ const el = document.querySelector({}); if (!el) return '__notfound__'; el.click(); return 'ok'; }})()",
        js_str(selector)
    );
    match drive_evaluate(port, tab_id, &expr).await? {
        serde_json::Value::String(s) if s == "ok" => Ok(()),
        _ => Err(format!("element not found: {}", selector)),
    }
}

/// Set the value of an input/textarea matching `selector` and fire input +
/// change events so frameworks (React, etc.) notice.
pub async fn drive_type(port: u16, tab_id: Option<&str>, selector: &str, text: &str) -> Result<(), String> {
    let expr = format!(
        "(() => {{ const el = document.querySelector({}); if (!el) return '__notfound__'; \
         el.focus(); el.value = {}; \
         el.dispatchEvent(new Event('input', {{bubbles:true}})); \
         el.dispatchEvent(new Event('change', {{bubbles:true}})); return 'ok'; }})()",
        js_str(selector),
        js_str(text)
    );
    match drive_evaluate(port, tab_id, &expr).await? {
        serde_json::Value::String(s) if s == "ok" => Ok(()),
        _ => Err(format!("input not found: {}", selector)),
    }
}

/// Read the innerText of the first element matching `selector` (or the
/// whole document body when selector is empty).
pub async fn drive_read_text(port: u16, tab_id: Option<&str>, selector: &str) -> Result<String, String> {
    let target = if selector.trim().is_empty() {
        "document.body".to_string()
    } else {
        format!("document.querySelector({})", js_str(selector))
    };
    let expr = format!("(() => {{ const el = {}; return el ? el.innerText : '__notfound__'; }})()", target);
    match drive_evaluate(port, tab_id, &expr).await? {
        serde_json::Value::String(s) if s == "__notfound__" => Err(format!("element not found: {}", selector)),
        serde_json::Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

#[cfg(test)]
mod driver_tests {
    use super::js_str;

    #[test]
    fn js_str_escapes_injection() {
        assert_eq!(js_str("a\"b"), "\"a\\\"b\"");
        // A selector trying to break out of the string cannot: every inner
        // double quote is backslash-escaped, so no bare quote closes it.
        let evil = "\"); alert(1); (\"";
        let escaped = js_str(evil);
        assert_eq!(escaped, "\"\\\"); alert(1); (\\\"\"");
        // Property: strip the outer quotes; no unescaped quote remains.
        let inner = &escaped[1..escaped.len() - 1];
        let bytes = inner.as_bytes();
        for i in 0..bytes.len() {
            if bytes[i] == b'"' {
                assert!(i > 0 && bytes[i - 1] == b'\\', "unescaped quote at {}", i);
            }
        }
    }

    #[test]
    fn js_str_neutralises_angle_brackets() {
        assert!(js_str("<script>").contains("\\x3C"));
    }
}


#[cfg(test)]
mod live_driver_tests {
    //! Live CDP driver check against a real Chrome on port 9222. Ignored by
    //! default (needs Chrome with --remote-debugging-port=9222 running). Run:
    //!   cargo test --bin orbixcore live_cdp_drive -- --ignored --nocapture
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_cdp_drive() {
        let port = 9222;
        assert!(probe(port).await, "no Chrome on {}; launch with --remote-debugging-port={}", port, port);

        // A self-contained page with an input and a button that echoes into #out.
        let html = r#"<html><body><h1 id=title>Orbix Driver Test</h1><input id=field /><button id=go onclick="document.getElementById('out').innerText=document.getElementById('field').value">Go</button><div id=out></div></body></html>"#;
        let data_url = format!("data:text/html,{}", urlencoding::encode(html));

        drive_navigate(port, None, &data_url).await.expect("navigate");
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        // read
        let title = drive_read_text(port, None, "#title").await.expect("read title");
        assert_eq!(title.trim(), "Orbix Driver Test", "read got: {:?}", title);

        // type into the field, then click Go which copies field -> #out
        drive_type(port, None, "#field", "hello-from-orbix").await.expect("type");
        drive_click(port, None, "#go").await.expect("click");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let out = drive_read_text(port, None, "#out").await.expect("read out");
        assert_eq!(out.trim(), "hello-from-orbix", "click+type roundtrip got: {:?}", out);

        // missing element errors cleanly
        assert!(drive_click(port, None, "#nope").await.is_err());
        println!("LIVE CDP DRIVE OK: navigate + read + type + click verified");
    }
}
