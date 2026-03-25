use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager,
    WebviewUrl,
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size, Rect,
    webview::{WebviewBuilder, PageLoadPayload, PageLoadEvent, NewWindowResponse},
    window::WindowBuilder,
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryItem {
    id: i64,
    url: String,
    title: String,
    visited_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Bookmark {
    id: i64,
    url: String,
    title: String,
    created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavedTab {
    position: i32,
    url: String,
    title: String,
    active: bool,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct AppState {
    db: Mutex<Connection>,
    active_tab: Mutex<String>,
    tabs: Mutex<HashMap<String, String>>,  // tab_id -> url
    tab_counter: Mutex<u32>,
}

const CHROME_H: f64 = 40.0;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            url        TEXT    NOT NULL,
            title      TEXT    NOT NULL DEFAULT '',
            visited_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS bookmarks (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            url        TEXT    NOT NULL,
            title      TEXT    NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS saved_tabs (
            position INTEGER NOT NULL,
            url      TEXT    NOT NULL,
            title    TEXT    NOT NULL DEFAULT '',
            active   INTEGER NOT NULL DEFAULT 0
        );",
    )
}

// ── Layout helper ─────────────────────────────────────────────────────────────

const SIDEBAR_W: f64 = 300.0;

fn do_layout(app: &AppHandle, w: f64, h: f64) {
    let sidebar_open = app.get_webview("sidebar").is_some();
    let sidebar_w = if sidebar_open { SIDEBAR_W } else { 0.0 };
    let content_w = w - sidebar_w;

    if let Some(chrome) = app.get_webview("chrome") {
        let _ = chrome.set_bounds(Rect {
            position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
            size:     Size::Logical(LogicalSize::new(w, CHROME_H)),
        });
    }
    if let Some(state) = app.try_state::<AppState>() {
        let tabs = state.tabs.lock().unwrap();
        for tab_id in tabs.keys() {
            if let Some(wv) = app.get_webview(tab_id) {
                let _ = wv.set_bounds(Rect {
                    position: Position::Logical(LogicalPosition::new(0.0, CHROME_H)),
                    size:     Size::Logical(LogicalSize::new(content_w, h - CHROME_H)),
                });
            }
        }
    }
    if sidebar_open {
        if let Some(sb) = app.get_webview("sidebar") {
            let _ = sb.set_bounds(Rect {
                position: Position::Logical(LogicalPosition::new(content_w, CHROME_H)),
                size:     Size::Logical(LogicalSize::new(sidebar_w, h - CHROME_H)),
            });
        }
    }
}

// ── Tab commands ──────────────────────────────────────────────────────────────

#[tauri::command]
fn new_tab(app: AppHandle, state: tauri::State<AppState>, url: String) -> Result<String, String> {
    let mut counter = state.tab_counter.lock().unwrap();
    *counter += 1;
    let tab_id = format!("tab-{}", counter);

    let window = app.get_window("main").ok_or("main window not found")?;
    let phys  = window.inner_size().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let phys_chrome = (CHROME_H * scale).round() as u32;

    // Hide every existing content webview
    {
        let tabs = state.tabs.lock().unwrap();
        for id in tabs.keys() {
            if let Some(wv) = app.get_webview(id) {
                let _ = wv.hide();
            }
        }
    }

    let parsed = url.parse::<tauri::Url>().map_err(|e| e.to_string())?;
    let tab_id_cb = tab_id.clone();
    let tab_id_nav = tab_id.clone();
    let app_nav = app.clone();
    #[cfg(not(target_os = "macos"))]
    let app_onnav = app.clone();

    #[allow(unused_mut)]
    let mut builder = WebviewBuilder::new(&tab_id, WebviewUrl::External(parsed))
        // Use a real Safari UA so Cloudflare Turnstile and other bot
        // detection don't reject the embedded WKWebView.
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15")
        // Intercept user-clicked external links and open them in the
        // system browser instead of navigating the tab away from grokipedia.
        .initialization_script(r#"
            document.addEventListener('click', function(e) {
                var a = e.target.closest('a');
                if (!a || !a.href) return;
                try {
                    var host = new URL(a.href).hostname;
                    var ok = host === 'grokipedia.com'
                          || host.endsWith('.grokipedia.com')
                          || host === 'accounts.x.ai';
                    if (!ok) {
                        e.preventDefault();
                        window.location.href = 'grokipedia-ext:' + a.href;
                    }
                } catch {}
            }, true);
        "#);

    // On Linux, inject Escape handler via JS since there's no native key monitor
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.initialization_script(r#"
            document.addEventListener('keydown', function(e) {
                if (e.key === 'Escape') {
                    window.location.href = 'grokipedia-cmd:exit-fullscreen';
                }
            }, true);
        "#);
    }

    let builder = builder
        // Intercept new-window requests (window.open / OAuth popups):
        // navigate the current webview instead of opening a popup.
        .on_new_window(move |url, _features| {
            if let Some(wv) = app_nav.get_webview(&tab_id_nav) {
                let _ = wv.navigate(url);
            }
            NewWindowResponse::Deny
        })
        // Allow all navigation EXCEPT the sentinel scheme used by the
        // click interceptor to signal "open in system browser".
        .on_navigation(move |url| {
            #[cfg(not(target_os = "macos"))]
            if url.as_str().starts_with("grokipedia-cmd:exit-fullscreen") {
                if let Some(win) = app_onnav.get_window("main") {
                    if win.is_fullscreen().unwrap_or(false) {
                        let _ = win.set_fullscreen(false);
                    }
                }
                return false;
            }
            if url.scheme() == "grokipedia-ext" {
                let real_url = url.as_str().strip_prefix("grokipedia-ext:").unwrap_or("");
                if !real_url.is_empty() {
                    #[cfg(target_os = "macos")]
                    { let _ = std::process::Command::new("open").arg(real_url).spawn(); }
                    #[cfg(target_os = "linux")]
                    { let _ = std::process::Command::new("xdg-open").arg(real_url).spawn(); }
                }
                return false;
            }
            true
        })
        .on_page_load(move |webview: tauri::webview::Webview<tauri::Wry>, payload: PageLoadPayload<'_>| {
            if payload.event() == PageLoadEvent::Finished {
                let url_str = payload.url().to_string();
                let _ = webview.app_handle().emit_to(
                    tauri::EventTarget::Webview { label: "chrome".into() },
                    "tab-navigated",
                    serde_json::json!({ "tabId": tab_id_cb, "url": url_str }),
                );
            }
        });

    window
        .add_child(
            builder,
            PhysicalPosition::new(0_i32, phys_chrome as i32),
            PhysicalSize::new(phys.width, phys.height.saturating_sub(phys_chrome)),
        )
        .map_err(|e| e.to_string())?;

    {
        let mut tabs = state.tabs.lock().unwrap();
        tabs.insert(tab_id.clone(), url);
        *state.active_tab.lock().unwrap() = tab_id.clone();
    }

    // Nudge window size by 1 px to force a Resized event — this ensures
    // do_layout runs with the correct scale_factor after the child webview
    // is created (works around set_bounds being ignored early in lifecycle).
    let phys_now = window.inner_size().unwrap_or_default();
    let _ = window.set_size(PhysicalSize::new(phys_now.width, phys_now.height + 1));
    let _ = window.set_size(phys_now);

    Ok(tab_id)
}

#[tauri::command]
fn close_tab(app: AppHandle, state: tauri::State<AppState>, tab_id: String) -> Result<(), String> {
    state.tabs.lock().unwrap().remove(&tab_id);
    if let Some(wv) = app.get_webview(&tab_id) {
        wv.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn switch_tab(app: AppHandle, state: tauri::State<AppState>, tab_id: String) -> Result<(), String> {
    {
        let tabs = state.tabs.lock().unwrap();
        for id in tabs.keys() {
            if let Some(wv) = app.get_webview(id) {
                let _ = wv.hide();
            }
        }
    }
    if let Some(wv) = app.get_webview(&tab_id) {
        wv.show().map_err(|e| e.to_string())?;
    }
    *state.active_tab.lock().unwrap() = tab_id;
    Ok(())
}

#[tauri::command]
fn navigate_tab(app: AppHandle, tab_id: String, url: String) -> Result<(), String> {
    if let Some(wv) = app.get_webview(&tab_id) {
        let parsed = url.parse::<tauri::Url>().map_err(|e| e.to_string())?;
        wv.navigate(parsed).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn go_back(app: AppHandle, tab_id: String) -> Result<(), String> {
    if let Some(wv) = app.get_webview(&tab_id) {
        wv.eval("history.back()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn go_forward(app: AppHandle, tab_id: String) -> Result<(), String> {
    if let Some(wv) = app.get_webview(&tab_id) {
        wv.eval("history.forward()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn reload_tab(app: AppHandle, tab_id: String) -> Result<(), String> {
    if let Some(wv) = app.get_webview(&tab_id) {
        wv.eval("location.reload()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Window commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn start_drag(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_window("main") {
        win.start_dragging().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn exit_fullscreen(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_window("main") {
        if win.is_fullscreen().unwrap_or(false) {
            #[cfg(target_os = "macos")]
            toggle_native_fullscreen(&win);
            #[cfg(not(target_os = "macos"))]
            { let _ = win.set_fullscreen(false); }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn toggle_native_fullscreen(win: &tauri::Window) {
    use objc2_app_kit::NSWindow;
    let ns_window: *mut NSWindow = win.ns_window().unwrap().cast();
    unsafe { (*ns_window).toggleFullScreen(None) };
}

#[cfg(target_os = "macos")]
fn install_escape_monitor(app: &AppHandle) {
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::ptr::NonNull;

    let app_handle = app.clone();
    let block = block2::RcBlock::new(move |event_ptr: NonNull<NSEvent>| -> *mut NSEvent {
        let event = unsafe { event_ptr.as_ref() };
        if event.keyCode() == 53 { // 53 = Escape
            if let Some(win) = app_handle.get_window("main") {
                if win.is_fullscreen().unwrap_or(false) {
                    toggle_native_fullscreen(&win);
                    return std::ptr::null_mut(); // consume the event
                }
            }
        }
        event_ptr.as_ptr()
    });
    unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyDown,
            &block,
        );
    }
    // Leak the block so it lives for the app's lifetime
    std::mem::forget(block);
}

// ── Sidebar commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn open_sidebar(app: AppHandle) -> Result<(), String> {
    // If already open, do nothing
    if app.get_webview("sidebar").is_some() {
        return Ok(());
    }

    let window = app.get_window("main").ok_or("no main window")?;
    let phys = window.inner_size().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let phys_chrome = (CHROME_H * scale).round() as u32;
    let phys_sw = (SIDEBAR_W * scale).round() as u32;

    let builder = WebviewBuilder::new("sidebar", WebviewUrl::App("sidebar.html".into()))
        .focused(true);

    window.add_child(
        builder,
        PhysicalPosition::new((phys.width - phys_sw) as i32, phys_chrome as i32),
        PhysicalSize::new(phys_sw, phys.height.saturating_sub(phys_chrome)),
    ).map_err(|e| e.to_string())?;

    // Focus the sidebar webview so it receives keyboard events
    if let Some(sb) = app.get_webview("sidebar") {
        let _ = sb.set_focus();
    }

    // Relayout to shrink content tabs
    let w = phys.width as f64 / scale;
    let h = phys.height as f64 / scale;
    do_layout(&app, w, h);

    Ok(())
}

#[tauri::command]
fn close_sidebar(app: AppHandle) -> Result<(), String> {
    if let Some(wv) = app.get_webview("sidebar") {
        wv.close().map_err(|e| e.to_string())?;
    }
    // Relayout to expand content tabs
    if let Some(window) = app.get_window("main") {
        let scale = window.scale_factor().unwrap_or(1.0);
        let phys = window.inner_size().unwrap_or_default();
        let w = phys.width as f64 / scale;
        let h = phys.height as f64 / scale;
        do_layout(&app, w, h);
    }
    Ok(())
}

// ── History commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn add_history(state: tauri::State<AppState>, url: String, title: String) -> Result<(), String> {
    state.db.lock().unwrap()
        .execute(
            "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, ?3)",
            params![url, title, now_secs()],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_history(state: tauri::State<AppState>) -> Result<Vec<HistoryItem>, String> {
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT id, url, title, visited_at FROM history ORDER BY visited_at DESC LIMIT 500",
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok(HistoryItem {
            id: row.get(0)?, url: row.get(1)?, title: row.get(2)?, visited_at: row.get(3)?,
        })).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok()).collect();
    Ok(rows)
}

#[tauri::command]
fn clear_history(state: tauri::State<AppState>) -> Result<(), String> {
    state.db.lock().unwrap()
        .execute("DELETE FROM history", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Bookmark commands ─────────────────────────────────────────────────────────

#[tauri::command]
fn add_bookmark(state: tauri::State<AppState>, url: String, title: String) -> Result<i64, String> {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO bookmarks (url, title, created_at) VALUES (?1, ?2, ?3)",
        params![url, title, now_secs()],
    ).map_err(|e| e.to_string())?;
    Ok(db.last_insert_rowid())
}

#[tauri::command]
fn get_bookmarks(state: tauri::State<AppState>) -> Result<Vec<Bookmark>, String> {
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT id, url, title, created_at FROM bookmarks ORDER BY created_at DESC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok(Bookmark {
            id: row.get(0)?, url: row.get(1)?, title: row.get(2)?, created_at: row.get(3)?,
        })).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok()).collect();
    Ok(rows)
}

#[tauri::command]
fn delete_bookmark(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    state.db.lock().unwrap()
        .execute("DELETE FROM bookmarks WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Settings commands ─────────────────────────────────────────────────────────

#[tauri::command]
fn get_setting(state: tauri::State<AppState>, key: String) -> Result<Option<String>, String> {
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT value FROM settings WHERE key = ?1")
        .map_err(|e| e.to_string())?;
    let val = stmt.query_row(params![key], |row| row.get::<_, String>(0)).ok();
    Ok(val)
}

#[tauri::command]
fn set_setting(state: tauri::State<AppState>, key: String, value: String) -> Result<(), String> {
    state.db.lock().unwrap()
        .execute("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)", params![key, value])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_tabs(state: tauri::State<AppState>, tabs: Vec<SavedTab>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.execute("DELETE FROM saved_tabs", []).map_err(|e| e.to_string())?;
    for t in &tabs {
        db.execute(
            "INSERT INTO saved_tabs (position, url, title, active) VALUES (?1, ?2, ?3, ?4)",
            params![t.position, t.url, t.title, t.active as i32],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_saved_tabs(state: tauri::State<AppState>) -> Result<Vec<SavedTab>, String> {
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT position, url, title, active FROM saved_tabs ORDER BY position")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(SavedTab {
        position: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        active: row.get::<_, i32>(3)? != 0,
    })).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok()).collect();
    Ok(rows)
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db_path = app.path().app_data_dir()
                .expect("could not resolve app data dir")
                .join("grokipedia.db");
            std::fs::create_dir_all(db_path.parent().unwrap())
                .expect("could not create app data dir");

            let conn = Connection::open(&db_path).expect("could not open database");
            init_db(&conn).expect("could not initialise schema");

            app.manage(AppState {
                db: Mutex::new(conn),
                active_tab: Mutex::new(String::new()),
                tabs: Mutex::new(HashMap::new()),
                tab_counter: Mutex::new(0),
            });

            // ── Main window ───────────────────────────────────────────────────
            // Bare window (no webview of its own). Chrome and content tabs are
            // non-overlapping child webviews — eliminates cursor flickering.
            let mut win_builder = WindowBuilder::new(app, "main")
                .title("Grokipedia")
                .inner_size(1280.0, 800.0)
                .min_inner_size(900.0, 600.0);

            #[cfg(target_os = "macos")]
            {
                win_builder = win_builder
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true);
            }

            let window = win_builder.build()?;

            #[cfg(target_os = "macos")]
            install_escape_monitor(app.handle());

            // ── Chrome bar (CHROME_H px, fixed at top) ────────────────────────
            // on_page_load nudges window size by 1 px to force a Resized event,
            // ensuring do_layout runs with correct scale_factor() on Retina.
            let init_phys  = window.inner_size().unwrap_or_default();
            let init_scale = window.scale_factor().unwrap_or(1.0);
            let phys_chrome = (CHROME_H * init_scale).round() as u32;

            let app_handle_chrome = app.handle().clone();
            window.add_child(
                WebviewBuilder::new("chrome", WebviewUrl::App("index.html".into()))
                    .on_page_load(move |_wv, payload| {
                        if payload.event() == PageLoadEvent::Finished {
                            if let Some(win) = app_handle_chrome.get_window("main") {
                                let phys = win.inner_size().unwrap_or_default();
                                let _ = win.set_size(PhysicalSize::new(
                                    phys.width, phys.height + 1,
                                ));
                                let _ = win.set_size(phys);
                            }
                        }
                    }),
                PhysicalPosition::new(0_i32, 0_i32),
                PhysicalSize::new(init_phys.width, phys_chrome),
            )?;

            // ── Resize: keep chrome + content views fitted ────────────────────
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Resized(phys) = event {
                    let win   = app_handle.get_window("main").unwrap();
                    let scale = win.scale_factor().unwrap_or(1.0);
                    let w = phys.width  as f64 / scale;
                    let h = phys.height as f64 / scale;
                    do_layout(&app_handle, w, h);
                }
            });


            // ── App menu with keyboard shortcuts ──────────────────────────────
            let app_menu = SubmenuBuilder::new(app, "Grokipedia")
                .about(None)
                .separator()
                .item(&PredefinedMenuItem::services(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::hide(app, None)?)
                .item(&PredefinedMenuItem::hide_others(app, None)?)
                .item(&PredefinedMenuItem::show_all(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::quit(app, None)?)
                .build()?;

            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .item(&PredefinedMenuItem::undo(app, None)?)
                .item(&PredefinedMenuItem::redo(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::cut(app, None)?)
                .item(&PredefinedMenuItem::copy(app, None)?)
                .item(&PredefinedMenuItem::paste(app, None)?)
                .item(&PredefinedMenuItem::select_all(app, None)?)
                .build()?;

            let new_tab_item = MenuItemBuilder::new("New Tab")
                .id("new-tab").accelerator("CmdOrCtrl+T").build(app)?;
            let close_tab_item = MenuItemBuilder::new("Close Tab")
                .id("close-tab").accelerator("CmdOrCtrl+W").build(app)?;
            let file_menu = SubmenuBuilder::new(app, "File")
                .item(&new_tab_item)
                .item(&close_tab_item)
                .build()?;

            let reload_item = MenuItemBuilder::new("Reload Page")
                .id("reload").accelerator("CmdOrCtrl+R").build(app)?;
            let back_item = MenuItemBuilder::new("Back")
                .id("back").accelerator("CmdOrCtrl+[").build(app)?;
            let forward_item = MenuItemBuilder::new("Forward")
                .id("forward").accelerator("CmdOrCtrl+]").build(app)?;
            let panel_item = MenuItemBuilder::new("Bookmarks & History")
                .id("show-sidebar").accelerator("CmdOrCtrl+Shift+L").build(app)?;
            let fullscreen_item = MenuItemBuilder::new("Toggle Full Screen")
                .id("toggle-fullscreen").accelerator("CmdOrCtrl+Ctrl+F").build(app)?;
            let exit_fs_item = MenuItemBuilder::new("Exit Full Screen")
                .id("exit-fullscreen").accelerator("Escape").build(app)?;
            let view_menu = SubmenuBuilder::new(app, "View")
                .item(&reload_item)
                .separator()
                .item(&back_item)
                .item(&forward_item)
                .separator()
                .item(&panel_item)
                .separator()
                .item(&fullscreen_item)
                .item(&exit_fs_item)
                .build()?;

            let bookmark_item = MenuItemBuilder::new("Add Bookmark…")
                .id("add-bookmark").accelerator("CmdOrCtrl+D").build(app)?;
            let bookmarks_menu = SubmenuBuilder::new(app, "Bookmarks")
                .item(&bookmark_item)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&file_menu)
                .item(&edit_menu)
                .item(&view_menu)
                .item(&bookmarks_menu)
                .build()?;
            app.set_menu(menu)?;

            app.on_menu_event(|app, event| {
                if event.id.0.as_str() == "exit-fullscreen" {
                    if let Some(win) = app.get_window("main") {
                        if win.is_fullscreen().unwrap_or(false) {
                            #[cfg(target_os = "macos")]
                            toggle_native_fullscreen(&win);
                            #[cfg(not(target_os = "macos"))]
                            { let _ = win.set_fullscreen(false); }
                        }
                    }
                    return;
                }
                if event.id.0.as_str() == "toggle-fullscreen" {
                    if let Some(win) = app.get_window("main") {
                        #[cfg(target_os = "macos")]
                        toggle_native_fullscreen(&win);
                        #[cfg(not(target_os = "macos"))]
                        {
                            let is_fs = win.is_fullscreen().unwrap_or(false);
                            let _ = win.set_fullscreen(!is_fs);
                        }
                    }
                    return;
                }
                let target = tauri::EventTarget::Webview { label: "chrome".into() };
                let action = match event.id.0.as_str() {
                    "new-tab"      => "new-tab",
                    "close-tab"    => "close-tab",
                    "reload"       => "reload",
                    "back"         => "back",
                    "forward"      => "forward",
                    "show-sidebar" => "show-sidebar",
                    "add-bookmark" => "add-bookmark",
                    _ => return,
                };
                let _ = app.emit_to(target, "menu-action", action);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            new_tab, close_tab, switch_tab, navigate_tab,
            go_back, go_forward, reload_tab,
            start_drag, exit_fullscreen,
            open_sidebar, close_sidebar,
            add_history, get_history, clear_history,
            add_bookmark, get_bookmarks, delete_bookmark,
            get_setting, set_setting, save_tabs, get_saved_tabs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Grokipedia");
}
