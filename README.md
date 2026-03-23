# Grokipedia Desktop

A native macOS desktop app for [grokipedia.com](https://grokipedia.com) — xAI's Wikipedia-style platform. Built with Tauri 2, it wraps the site in a multi-tab browser with shared login state, persistent history, and bookmarks. Linux works from the same codebase.

## Stack

- **Rust / Tauri 2** (`src-tauri/`) — window management, tab lifecycle, SQLite persistence, sidebar window
- **Vanilla HTML/CSS/JS** (`ui/`) — no bundler, no framework. Tauri injects `window.__TAURI__` via `withGlobalTauri: true`
- **SQLite** via `rusqlite` (bundled) — stores history and bookmarks in `~/Library/Application Support/com.grokipedia.desktop/grokipedia.db`

## Architecture

The main window is a `WebviewWindow` whose own webview loads the chrome UI (`index.html`). *Chrome* is the UI design term for the app's own controls (tabs, buttons, toolbars) as distinct from the website content. On macOS, the title bar uses `TitleBarStyle::Overlay` so the chrome sits directly in the title bar area — a single compact 40 px row with tabs, navigation buttons, bookmark, and sidebar toggle.

Content tabs are child `Webview`s (Tauri 2 `unstable` feature) layered on top of the main webview, positioned below the chrome. All content webviews share the same WebKit data store, so cookies from `accounts.x.ai` persist across tabs automatically.

| Webview | Purpose |
|---|---|
| `main` (WebviewWindow) | Chrome row — tabs, back/forward, bookmark, sidebar (`index.html`) |
| `tab-N` (child Webview) | One per open tab, loads grokipedia.com pages |
| `sidebar` (WebviewWindow) | Detached bookmarks & history panel (`sidebar.html`) |

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
| Reload | ⌘R |
| Back / Forward | ⌘[ / ⌘] |
| Bookmarks & History panel | ⌘⇧L |
| Add/remove bookmark | ⌘D |

---

## Known Issues

### Sidebar doesn't track the main window

The sidebar opens as a separate floating `WebviewWindow`. It does not move when the main window is dragged or resized. A future fix will convert it to a child `Webview` inside the main window so it follows automatically.

### Bookmark button state

The bookmark button occasionally shows the wrong active/inactive state after fast tab switches or navigation, due to an async race in `updateBookmarkBtn()`. A future fix will cache the bookmark set in JS and update it synchronously.
