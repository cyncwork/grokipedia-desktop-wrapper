# Grokipedia Desktop

A native desktop app for [grokipedia.com](https://grokipedia.com) — xAI's Wikipedia-style platform. Built with Tauri 2, it wraps the site in a multi-tab browser with shared login state, persistent history, and bookmarks.

Targets **macOS** and **Linux**.

## Features

- Multi-tab browsing with shared cookie/session state
- Bookmarks and browsing history (stored locally in SQLite)
- Sidebar panel for bookmarks & history
- Native keyboard shortcuts
- External links open in your default browser
- Single-row chrome bar — compact 40 px title bar with tabs, navigation, and controls

## Prerequisites

### macOS

1. **Xcode Command Line Tools**
   ```bash
   xcode-select --install
   ```
2. **Rust**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source "$HOME/.cargo/env"
   ```
3. **Tauri CLI**
   ```bash
   cargo install tauri-cli
   ```

### Linux (Debian/Ubuntu)

1. **System dependencies**
   ```bash
   sudo apt update
   sudo apt install -y \
     build-essential \
     curl \
     wget \
     file \
     libssl-dev \
     libgtk-3-dev \
     libwebkit2gtk-4.1-dev \
     libappindicator3-dev \
     librsvg2-dev \
     patchelf
   ```
2. **Rust**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source "$HOME/.cargo/env"
   ```
3. **Tauri CLI**
   ```bash
   cargo install tauri-cli
   ```

## Building & Running

```bash
# Development (hot-reloads UI, rebuilds Rust on change)
source "$HOME/.cargo/env" && cargo tauri dev

# Check Rust compiles without running
source "$HOME/.cargo/env" && cargo build --manifest-path src-tauri/Cargo.toml

# Build a distributable app
source "$HOME/.cargo/env" && cargo tauri build
```

The built app will be at:
- **macOS:** `src-tauri/target/release/bundle/macos/Grokipedia.app`
- **Linux:** `src-tauri/target/release/bundle/deb/` or `appimage/`

> If you rename the project folder, run `cargo clean` inside `src-tauri/` before building to clear stale cached paths.

## Stack

- **Rust / Tauri 2** (`src-tauri/`) — window management, tab lifecycle, SQLite persistence
- **Vanilla HTML/CSS/JS** (`ui/`) — no bundler, no framework
- **SQLite** via `rusqlite` (bundled) — stores history and bookmarks locally

## Architecture

The main window is a bare `Window` with no webview of its own. The chrome UI and content tabs are non-overlapping child webviews, which eliminates cursor flickering. On macOS, the title bar uses `TitleBarStyle::Overlay` so the chrome sits directly in the 40 px title bar area.

| Webview | Purpose |
|---|---|
| `main` | Host window (no webview) |
| `chrome` | Chrome bar — tabs, back/forward, bookmark, sidebar (`index.html`) |
| `tab-N` | One per open tab, loads grokipedia.com pages |
| `sidebar` | Child webview — bookmarks & history panel (`sidebar.html`) |

All content webviews share the same WebKit data store, so cookies from `accounts.x.ai` persist across tabs automatically.

## Keyboard Shortcuts

| Action | Shortcut |
|---|---|
| New tab | ⌘T |
| Close tab | ⌘W |
| Reload | ⌘R |
| Back / Forward | ⌘[ / ⌘] |
| Bookmarks & History panel | ⌘⇧L |
| Add/remove bookmark | ⌘D |

## Known Issues

- **Bookmark button state** — The bookmark button can show stale active/inactive state after fast tab switches, due to an async race in `updateBookmarkBtn()`.

## License

[MIT](LICENSE)
