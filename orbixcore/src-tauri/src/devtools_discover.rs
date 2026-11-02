//! Selector discovery over DevTools: find the composer, the send button
//! and the reply area of a Chromium app's page by looking at the page,
//! instead of shipping selectors that drift with every update.
//!
//! One evaluation in the page scores every visible text input (placeholder
//! and label words, contenteditable, focus, position, size), then the
//! button nearest to it that reads like Send, then the most specific
//! repeated marker for messages. Each pick becomes a selector that is
//! stable across reloads: an id, a data attribute, an aria-label, else a
//! short nth-of-type path. Pairing runs it when a recipe has no selectors;
//! peer_send and peer_read run it again when a selector stops matching,
//! so a markup change costs one retry, not a release.

use serde_json::Value;

use crate::peer::DevToolsRecipe;

const DISCOVER_JS: &str = r##"(() => {
  const vis = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 10 || r.height < 10) return false;
    const cs = getComputedStyle(el);
    return cs.visibility !== 'hidden' && cs.display !== 'none' && cs.opacity !== '0';
  };
  const attrSel = (el) => {
    if (el.id && !/\d{3,}/.test(el.id)) return '#' + CSS.escape(el.id);
    for (const a of ['data-testid', 'data-test', 'name', 'aria-label', 'placeholder', 'data-id']) {
      const v = el.getAttribute(a);
      if (v) return el.tagName.toLowerCase() + '[' + a + '="' + v.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"]';
    }
    return null;
  };
  const pathSel = (el) => {
    const parts = []; let cur = el; let depth = 0;
    while (cur && cur.nodeType === 1 && depth < 6) {
      if (cur.id && !/\d{3,}/.test(cur.id)) { parts.unshift('#' + CSS.escape(cur.id)); break; }
      const tag = cur.tagName.toLowerCase();
      const parent = cur.parentElement;
      if (parent) {
        const sib = Array.from(parent.children).filter(c => c.tagName === cur.tagName);
        const idx = sib.indexOf(cur) + 1;
        parts.unshift(sib.length > 1 ? tag + ':nth-of-type(' + idx + ')' : tag);
      } else {
        parts.unshift(tag);
      }
      cur = parent; depth++;
    }
    return parts.join(' > ');
  };
  const sel = (el) => {
    const a = attrSel(el);
    if (a && document.querySelectorAll(a).length === 1) return a;
    const p = pathSel(el);
    return p || a;
  };
  const words = (el) => ((el.getAttribute('aria-label') || '') + ' ' + (el.getAttribute('placeholder') || '') + ' ' +
    (el.getAttribute('title') || '') + ' ' + (el.getAttribute('name') || '') + ' ' + (el.getAttribute('data-testid') || '') + ' ' +
    (el.id || '') + ' ' + (typeof el.className === 'string' ? el.className : '')).toLowerCase();
  const H = innerHeight || 800;

  const inputs = Array.from(document.querySelectorAll('textarea, input[type="text"], input:not([type]), [contenteditable="true"], [contenteditable=""], [role="textbox"]'))
    .filter(vis).filter(el => !el.disabled && !el.readOnly);
  let best = null, bestScore = -Infinity;
  for (const el of inputs) {
    const t = words(el); const r = el.getBoundingClientRect(); let s = 0;
    if (/(message|ask|prompt|write|reply|chat|talk|type|send|work with|anything|question)/.test(t)) s += 3;
    if (/(search|find|filter|url|address|email|password|username|first name|last name|phone)/.test(t)) s -= 4;
    if (el.isContentEditable) s += 1;
    if (document.activeElement === el) s += 2;
    if (r.top > H * 0.5) s += 1;
    s += Math.min(2, (r.width * r.height) / 60000);
    if (s > bestScore) { bestScore = s; best = el; }
  }
  const composer = best ? sel(best) : null;

  let send = null;
  if (best) {
    const br = best.getBoundingClientRect();
    const btns = Array.from(document.querySelectorAll('button, [role="button"], input[type="submit"]')).filter(vis);
    let bb = null, bs = -Infinity;
    for (const b of btns) {
      const t = (words(b) + ' ' + (b.innerText || '').toLowerCase()).trim();
      const r = b.getBoundingClientRect(); let s = 0;
      if (/(^|[^a-z])(send|submit|post|reply)([^a-z]|$)/.test(t)) s += 3;
      if (/send/.test(t)) s += 1;
      const dx = Math.abs((r.left + r.width / 2) - (br.left + br.width / 2));
      const dy = Math.abs((r.top + r.height / 2) - (br.top + br.height / 2));
      if (dx < 500 && dy < 220) s += 2;
      if (dx < 120 && dy < 90) s += 1;
      if (/(attach|upload|file|mic|voice|dictat|emoji|menu|close|cancel|settings|model|new chat|sidebar)/.test(t)) s -= 3;
      if (b.disabled) s -= 0.5;
      if (s > bs) { bs = s; bb = b; }
    }
    if (bb && bs >= 2) send = sel(bb);
  }

  let reply = null;
  const markers = [
    '[data-message-author-role="assistant"]', '[data-message-author="assistant"]', '[data-role="assistant"]',
    '[data-is-streaming]', 'article[data-message-id]', '[class*="assistant"]', '[class*="response"]',
    '[class*="message"]', '[role="article"]', 'article', '[role="log"] > *',
  ];
  for (const m of markers) {
    let els = [];
    try { els = Array.from(document.querySelectorAll(m)).filter(vis); } catch (e) { continue; }
    if (els.length >= 1) { reply = m; break; }
  }
  return { composer, send, reply, inputs: inputs.length, evidence: best ? words(best).trim().slice(0, 80) : '' };
})()"##;

