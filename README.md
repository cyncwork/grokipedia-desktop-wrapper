# Grokipedia Desktop

A native macOS desktop app for [grokipedia.com](https://grokipedia.com) — xAI's Wikipedia-style platform. Built with Tauri 2, it wraps the site in a multi-tab browser with shared login state, persistent history, and bookmarks. Linux works from the same codebase.

## Stack

- **Rust / Tauri 2** (`src-tauri/`) — all native behaviour: tab management, window layout, SQLite persistence, sidebar window
- **Vanilla HTML/CSS/JS** (`ui/`) — no bundler, no framework. Tauri injects `window.__TAURI__` via `withGlobalTauri: true`
- **SQLite** via `rusqlite` (bundled) — stores history and bookmarks in `~/Library/Application Support/com.grokipedia.desktop/grokipedia.db`

## Architecture

The UI is split into two webview layers:

| Webview | Purpose |
|---|---|
| `tabbar` (child) | 88px chrome strip at the top — tab bar + nav/address bar (`index.html`) |
| `tab-N` (child) | One per open tab, loads grokipedia.com pages |
| `sidebar` (WebviewWindow) | Detached bookmarks & history panel (`sidebar.html`) |

All `tab-N` webviews share the same WebKit data store, so cookies from `accounts.x.ai` are shared automatically — login persists across tabs.

## Running

```bash
# Development (hot-reloads UI, rebuilds Rust on change)
source "$HOME/.cargo/env" && cargo tauri dev

# Check Rust compiles without running
source "$HOME/.cargo/env" && cargo build --manifest-path src-tauri/Cargo.toml

# Build a distributable .app
source "$HOME/.cargo/env" && cargo tauri build
```

> If you rename the project folder, run `cargo clean` inside `src-tauri/` before building to clear any stale cached paths.

## Keyboard Shortcuts

| Action | Shortcut |
|---|---|
| New tab | ⌘T |
| Close tab | ⌘W |
| Focus address bar | ⌘L |
| Reload | ⌘R |
| Back / Forward | ⌘[ / ⌘] |
| Bookmarks & History panel | ⌘⇧L |
| Add/remove bookmark | ⌘D |

---

## Known Issues

### Sidebar panel (bookmarks & history)

**Symptom:** The sidebar opens as a separate floating `WebviewWindow` positioned flush against the right edge of the main window. It does not slide in/out — it appears and disappears instantly. It also does not move if the main window is dragged.

**Root cause:** The sidebar is implemented as a standalone `WebviewWindow` (a separate OS window with no decorations), not as a child webview layered inside the main window. Its position is calculated once at open time from `outer_position()` + `inner_size()`. There is no ongoing position sync and no animation.

**Proper fix (TODO):** Either (a) convert the sidebar to a child `Webview` inside the main window so it moves and resizes automatically, or (b) add a `WindowEvent::Moved` / `WindowEvent::Resized` listener that repositions the sidebar window whenever the main window changes.

### Bookmark button state

**Symptom:** The bookmark button (⌘D) occasionally shows the wrong active/inactive state — e.g., appears bookmarked on a page that has not been saved, or vice versa after navigating.

**Root cause:** `updateBookmarkBtn()` fetches the full bookmark list from SQLite on every call and checks if the current tab's URL matches. The check runs asynchronously and the result can arrive after the tab URL has already changed (e.g., after a fast navigation or tab switch), leaving the button in a stale state.

**Proper fix (TODO):** Invalidate the bookmark button state synchronously on tab switch and navigation, and consider caching the bookmark list in JS state rather than re-fetching from Rust on every update.
