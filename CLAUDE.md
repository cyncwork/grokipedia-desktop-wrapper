# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A native desktop app for **grokipedia.com** (xAI's Wikipedia-style platform). It is a Tauri 2 shell that wraps the site in a multi-tab browser with shared login state, history, and bookmarks. It targets macOS first; Linux works from the same codebase.

## Commands

All Rust commands require `source "$HOME/.cargo/env"` first (or a shell restart after installing Rust). The `cargo-tauri` CLI must also be installed (`cargo install tauri-cli`).

```bash
# Run in development (hot-reloads UI, rebuilds Rust on change)
source "$HOME/.cargo/env" && cargo tauri dev

# Check Rust compiles without running
source "$HOME/.cargo/env" && cargo build --manifest-path src-tauri/Cargo.toml

# Build a distributable .app
source "$HOME/.cargo/env" && cargo tauri build
```

### Deploy to /Applications (macOS)

After every `cargo tauri build`, replace the installed app:

```bash
rm -rf /Applications/Grokipedia.app && cp -R src-tauri/target/release/bundle/macos/Grokipedia.app /Applications/Grokipedia.app
```

**Always do this after building.** The user expects the latest build to be installed in `/Applications`.

There are no tests and no linter configured yet.

### Git commits

**Always propose the commit message to the user for review before committing.** Do not commit without explicit approval of the message.

## Architecture

The app has two layers that never overlap in responsibility:

### Rust (`src-tauri/src/lib.rs`)
All native behavior lives here as `#[tauri::command]` functions invoked from JS:
- **Window** — a bare `WindowBuilder` creates the `main` window (no webview of its own). The chrome UI and content tabs are all non-overlapping child webviews, which eliminates cursor flickering. On macOS, `TitleBarStyle::Overlay` + `hidden_title(true)` lets the chrome sit in the title bar area (traffic lights overlaid on the left).
- **Chrome** — a child `Webview` labeled `chrome` loads `index.html` and is pinned to the top of the window (height = `CHROME_H`). Its `on_page_load` nudges the window size by 1 px to force an initial `do_layout`.
- **Tab management** — `new_tab`, `close_tab`, `switch_tab`, `navigate_tab`, `go_back`, `go_forward`, `reload_tab`. Each tab is a child `Webview` (Tauri `unstable` feature) created with `window.add_child(builder, position, size)`. Tabs are shown/hidden with `.show()` / `.hide()` to preserve per-tab state. After creating a child webview, `new_tab` nudges the window size by 1 px to force a `Resized` event — this ensures `do_layout` runs with the correct `scale_factor()`.
- **Sidebar** — `open_sidebar` / `close_sidebar` open a decoration-free `WebviewWindow` positioned flush against the right edge of the main window.
- **Persistence** — `rusqlite` (bundled SQLite) stores history and bookmarks in `~/<AppData>/grokipedia.db`. Commands: `add_history`, `get_history`, `clear_history`, `add_bookmark`, `get_bookmarks`, `delete_bookmark`.
- **Resize handling** — a `window.on_window_event` listener calls `do_layout()` on every `Resized` event, repositioning the `chrome` webview and all content `tab-N` webviews.

### UI (`ui/`)
Vanilla HTML/CSS/JS, no bundler. Tauri injects `window.__TAURI__` because `withGlobalTauri: true` is set in `tauri.conf.json`. The JS accesses the API as:
```js
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;   // app.js uses listen only
const { emit }   = window.__TAURI__.event;   // sidebar.js uses emit
```
- `index.html` / `app.js` — the single-row chrome bar (40 px) rendered in the macOS title bar area. Contains tabs, back/forward, bookmark, and sidebar buttons. No address bar — the app is dedicated to grokipedia.com. Manages tab state in a plain JS array and calls Rust commands. Also listens for `menu-action` events from the native menu.
- `sidebar.html` / `sidebar.js` — the bookmarks/history panel. Opened as a child webview inside the main window. Navigating a link emits `sidebar-navigate` which `app.js` listens for to navigate the active tab.
- `style.css` — shared by both pages (chrome styles + `.sidebar-root` styles).

### Key constants
- `CHROME_H = 40.0` in Rust (`lib.rs`) is the chrome bar height. If you change this, update both the Rust constant and the `#chrome-bar` height in `style.css`.
- `SIDEBAR_W = 300.0` in Rust (`lib.rs`) is the sidebar width. When open, content tabs shrink to `window_width - SIDEBAR_W`.
- The `chrome` webview occupies `(0, 0)` to `(window_width, CHROME_H)`.
- Content webviews start at `y = CHROME_H` and fill the rest of the window.

### Webview labels
| Label | Type | Purpose |
|---|---|---|
| `main` | `Window` (bare, no webview) | The host window that holds all child webviews |
| `chrome` | child `Webview` | Chrome bar — loads `index.html` (tabs, buttons) |
| `tab-N` | child `Webview` | Content tabs (grokipedia.com pages) |
| `sidebar` | child `Webview` | Bookmarks/history panel (sidebar.html) |

### Navigation filtering
Content tabs use `on_navigation` to restrict in-app navigation to `grokipedia.com`, `*.grokipedia.com`, and `accounts.x.ai`. All other URLs are opened in the system browser (`open` on macOS, `xdg-open` on Linux). An injected `initialization_script` rewrites `target="_blank"` links to same-window navigation so they go through `on_navigation` instead of being silently blocked.

### Shared login state
All `tab-N` webviews share the same WebKit data store (Tauri default), so cookies from `accounts.x.ai` are automatically shared — no extra work needed.

### Permissions / CSP
`src-tauri/capabilities/default.json` declares which webviews may call Tauri commands (`chrome` and `sidebar`). CSP is intentionally set to `null` in `tauri.conf.json` because the app loads an external domain.

### Event flow
```
User clicks link in sidebar
  → sidebar.js emits 'sidebar-navigate' + calls close_sidebar
  → app.js listens, calls navigateActive()
  → navigateActive() invokes navigate_tab (Rust)

Content webview finishes loading a page
  → on_page_load callback in Rust fires
  → Rust emits 'tab-navigated' to the 'chrome' webview
  → app.js updates tab title and records history

Menu keyboard shortcut (e.g. ⌘T)
  → Rust on_menu_event fires
  → Rust emits 'menu-action' to the 'chrome' webview
  → app.js dispatches to the appropriate handler
```

## Verification

After ANY code change, always run:
```bash
source "$HOME/.cargo/env" && cargo build --manifest-path src-tauri/Cargo.toml
```
If it fails, fix the error before moving on. Do not present partial or non-compiling code.
