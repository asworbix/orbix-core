// Send to Orbix - background worker (FABLE B4).
// Receives selected text from the content script or the context menu,
// shapes it into a HandoffSpec, and posts it to the local Orbix app.
// The token lives in chrome.storage.sync, entered once in Options.

const DEFAULT_APP_URL = 'http://127.0.0.1:51231';

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: 'send-to-orbix',
    title: 'Send to Orbix',
    contexts: ['selection'],
    documentUrlPatterns: [
      'https://claude.ai/*',
      'https://copilot.microsoft.com/*',
      'https://m365.cloud.microsoft/*',
    ],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== 'send-to-orbix' || !info.selectionText) return;
  const result = await sendToOrbix(info.selectionText);
  if (tab && tab.id !== undefined) {
    chrome.tabs.sendMessage(tab.id, { type: 'orbix-result', result }).catch(() => {});
  }
});

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg && msg.type === 'send-to-orbix') {
    sendToOrbix(msg.text).then(sendResponse);
    return true; // async response
  }
  return false;
});

// Selected text is either a ready HandoffSpec (possibly inside a
// ```json fence) or a prose plan. Prose becomes prompt steps: one per
// numbered/bulleted line, or a single step when there is no list.
function buildSpec(text) {
  const trimmed = text
    .trim()
    .replace(/^```(?:json)?\s*/i, '')
    .replace(/\s*```$/, '');
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === 'object' && parsed.title && Array.isArray(parsed.steps)) {
      return parsed;
    }
  } catch (_) {
    // not JSON: fall through to prose shaping
  }

  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);
  const title = (lines[0] || 'Handoff from browser').slice(0, 120);
  const listLines = lines
    .map((l) => l.match(/^(?:\d+[.)]|[-*])\s+(.*)$/))
    .filter(Boolean)
    .map((m) => m[1])
    .slice(0, 50);
  const steps = listLines.length >= 2
    ? listLines.map((instruction) => ({ kind: 'prompt', instruction }))
    : [{ kind: 'prompt', instruction: text.trim() }];
  return { title, goal: '', steps };
}

async function sendToOrbix(text) {
  const { token, appUrl } = await chrome.storage.sync.get(['token', 'appUrl']);
  if (!token) {
    chrome.runtime.openOptionsPage();
    return { ok: false, message: 'No token yet. Paste it in the Orbix extension options.' };
  }
  const base = (appUrl || DEFAULT_APP_URL).replace(/\/+$/, '');
  const spec = buildSpec(text);
  try {
    const resp = await fetch(base + '/handoff?wait=false', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: 'Bearer ' + token,
      },
      body: JSON.stringify(spec),
    });
    if (resp.status === 401) {
      return { ok: false, message: 'Orbix rejected the token. Check the extension options.' };
    }
    if (resp.status === 400) {
      const body = await resp.json().catch(() => ({}));
      return { ok: false, message: 'Orbix rejected the plan: ' + (body.error || 'invalid spec') };
    }
    if (!resp.ok) {
      return { ok: false, message: 'Orbix returned ' + resp.status };
    }
    const body = await resp.json().catch(() => ({}));
    return { ok: true, message: 'Running in Orbix (run ' + (body.run_id || '?').slice(0, 8) + ')' };
  } catch (_) {
    return { ok: false, message: 'Orbix Core is not reachable. Is the app running with the endpoint on?' };
  }
}
