# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A native desktop app for **grokipedia.com** (xAI's Wikipedia-style platform). It is a Tauri 2 shell that wraps the site in a multi-tab browser with shared login state, history, and bookmarks. It targets macOS first; Linux works from the same codebase.

## Commands

All Rust commands require `source "$HOME/.cargo/env"` first (or a shell restart after installing Rust).

```bash
# Run in development (hot-reloads UI, rebuilds Rust on change)
source "$HOME/.cargo/env" && cargo tauri dev

# Check Rust compiles without running
source "$HOME/.cargo/env" && cargo build --manifest-path src-tauri/Cargo.toml

# Build a distributable .app
source "$HOME/.cargo/env" && cargo tauri build
```

There are no tests and no linter configured yet.

## Architecture

The app has two layers that never overlap in responsibility:

### Rust (`src-tauri/src/lib.rs`)
All native behavior lives here as `#[tauri::command]` functions invoked from JS:
- **Tab management** — `new_tab`, `close_tab`, `switch_tab`, `navigate_tab`, `go_back`, `go_forward`, `reload_tab`. Each tab is a child `Webview` (Tauri `unstable` feature) created with `window.add_child(builder, position, size)`. Tabs are shown/hidden with `.show()` / `.hide()` to preserve per-tab state.
- **Sidebar** — `open_sidebar` / `close_sidebar` open a decoration-free `WebviewWindow` positioned flush against the right edge of the main window.
- **Persistence** — `rusqlite` (bundled SQLite) stores history and bookmarks in `~/<AppData>/grokipedia.db`. Commands: `add_history`, `get_history`, `clear_history`, `add_bookmark`, `get_bookmarks`, `delete_bookmark`.
- **Resize handling** — a `window.on_window_event` listener resizes the `tabbar` webview (always 88 px tall, full width) and all content webviews whenever the window is resized.

### UI (`ui/`)
Vanilla HTML/CSS/JS, no bundler. Tauri injects `window.__TAURI__` because `withGlobalTauri: true` is set in `tauri.conf.json`. The JS accesses the API as:
```js
const { invoke } = window.__TAURI__.core;
const { listen, emit } = window.__TAURI__.event;
```
- `index.html` / `app.js` — the tab bar + nav bar chrome (88 px, always visible). Manages tab state in a plain JS array and calls Rust commands.
- `sidebar.html` / `sidebar.js` — the bookmarks/history panel. Opened as a separate window by Rust. Navigating a link emits `sidebar-navigate` which `app.js` listens for to navigate the active tab.
- `style.css` — shared by both pages (chrome styles + `.sidebar-root` styles).

### Key constants
- `CHROME_H = 88.0` in Rust (`lib.rs`) is the total chrome height (two 44px bars in CSS). If you change this, update both the Rust constant and the `#tab-bar` / `#nav-bar` heights in `style.css`.
- Content webviews start at `y = CHROME_H` and fill the rest of the window.

### Webview labels
| Label | Type | Purpose |
|---|---|---|
| `main` | `Window` | The host window (no webview of its own) |
| `tabbar` | child `Webview` | Our chrome UI (index.html) |
| `tab-N` | child `Webview` | Content tabs (grokipedia.com pages) |
| `sidebar` | `WebviewWindow` | Bookmarks/history panel (sidebar.html) |

### Shared login state
All `tab-N` webviews share the same WebKit data store (Tauri default), so cookies from `accounts.x.ai` are automatically shared — no extra work needed.

### Event flow
```
User clicks link in sidebar
  → sidebar.js emits 'sidebar-navigate' + calls close_sidebar
  → app.js listens, calls navigateActive()
  → navigateActive() invokes navigate_tab (Rust)

Content webview finishes loading a page
  → on_page_load callback in Rust fires
  → Rust emits 'tab-navigated' to the 'tabbar' webview
  → app.js updates tab title/URL bar and records history
```