/// Look at the page behind `port` and pick selectors for it.
pub async fn discover(port: u16) -> Result<DevToolsRecipe, String> {
    let v = crate::browser_attach::drive_evaluate(port, None, DISCOVER_JS).await?;
    parse_discovery(&v)
}

/// The evaluation's object into a recipe. Empty strings count as absent.
pub fn parse_discovery(v: &Value) -> Result<DevToolsRecipe, String> {
    let pick = |k: &str| v[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let composer = pick("composer");
    let send = pick("send");
    let reply = pick("reply");
    if composer.is_none() && reply.is_none() {
        return Err(format!(
            "the page shows no message box or reply area ({} text inputs seen)",
            v["inputs"].as_u64().unwrap_or(0)
        ));
    }
    Ok(DevToolsRecipe { composer, send, reply, port: None })
}

/// Fill what a recipe lacks with what discovery found. `replace` swaps the
/// shipped composer and send for the discovered ones (after they failed).
pub fn merge(recipe: Option<DevToolsRecipe>, found: DevToolsRecipe, replace: bool) -> DevToolsRecipe {
    let mut out = recipe.unwrap_or(DevToolsRecipe { composer: None, send: None, reply: None, port: None });
    if replace || out.composer.is_none() {
        out.composer = found.composer.or(out.composer);
    }
    if replace || out.send.is_none() {
        out.send = found.send.or(out.send);
    }
    if out.reply.is_none() {
        out.reply = found.reply;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn discovery_parses_and_merges() {
        let found = parse_discovery(&json!({ "composer": "#prompt-textarea", "send": "button[data-testid=\"send-button\"]", "reply": "[data-message-author-role=\"assistant\"]", "inputs": 2 })).unwrap();
        assert_eq!(found.composer.as_deref(), Some("#prompt-textarea"));
        assert!(parse_discovery(&json!({ "composer": "", "send": "", "reply": null, "inputs": 0 })).is_err());
        let shipped = DevToolsRecipe { composer: Some("div.old".into()), send: None, reply: None, port: Some(9500) };
        let kept = merge(Some(shipped.clone()), found.clone(), false);
        assert_eq!(kept.composer.as_deref(), Some("div.old"));
        assert_eq!(kept.send.as_deref(), Some("button[data-testid=\"send-button\"]"));
        assert_eq!(kept.port, Some(9500));
        let swapped = merge(Some(shipped), found, true);
        assert_eq!(swapped.composer.as_deref(), Some("#prompt-textarea"));
    }

    #[test]
    fn discovery_script_is_a_single_expression() {
        assert!(DISCOVER_JS.trim_start().starts_with("(() =>"));
        assert!(DISCOVER_JS.trim_end().ends_with("})()"));
    }
}

/// Live harness: a headless Chrome on a fake chat page. Ignored by default:
///   cargo test --bin orbixcore live_devtools_discovery -- --ignored --nocapture
#[cfg(all(test, target_os = "macos"))]
mod live {
    use super::*;

    const PAGE: &str = r#"<!doctype html><html><body style="margin:0">
<div id="app" style="display:flex;flex-direction:column;height:100vh">
  <header style="padding:8px"><input placeholder="Search chats" style="width:200px"></header>
  <main style="flex:1;padding:8px">
    <article data-message-author-role="user">Hi there</article>
    <article data-message-author-role="assistant">Hello! How can I help?</article>
  </main>
  <footer style="padding:8px;display:flex;gap:8px;align-items:center">
    <div id="prompt-textarea" contenteditable="true" role="textbox" aria-label="Message the assistant" style="flex:1;min-height:40px;border:1px solid #999"></div>
    <button aria-label="Attach files">+</button>
    <button data-testid="send-button" aria-label="Send message">Send</button>
  </footer>
</div>
<script>
  document.querySelector('[data-testid="send-button"]').addEventListener('click', () => {
    const el = document.getElementById('prompt-textarea');
    const a = document.createElement('article');
    a.setAttribute('data-message-author-role', 'assistant');
    a.textContent = 'Echo: ' + el.innerText;
    document.querySelector('main').appendChild(a);
    el.textContent = '';
  });
</script></body></html>"#;

    #[tokio::test]
    #[ignore]
    async fn live_devtools_discovery() {
        let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        if !std::path::Path::new(chrome).exists() {
            eprintln!("[skip] Chrome not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("chat.html");
        std::fs::write(&page, PAGE).unwrap();
        let port: u16 = 9877;
        let mut child = std::process::Command::new(chrome)
            .args([
                "--headless=new",
                &format!("--remote-debugging-port={}", port),
                &format!("--user-data-dir={}", dir.path().join("profile").display()),
                "--no-first-run",
                "--disable-gpu",
                "--window-size=900,700",
                &format!("file://{}", page.display()),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn chrome");
        let mut up = false;
        for _ in 0..40 {
            if crate::browser_attach::probe(port).await {
                up = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        assert!(up, "headless Chrome did not open port {}", port);
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let found = discover(port).await.expect("discover");
        eprintln!("found: {:?}", found);
        assert_eq!(found.composer.as_deref(), Some("#prompt-textarea"));
        assert_eq!(found.send.as_deref(), Some("button[data-testid=\"send-button\"]"));
        assert_eq!(found.reply.as_deref(), Some("[data-message-author-role=\"assistant\"]"));

        // Drive the page the way a paired peer would.
        let peer: crate::peer::Peer = serde_json::from_value(serde_json::json!({
            "id": "t", "app": "FakeChat", "transports": ["devtools"], "capabilities": ["send", "read"],
            "status": "connected", "bob_line": "", "paired_at": "", "chromium": true, "devtools_port": port,
            "devtools": { "composer": found.composer, "send": found.send, "reply": found.reply }
        })).unwrap();
        let sent = crate::peer::devtools_send(&peer, "hello from Bob").await.expect("send");
        eprintln!("send: {}", sent);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let (_title, text) = crate::peer::devtools_read(&peer).await.expect("read");
        eprintln!("read: {}", text);
        assert!(text.contains("Echo: hello from Bob"), "reply not read back: {}", text);
        let _ = child.kill();
    }
}
