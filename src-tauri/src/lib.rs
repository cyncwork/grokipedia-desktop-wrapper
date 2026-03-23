use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager,
    WebviewUrl,
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size, Rect,
    webview::{WebviewBuilder, PageLoadPayload, PageLoadEvent},
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
        );",
    )
}

// ── Layout helper ─────────────────────────────────────────────────────────────

fn do_layout(app: &AppHandle, w: f64, h: f64) {
    if let Some(state) = app.try_state::<AppState>() {
        let tabs = state.tabs.lock().unwrap();
        for tab_id in tabs.keys() {
            if let Some(wv) = app.get_webview(tab_id) {
                let _ = wv.set_bounds(Rect {
                    position: Position::Logical(LogicalPosition::new(0.0, CHROME_H)),
                    size:     Size::Logical(LogicalSize::new(w, h - CHROME_H)),
                });
            }
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

    let builder = WebviewBuilder::new(&tab_id, WebviewUrl::External(parsed))
        .on_page_load(move |webview: tauri::webview::Webview<tauri::Wry>, payload: PageLoadPayload<'_>| {
            if payload.event() == PageLoadEvent::Finished {
                let url_str = payload.url().to_string();
                let _ = webview.app_handle().emit_to(
                    tauri::EventTarget::Webview { label: "main".into() },
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

// ── Sidebar commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn open_sidebar(app: AppHandle) -> Result<(), String> {
    // If already open, just focus it
    if let Some(w) = app.get_webview_window("sidebar") {
        w.show().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let main = app.get_window("main").ok_or("no main window")?;
    let phys  = main.inner_size().map_err(|e| e.to_string())?;
    let pos   = main.outer_position().map_err(|e| e.to_string())?;
    let scale = main.scale_factor().map_err(|e| e.to_string())?;

    let win_w  = phys.width  as f64 / scale;
    let win_h  = phys.height as f64 / scale;
    let sw     = 300.0_f64;
    let sh     = win_h - CHROME_H;
    let sx     = pos.x as f64 / scale + win_w - sw;
    let sy     = pos.y as f64 / scale + CHROME_H;

    tauri::WebviewWindowBuilder::new(
        &app,
        "sidebar",
        WebviewUrl::App("sidebar.html".into()),
    )
    .title("Grokipedia — Panel")
    .decorations(false)
    .position(sx, sy)
    .inner_size(sw, sh)
    .focused(false)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn close_sidebar(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("sidebar") {
        w.hide().map_err(|e| e.to_string())?;
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
            // index.html (chrome) is the window's own webview — it fills the
            // window automatically, avoiding the child-webview DPI sizing bug.
            // Overlay title bar lets our single-row chrome sit in the macOS
            // title-bar area.
            let mut win_builder = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
                .title("Grokipedia")
                .inner_size(1280.0, 800.0)
                .min_inner_size(900.0, 600.0);

            #[cfg(target_os = "macos")]
            {
                win_builder = win_builder
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true);
            }

            win_builder.build()?;

            // ── Resize: keep content tab views fitted below the chrome ────────
            let window = app.get_window("main").expect("main window");
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
            let view_menu = SubmenuBuilder::new(app, "View")
                .item(&reload_item)
                .separator()
                .item(&back_item)
                .item(&forward_item)
                .separator()
                .item(&panel_item)
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
                let target = tauri::EventTarget::Webview { label: "main".into() };
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
            open_sidebar, close_sidebar,
            add_history, get_history, clear_history,
            add_bookmark, get_bookmarks, delete_bookmark,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Grokipedia");
}
