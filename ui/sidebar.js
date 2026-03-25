const { invoke } = window.__TAURI__.core;

// ── Helpers ───────────────────────────────────────────────────────────────────
function escHtml(s) {
  return String(s)
    .replace(/&/g,'&amp;').replace(/</g,'&lt;')
    .replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function shortTitle(url) {
  try {
    const u = new URL(url);
    const path = u.pathname.replace(/\/$/, '');
    if (path) {
      const seg = path.split('/').filter(Boolean).pop();
      return decodeURIComponent(seg).replace(/-/g, ' ');
    }
    return u.hostname.replace('www.', '');
  } catch { return url; }
}

function formatTime(unixSecs) {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
}

// ── Render ────────────────────────────────────────────────────────────────────
async function renderBookmarks() {
  const list  = document.getElementById('bookmark-list');
  const items = await invoke('get_bookmarks');
  if (!items.length) {
    list.innerHTML = '<li class="empty-state">No bookmarks yet.<br>Press ⌘D to save a page.</li>';
    return;
  }
  list.innerHTML = items.map(b => `
    <li class="list-item" data-url="${escHtml(b.url)}">
      <div class="list-item-text">
        <div class="list-item-title">${escHtml(b.title || shortTitle(b.url))}</div>
        <div class="list-item-url">${escHtml(b.url)}</div>
      </div>
      <button class="item-del" data-bm-id="${b.id}">×</button>
    </li>`).join('');
}

async function renderHistory() {
  const list  = document.getElementById('history-list');
  const items = await invoke('get_history');
  if (!items.length) {
    list.innerHTML = '<li class="empty-state">No history yet.</li>';
    return;
  }
  list.innerHTML = items.map(h => `
    <li class="list-item" data-url="${escHtml(h.url)}">
      <div class="list-item-text">
        <div class="list-item-title">${escHtml(h.title || shortTitle(h.url))}</div>
        <div class="list-item-url">${escHtml(h.url)}</div>
      </div>
      <span class="list-item-time">${formatTime(h.visited_at)}</span>
    </li>`).join('');
}

// ── Settings ─────────────────────────────────────────────────────────────────
async function renderSettings() {
  const val = await invoke('get_setting', { key: 'restore_tabs' });
  document.getElementById('chk-restore-tabs').checked = val !== '0';
}

document.getElementById('chk-restore-tabs').addEventListener('change', async (e) => {
  await invoke('set_setting', { key: 'restore_tabs', value: e.target.checked ? '1' : '0' });
});

// ── Panel switching ───────────────────────────────────────────────────────────
function showPanel(name) {
  document.querySelectorAll('.sidebar-tab').forEach(t =>
    t.classList.toggle('active', t.dataset.panel === name));
  document.getElementById('panel-bookmarks').classList.toggle('hidden', name !== 'bookmarks');
  document.getElementById('panel-history').classList.toggle('hidden', name !== 'history');
  document.getElementById('panel-settings').classList.toggle('hidden', name !== 'settings');
  if (name === 'bookmarks') renderBookmarks();
  else if (name === 'history') renderHistory();
  else if (name === 'settings') renderSettings();
}

document.querySelectorAll('.sidebar-tab').forEach(btn =>
  btn.addEventListener('click', () => showPanel(btn.dataset.panel)));

// ── Helpers ───────────────────────────────────────────────────────────────────
const { emit } = window.__TAURI__.event;

async function navigateAndClose(url) {
  await emit('sidebar-navigate', { url });
  await emit('sidebar-closed', null);
  invoke('close_sidebar');
}

// ── Open URL in main window (navigates active tab) ────────────────────────────
document.getElementById('bookmark-list').addEventListener('click', async e => {
  const del = e.target.closest('.item-del');
  if (del) {
    const li = del.closest('.list-item');
    await invoke('delete_bookmark', { id: parseInt(del.dataset.bmId, 10) });
    if (li?.dataset.url) await emit('bookmark-deleted', { url: li.dataset.url });
    renderBookmarks();
    return;
  }
  const item = e.target.closest('.list-item');
  if (item?.dataset.url) navigateAndClose(item.dataset.url);
});

document.getElementById('history-list').addEventListener('click', async e => {
  const item = e.target.closest('.list-item');
  if (item?.dataset.url) navigateAndClose(item.dataset.url);
});

document.getElementById('btn-clear-history').addEventListener('click', async () => {
  await invoke('clear_history');
  renderHistory();
});

// ── Boot ──────────────────────────────────────────────────────────────────────
renderBookmarks();
