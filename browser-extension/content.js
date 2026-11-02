// Send to Orbix - content script (FABLE B4).
// Shows a small "Send to Orbix" pill when a meaningful selection
// exists; clicking ships the selection to the background worker.
// Also renders result toasts for both the pill and the context menu.

let pill = null;
let toastTimer = null;

function ensurePill() {
  if (pill) return pill;
  pill = document.createElement('button');
  pill.id = 'orbix-send-pill';
  pill.type = 'button';
  pill.textContent = 'Send to Orbix';
  pill.addEventListener('mousedown', (e) => e.preventDefault()); // keep selection
  pill.addEventListener('click', () => {
    const text = String(window.getSelection() || '');
    if (!text.trim()) return;
    pill.disabled = true;
    pill.textContent = 'Sending…';
    chrome.runtime.sendMessage({ type: 'send-to-orbix', text }, (result) => {
      pill.disabled = false;
      pill.textContent = 'Send to Orbix';
      hidePill();
      showToast(result || { ok: false, message: 'No response from the extension.' });
    });
  });
  document.documentElement.appendChild(pill);
  return pill;
}

function hidePill() {
  if (pill) pill.classList.remove('orbix-visible');
}

let debounce = null;
document.addEventListener('selectionchange', () => {
  clearTimeout(debounce);
  debounce = setTimeout(() => {
    const text = String(window.getSelection() || '').trim();
    if (text.length >= 40) {
      ensurePill().classList.add('orbix-visible');
    } else {
      hidePill();
    }
  }, 250);
});

function showToast(result) {
  let toast = document.getElementById('orbix-toast');
  if (!toast) {
    toast = document.createElement('div');
    toast.id = 'orbix-toast';
    document.documentElement.appendChild(toast);
  }
  toast.textContent = (result.ok ? '◉ ' : '✗ ') + result.message;
  toast.classList.toggle('orbix-ok', !!result.ok);
  toast.classList.add('orbix-visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove('orbix-visible'), 5000);
}

chrome.runtime.onMessage.addListener((msg) => {
  if (msg && msg.type === 'orbix-result') {
    showToast(msg.result || { ok: false, message: 'Unknown result.' });
  }
});
