const { invoke } = window.__TAURI__.core;
const { listen }  = window.__TAURI__.event;

const HOME = 'https://grokipedia.com';

// ── State ─────────────────────────────────────────────────────────────────────
const state = {
  tabs: [],          // [{ id, url, title }]
  activeTabId: null,
  sidebarOpen: false,
};

let bookmarkUrls = new Set();

// ── DOM ───────────────────────────────────────────────────────────────────────
const tabsEl      = document.getElementById('tabs');
const btnBookmark = document.getElementById('btn-bookmark');

// ── Helpers ───────────────────────────────────────────────────────────────────
const activeTab = () => state.tabs.find(t => t.id === state.activeTabId) ?? null;

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

function escHtml(s) {
  return String(s)
    .replace(/&/g,'&amp;').replace(/</g,'&lt;')
    .replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

// ── Tab drag-to-reorder (pointer events) ─────────────────────────────────────
let drag = null; // { tabId, startX, el, ghost }
const DRAG_THRESHOLD = 5;

tabsEl.addEventListener('pointerdown', e => {
  const tabEl = e.target.closest('.tab');
  if (!tabEl || e.target.closest('.tab-close')) return;
  const tabId = tabEl.dataset.id;
  drag = { tabId, startX: e.clientX, el: tabEl, active: false };
  tabEl.setPointerCapture(e.pointerId);
});

tabsEl.addEventListener('pointermove', e => {
  if (!drag) return;
  if (!drag.active) {
    if (Math.abs(e.clientX - drag.startX) < DRAG_THRESHOLD) return;
    drag.active = true;
    drag.el.classList.add('dragging');
  }
  // Find which tab we're hovering over
  tabsEl.querySelectorAll('.tab').forEach(t => t.classList.remove('drop-before', 'drop-after'));
  const target = [...tabsEl.querySelectorAll('.tab')].find(t => {
    if (t.dataset.id === drag.tabId) return false;
    const r = t.getBoundingClientRect();
    return e.clientX >= r.left && e.clientX <= r.right;
  });
  if (target) {
    const r = target.getBoundingClientRect();
    target.classList.add(e.clientX < r.left + r.width / 2 ? 'drop-before' : 'drop-after');
  }
});

tabsEl.addEventListener('pointerup', e => {
  if (!drag) return;
  const wasDrag = drag.active;
  const dragId = drag.tabId;
  drag.el.classList.remove('dragging');
  tabsEl.querySelectorAll('.tab').forEach(t => t.classList.remove('drop-before', 'drop-after'));

  if (wasDrag) {
    const target = [...tabsEl.querySelectorAll('.tab')].find(t => {
      if (t.dataset.id === dragId) return false;
      const r = t.getBoundingClientRect();
      return e.clientX >= r.left && e.clientX <= r.right;
    });
    if (target) {
      const fromIdx = state.tabs.findIndex(t => t.id === dragId);
      const toIdx = state.tabs.findIndex(t => t.id === target.dataset.id);
      const r = target.getBoundingClientRect();
      const insertIdx = e.clientX < r.left + r.width / 2 ? toIdx : toIdx + 1;
      const [moved] = state.tabs.splice(fromIdx, 1);
      const finalIdx = insertIdx > fromIdx ? insertIdx - 1 : insertIdx;
      state.tabs.splice(finalIdx, 0, moved);
      renderTabs();
    }
  }
  drag = null;
});

// ── Tab rendering ─────────────────────────────────────────────────────────────
function renderTabs() {
  tabsEl.innerHTML = '';
  for (const tab of state.tabs) {
    const el = document.createElement('button');
    el.className = 'tab' + (tab.id === state.activeTabId ? ' active' : '');
    el.dataset.id = tab.id;
    el.innerHTML = `
      <span class="tab-title">${escHtml(tab.title || shortTitle(tab.url))}</span>
      <button class="tab-close" data-id="${tab.id}" title="Close">
        <svg viewBox="0 0 8 8" fill="none" stroke="currentColor" stroke-width="1.5">
          <line x1="1" y1="1" x2="7" y2="7"/>
          <line x1="7" y1="1" x2="1" y2="7"/>
        </svg>
      </button>`;
    tabsEl.appendChild(el);
  }
  updateBookmarkBtn();
}

// ── Tab actions ───────────────────────────────────────────────────────────────
async function openTab(url = HOME) {
  try {
    const id = await invoke('new_tab', { url });
    state.tabs.push({ id, url, title: shortTitle(url) });
    state.activeTabId = id;
    renderTabs();
    invoke('add_history', { url, title: shortTitle(url) });
  } catch (err) { console.error('new_tab:', err); }
}

async function closeTab(tabId) {
  await invoke('close_tab', { tabId });
  const idx = state.tabs.findIndex(t => t.id === tabId);
  state.tabs.splice(idx, 1);
  if (!state.tabs.length) { await openTab(); return; }
  if (state.activeTabId === tabId) {
    await switchTab(state.tabs[Math.max(0, idx - 1)].id);
    return;
  }
  renderTabs();
}

async function switchTab(tabId) {
  await invoke('switch_tab', { tabId });
  state.activeTabId = tabId;
  renderTabs();
}

async function navigateActive(url) {
  const tab = activeTab();
  if (!tab) return;
  await invoke('navigate_tab', { tabId: tab.id, url });
  tab.url   = url;
  tab.title = shortTitle(url);
  renderTabs();
  invoke('add_history', { url, title: tab.title });
}

// ── Sidebar toggle ────────────────────────────────────────────────────────────
const btnSidebar = document.getElementById('btn-sidebar');

async function toggleSidebar() {
  if (state.sidebarOpen) {
    await invoke('close_sidebar');
    state.sidebarOpen = false;
    btnSidebar.classList.remove('lit');
  } else {
    await invoke('open_sidebar');
    state.sidebarOpen = true;
    btnSidebar.classList.add('lit');
  }
}

// ── Bookmark helpers ──────────────────────────────────────────────────────────
function updateBookmarkBtn() {
  const tab = activeTab();
  if (!tab) return;
  btnBookmark.classList.toggle('lit', bookmarkUrls.has(tab.url));
}

async function toggleBookmark() {
  const tab = activeTab();
  if (!tab) return;
  if (bookmarkUrls.has(tab.url)) {
    const bms = await invoke('get_bookmarks');
    const ex  = bms.find(b => b.url === tab.url);
    if (ex) await invoke('delete_bookmark', { id: ex.id });
    bookmarkUrls.delete(tab.url);
  } else {
    await invoke('add_bookmark', { url: tab.url, title: tab.title || shortTitle(tab.url) });
    bookmarkUrls.add(tab.url);
  }
  updateBookmarkBtn();
}

// ── Event wiring ──────────────────────────────────────────────────────────────
tabsEl.addEventListener('click', e => {
  const close = e.target.closest('.tab-close');
  if (close) { closeTab(close.dataset.id); return; }
  const tab = e.target.closest('.tab');
  if (tab && tab.dataset.id !== state.activeTabId) switchTab(tab.dataset.id);
});

document.getElementById('btn-new-tab').addEventListener('click', () => openTab());

document.getElementById('btn-back').addEventListener('click', () => {
  const t = activeTab(); if (t) invoke('go_back', { tabId: t.id });
});
document.getElementById('btn-forward').addEventListener('click', () => {
  const t = activeTab(); if (t) invoke('go_forward', { tabId: t.id });
});

btnBookmark.addEventListener('click', toggleBookmark);

btnSidebar.addEventListener('click', toggleSidebar);

// ── Tauri events ─────────────────────────────────────────────────────────────

// Native menu keyboard shortcuts (fire regardless of which webview has focus)
listen('menu-action', ({ payload }) => {
  const t = activeTab();
  switch (payload) {
    case 'new-tab':      openTab(); break;
    case 'close-tab':    if (state.activeTabId) closeTab(state.activeTabId); break;
    case 'reload':       if (t) invoke('reload_tab', { tabId: t.id }); break;
    case 'back':         if (t) invoke('go_back',    { tabId: t.id }); break;
    case 'forward':      if (t) invoke('go_forward', { tabId: t.id }); break;
    case 'show-sidebar': toggleSidebar(); break;
    case 'add-bookmark': toggleBookmark(); break;
  }
});

// Sidebar deleted a bookmark — sync the local cache
listen('bookmark-deleted', ({ payload }) => {
  bookmarkUrls.delete(payload.url);
  updateBookmarkBtn();
});

// Sidebar closed itself (user clicked a link)
listen('sidebar-closed', () => {
  state.sidebarOpen = false;
  btnSidebar.classList.remove('lit');
});

// Sidebar opened a link — navigate active tab
listen('sidebar-navigate', ({ payload }) => {
  navigateActive(payload.url);
});

listen('tab-navigated', ({ payload }) => {
  const tab = state.tabs.find(t => t.id === payload.tabId);
  if (!tab) return;
  tab.url   = payload.url;
  tab.title = shortTitle(payload.url);
  renderTabs();
  invoke('add_history', { url: payload.url, title: tab.title });
});

// ── Window dragging & fullscreen ──────────────────────────────────────────────
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') invoke('exit_fullscreen');
});

document.querySelectorAll('.tl-spacer, .drag-spacer').forEach(el => {
  el.addEventListener('mousedown', e => {
    if (e.button === 0) invoke('start_drag');
  });
});

// ── Boot ──────────────────────────────────────────────────────────────────────
invoke('get_bookmarks').then(bms => {
  bms.forEach(b => bookmarkUrls.add(b.url));
  updateBookmarkBtn();
});
openTab(HOME);
