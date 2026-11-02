// Send to Orbix - options page.

const DEFAULT_APP_URL = 'http://127.0.0.1:51231';
const $ = (id) => document.getElementById(id);

function setStatus(message, ok) {
  const el = $('status');
  el.textContent = message;
  el.className = ok ? 'ok' : 'err';
}

chrome.storage.sync.get(['token', 'appUrl']).then(({ token, appUrl }) => {
  $('token').value = token || '';
  $('appUrl').value = appUrl || DEFAULT_APP_URL;
});

$('save').addEventListener('click', async () => {
  await chrome.storage.sync.set({
    token: $('token').value.trim(),
    appUrl: ($('appUrl').value.trim() || DEFAULT_APP_URL).replace(/\/+$/, ''),
  });
  setStatus('Saved.', true);
});

$('test').addEventListener('click', async () => {
  const base = ($('appUrl').value.trim() || DEFAULT_APP_URL).replace(/\/+$/, '');
  const token = $('token').value.trim();
  if (!token) {
    setStatus('Enter the token first.', false);
    return;
  }
  setStatus('Testing…', true);
  try {
    const resp = await fetch(base + '/v1/models', {
      headers: { authorization: 'Bearer ' + token },
    });
    if (resp.ok) {
      setStatus('Connected. Orbix is ready to receive handoffs.', true);
    } else if (resp.status === 401) {
      setStatus('Orbix rejected the token. Copy it again from Settings > Models.', false);
    } else if (resp.status === 503) {
      setStatus('Connected, but no local model is running. Select one in Orbix Settings > Models.', false);
    } else {
      setStatus('Orbix returned ' + resp.status + '.', false);
    }
  } catch (_) {
    setStatus('Cannot reach Orbix. Is the app running with the endpoint on?', false);
  }
});
