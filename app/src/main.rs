// Meanwhile: something to do while your AI thinks.
// Tray app + always-on-top overlay + a tiny local HTTP endpoint that agent hooks poke.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use meanwhile_core::{self as core, Agent, Config};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Overlay window size in logical px: a 360x240 card plus room for its shadow.
const WIN_W: f64 = 392.0;
const WIN_H: f64 = 272.0;
const EDGE: f64 = 8.0;

struct Wait {
    gen: u64,
    started: Instant,
    shown: bool,
    from: String,
}

struct Inner {
    cfg: Mutex<Config>,
    visible: AtomicBool,
    seq: AtomicU64,
    gen: AtomicU64,
    wait: Mutex<Option<Wait>>,
    hotkey_error: Mutex<Option<String>>,
    server_error: Mutex<Option<String>>,
    tray: Mutex<Option<TrayIcon>>,
}

type St = Arc<Inner>;

fn log(msg: &str) {
    core::log_line("app", msg);
}

fn st(app: &AppHandle) -> St {
    app.state::<St>().inner().clone()
}

fn overlay(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("overlay")
}

// ---------------------------------------------------------------- overlay show / hide

#[cfg(windows)]
fn native_show(w: &WebviewWindow, show: bool) {
    // tao's show() activates the window (steals focus); we want it to appear without taking focus.
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE,
        SW_SHOWNOACTIVATE,
    };
    if let Ok(h) = w.hwnd() {
        let hwnd = h.0 as windows_sys::Win32::Foundation::HWND;
        unsafe {
            if show {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            } else {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

#[cfg(not(windows))]
fn native_show(w: &WebviewWindow, show: bool) {
    if show {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    } else {
        let _ = w.hide();
    }
}

fn place(app: &AppHandle, w: &WebviewWindow, corner: &str) {
    let mon = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten());
    let Some(mon) = mon else { return };
    let wa = mon.work_area();
    let s = mon.scale_factor();
    let (ww, wh, m) = ((WIN_W * s).round() as i32, (WIN_H * s).round() as i32, (EDGE * s).round() as i32);
    let right = corner.ends_with("right");
    let bottom = corner.starts_with("bottom");
    let x = if right { wa.position.x + wa.size.width as i32 - ww - m } else { wa.position.x + m };
    let y = if bottom { wa.position.y + wa.size.height as i32 - wh - m } else { wa.position.y + m };
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(ww as u32, wh as u32));
}

/// Show the overlay. mode: "manual" (hotkey/tray) or "auto" (agent hook).
fn show_overlay(app: &AppHandle, mode: &str, from: &str, done_only: bool) {
    let s = st(app);
    let Some(w) = overlay(app) else { return };
    let cfg = s.cfg.lock().unwrap().clone();
    let was_visible = s.visible.swap(true, Ordering::SeqCst);
    let seq = s.seq.fetch_add(1, Ordering::SeqCst) + 1;
    let waited_ms = s.wait.lock().unwrap().as_ref().map(|w| w.started.elapsed().as_millis() as u64).unwrap_or(0);
    if !was_visible {
        place(app, &w, &cfg.corner);
        native_show(&w, true);
    }
    let _ = w.emit(
        "mw:show",
        json!({ "mode": mode, "from": from, "seq": seq, "fresh": !was_visible, "doneOnly": done_only, "waitedMs": waited_ms, "config": cfg }),
    );
}

/// Ask the UI to animate out, then hide the window (unless it was shown again meanwhile).
fn request_hide(app: &AppHandle) {
    let s = st(app);
    if !s.visible.load(Ordering::SeqCst) {
        return;
    }
    let seq = s.seq.load(Ordering::SeqCst);
    if let Some(w) = overlay(app) {
        let _ = w.emit("mw:hide", json!({ "seq": seq }));
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(230));
        let app3 = app2.clone();
        let _ = app2.run_on_main_thread(move || {
            let s = st(&app3);
            if s.seq.load(Ordering::SeqCst) == seq && s.visible.swap(false, Ordering::SeqCst) {
                if let Some(w) = overlay(&app3) {
                    native_show(&w, false);
                }
            }
        });
    });
}

fn toggle(app: &AppHandle) {
    if st(app).visible.load(Ordering::SeqCst) {
        request_hide(app);
    } else {
        let from = st(app).wait.lock().unwrap().as_ref().map(|w| w.from.clone());
        match from {
            Some(f) => show_overlay(app, "auto", &f, false),
            None => show_overlay(app, "manual", "", false),
        }
    }
}

// ---------------------------------------------------------------- agent events

fn on_start(app: &AppHandle, from: String) -> &'static str {
    let s = st(app);
    let cfg = s.cfg.lock().unwrap().clone();
    if !cfg.auto_trigger {
        return "ignored (auto-trigger is off)";
    }
    let gen = s.gen.fetch_add(1, Ordering::SeqCst) + 1;
    let visible = s.visible.load(Ordering::SeqCst);
    *s.wait.lock().unwrap() = Some(Wait { gen, started: Instant::now(), shown: visible, from: from.clone() });
    if visible {
        // already open (hotkey): just switch it to "waiting for the agent"
        let a = app.clone();
        let f = from.clone();
        let _ = app.run_on_main_thread(move || show_overlay(&a, "auto", &f, false));
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(cfg.start_delay_seconds));
        let a = app2.clone();
        let _ = app2.run_on_main_thread(move || {
            let s = st(&a);
            let mut wait = s.wait.lock().unwrap();
            if let Some(w) = wait.as_mut().filter(|w| w.gen == gen && !w.shown) {
                w.shown = true;
                let from = w.from.clone();
                drop(wait);
                show_overlay(&a, "auto", &from, false);
            }
        });
        // an interrupted agent never says "done": give up after max_wait
        std::thread::sleep(Duration::from_secs(cfg.max_wait_minutes.max(1) * 60));
        let a = app2.clone();
        let _ = app2.run_on_main_thread(move || {
            let s = st(&a);
            let mut wait = s.wait.lock().unwrap();
            if wait.as_ref().map(|w| w.gen == gen).unwrap_or(false) {
                *wait = None;
                drop(wait);
                log("max wait reached, hiding");
                if let Some(w) = overlay(&a) {
                    let _ = w.emit("mw:cancel", json!({}));
                }
                request_hide(&a);
            }
        });
    });
    if visible {
        "waiting (overlay already open)"
    } else {
        "waiting"
    }
}

fn on_done(app: &AppHandle, from: String) -> &'static str {
    let s = st(app);
    let wait = s.wait.lock().unwrap().take();
    s.gen.fetch_add(1, Ordering::SeqCst);
    let visible = s.visible.load(Ordering::SeqCst);
    let shown = wait.as_ref().map(|w| w.shown).unwrap_or(false);
    let secs = wait.as_ref().map(|w| w.started.elapsed().as_secs_f32()).unwrap_or(0.0);
    log(&format!("done from {from:?} after {secs:.1}s (shown={shown}, visible={visible})"));
    let a = app.clone();
    if visible {
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = overlay(&a) {
                let _ = w.emit("mw:done", json!({ "from": from }));
            }
        });
        "ready shown"
    } else if shown {
        // the user closed the overlay while waiting: still pull them back
        let _ = app.run_on_main_thread(move || {
            show_overlay(&a, "auto", &from, true);
            if let Some(w) = overlay(&a) {
                let _ = w.emit("mw:done", json!({ "from": from }));
            }
        });
        "ready shown"
    } else {
        "quick answer, nothing shown"
    }
}

fn on_cancel(app: &AppHandle) -> &'static str {
    let s = st(app);
    let had = s.wait.lock().unwrap().take().is_some();
    s.gen.fetch_add(1, Ordering::SeqCst);
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = overlay(&a) {
            let _ = w.emit("mw:cancel", json!({}));
        }
        request_hide(&a);
    });
    if had {
        "cancelled"
    } else {
        "nothing to cancel"
    }
}

// ---------------------------------------------------------------- HTTP endpoint

fn serve(app: AppHandle, addr: String) {
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("cannot listen on {addr}: {e}");
            log(&msg);
            *st(&app).server_error.lock().unwrap() = Some(msg);
            return;
        }
    };
    log(&format!("listening on {addr}"));
    let json_hdr = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    for mut req in server.incoming_requests() {
        let path = req.url().split('?').next().unwrap_or("").to_string();
        let method = req.method().as_str().to_ascii_uppercase();
        let header = |req: &tiny_http::Request, name: &str| {
            req.headers()
                .iter()
                .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
                .map(|h| h.value.as_str().to_string())
        };
        let (code, body) = if method == "GET" && path == "/health" {
            (200, json!({ "app": "meanwhile", "version": core::VERSION }))
        } else if header(&req, "Origin").is_some() {
            // browsers always send Origin on cross-site POSTs; agents and curl don't
            (403, json!({ "error": "browser requests are not accepted" }))
        } else if method != "POST" {
            (405, json!({ "error": "use POST" }))
        } else {
            let token = st(&app).cfg.lock().unwrap().token.clone();
            let auth = header(&req, "Authorization").unwrap_or_default();
            let given = auth
                .strip_prefix("Bearer ")
                .map(|t| t.trim().to_string())
                .or_else(|| header(&req, "X-Meanwhile-Token"));
            if token.is_empty() || given.as_deref() != Some(token.as_str()) {
                (401, json!({ "error": "bad or missing token (see config.json)" }))
            } else {
                let mut raw = String::new();
                let _ = std::io::Read::read_to_string(&mut std::io::Read::take(req.as_reader(), 4096), &mut raw);
                let from = serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| v.get("from").and_then(|f| f.as_str()).map(|s| s.chars().take(32).collect::<String>()))
                    .unwrap_or_default();
                match path.as_str() {
                    "/start" => (200, json!({ "ok": true, "result": on_start(&app, from) })),
                    "/done" => (200, json!({ "ok": true, "result": on_done(&app, from) })),
                    "/cancel" => (200, json!({ "ok": true, "result": on_cancel(&app) })),
                    "/toggle" => {
                        let a = app.clone();
                        let _ = app.run_on_main_thread(move || toggle(&a));
                        (200, json!({ "ok": true }))
                    }
                    "/status" => {
                        let s = st(&app);
                        let w = s.wait.lock().unwrap();
                        let hotkey = s.cfg.lock().unwrap().hotkey.clone();
                        let hotkey_error = s.hotkey_error.lock().unwrap().clone();
                        (
                            200,
                            json!({
                                "app": "meanwhile",
                                "version": core::VERSION,
                                "visible": s.visible.load(Ordering::SeqCst),
                                "waiting": w.is_some(),
                                "waiting_for": w.as_ref().map(|w| w.from.clone()),
                                "hotkey": hotkey,
                                "hotkey_error": hotkey_error,
                            }),
                        )
                    }
                    _ => (404, json!({ "error": "unknown path" })),
                }
            }
        };
        if path != "/health" && path != "/status" {
            log(&format!("{method} {path} -> {code} {body}"));
        }
        let resp = tiny_http::Response::from_string(body.to_string())
            .with_status_code(code)
            .with_header(json_hdr.clone());
        let _ = req.respond(resp);
    }
}

// ---------------------------------------------------------------- hotkey, autostart, tray

fn apply_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let sc: Shortcut = hotkey.parse().map_err(|e| format!("\"{hotkey}\" is not a valid shortcut: {e}"))?;
    gs.register(sc).map_err(|e| format!("could not register {hotkey} (taken by another app?): {e}"))
}

fn apply_autostart(app: &AppHandle, on: bool) {
    let al = app.autolaunch();
    let cur = al.is_enabled().unwrap_or(false);
    let r = if on && !cur {
        al.enable()
    } else if !on && cur {
        al.disable()
    } else {
        Ok(())
    };
    if let Err(e) = r {
        log(&format!("autostart: {e}"));
    }
}

fn os_name() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else {
        "login"
    }
}

fn build_menu(app: &AppHandle, cfg: &Config) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "toggle", format!("Show / hide now\t{}", cfg.hotkey), true, None::<&str>)?;
    let auto = CheckMenuItem::with_id(app, "auto", "Pop up while an agent is thinking", true, cfg.auto_trigger, None::<&str>)?;
    let acts = Submenu::with_items(
        app,
        "Activities",
        true,
        &[
            &CheckMenuItem::with_id(app, "act:body", "Body: stretch, water, eyes, breathing", true, cfg.activities.body, None::<&str>)?,
            &CheckMenuItem::with_id(app, "act:brain", "Brain: math, reaction, words", true, cfg.activities.brain, None::<&str>)?,
            &CheckMenuItem::with_id(app, "act:focus", "Focus: what will you check first?", true, cfg.activities.focus, None::<&str>)?,
        ],
    )?;
    let c = |id: &str, label: &str| CheckMenuItem::with_id(app, format!("corner:{id}"), label, true, cfg.corner == id, None::<&str>);
    let corner = Submenu::with_items(
        app,
        "Corner",
        true,
        &[&c("top-left", "Top left")?, &c("top-right", "Top right")?, &c("bottom-left", "Bottom left")?, &c("bottom-right", "Bottom right")?],
    )?;
    let autostart = CheckMenuItem::with_id(app, "autostart", format!("Start with {}", os_name()), true, cfg.autostart, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings (hotkey, agents)…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Meanwhile", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&show, &sep1, &auto, &acts, &corner, &autostart, &settings, &sep2, &quit])
}

fn refresh_tray(app: &AppHandle) {
    let s = st(app);
    let cfg = s.cfg.lock().unwrap().clone();
    let hk_err = s.hotkey_error.lock().unwrap().clone();
    if let Some(t) = s.tray.lock().unwrap().as_ref() {
        if let Ok(m) = build_menu(app, &cfg) {
            let _ = t.set_menu(Some(m));
        }
        let tip = match hk_err {
            Some(e) => format!("Meanwhile: {e}"),
            None => format!("Meanwhile ({})", cfg.hotkey),
        };
        let _ = t.set_tooltip(Some(tip));
    };
}

/// Persist + apply a new config (hotkey, autostart, tray, overlay).
fn apply_config(app: &AppHandle, mut new: Config) -> Result<(), String> {
    let s = st(app);
    let old = s.cfg.lock().unwrap().clone();
    if new.token.is_empty() {
        new.token = old.token.clone();
    }
    let mut err = None;
    let had_err = s.hotkey_error.lock().unwrap().is_some();
    if new.hotkey != old.hotkey || had_err {
        match apply_hotkey(app, &new.hotkey) {
            Ok(()) => *s.hotkey_error.lock().unwrap() = None,
            Err(e) => {
                // fall back to the old one so there is always a working shortcut
                let _ = apply_hotkey(app, &old.hotkey);
                new.hotkey = old.hotkey.clone();
                err = Some(e);
            }
        }
    }
    if new.autostart != old.autostart {
        apply_autostart(app, new.autostart);
    }
    new.save().map_err(|e| format!("could not save config: {e}"))?;
    *s.cfg.lock().unwrap() = new.clone();
    refresh_tray(app);
    let _ = app.emit("mw:config", &new);
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let mut cfg = st(app).cfg.lock().unwrap().clone();
    match id {
        "toggle" => return toggle(app),
        "settings" => return open_settings(app),
        "quit" => {
            log("quit from tray");
            app.exit(0);
            return;
        }
        "auto" => cfg.auto_trigger = !cfg.auto_trigger,
        "autostart" => cfg.autostart = !cfg.autostart,
        "act:body" => cfg.activities.body = !cfg.activities.body,
        "act:brain" => cfg.activities.brain = !cfg.activities.brain,
        "act:focus" => cfg.activities.focus = !cfg.activities.focus,
        x if x.starts_with("corner:") => cfg.corner = x["corner:".len()..].to_string(),
        _ => return,
    }
    if !cfg.activities.body && !cfg.activities.brain && !cfg.activities.focus {
        cfg.activities.body = true;
    }
    let _ = apply_config(app, cfg);
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let r = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Meanwhile settings")
        .inner_size(520.0, 700.0)
        .min_inner_size(440.0, 520.0)
        .center()
        .build();
    if let Err(e) = r {
        log(&format!("settings window: {e}"));
    }
}

// ---------------------------------------------------------------- commands (UI -> Rust)

#[tauri::command]
fn get_state(app: AppHandle) -> serde_json::Value {
    let s = st(&app);
    let cfg = s.cfg.lock().unwrap().clone();
    let hook = hook_exe();
    let hotkey_error = s.hotkey_error.lock().unwrap().clone();
    let server_error = s.server_error.lock().unwrap().clone();
    json!({
        "config": cfg,
        "version": core::VERSION,
        "os": std::env::consts::OS,
        "hotkeyError": hotkey_error,
        "serverError": server_error,
        "configPath": core::config_path(),
        "hookExe": hook,
        "hookCommandStart": core::hook_command(&hook, "start --from claude"),
        "hookCommandDone": core::hook_command(&hook, "done --from claude"),
        "claudeFile": Agent::Claude.config_file(),
        "codexFile": Agent::Codex.config_file(),
    })
}

#[tauri::command]
fn save_config(app: AppHandle, config: Config) -> Result<(), String> {
    apply_config(&app, config)
}

#[tauri::command]
fn dismiss(app: AppHandle) {
    request_hide(&app);
}

#[tauri::command]
fn toggle_overlay(app: AppHandle) {
    toggle(&app);
}

#[tauri::command]
fn overlay_ready() {
    log("overlay UI loaded");
}

#[tauri::command]
fn setup_agent(agent: String, remove: bool) -> Result<String, String> {
    let a = Agent::parse(&agent).ok_or("unknown agent")?;
    let r = core::setup_agent(a, &hook_exe(), remove, false);
    log(&format!("setup {agent} remove={remove}: {r:?}"));
    r
}

#[tauri::command]
fn open_config_folder() {
    let dir = core::config_dir();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
}

fn hook_exe() -> std::path::PathBuf {
    let name = if cfg!(windows) { "meanwhile-hook.exe" } else { "meanwhile-hook" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| name.into())
}

// ---------------------------------------------------------------- main

fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("meanwhile {}", core::VERSION);
        return;
    }
    let first_run = !core::config_path().exists();
    let cfg = Config::load_or_create();
    let launched_by_os = std::env::args().any(|a| a == "--autostart");
    log(&format!("start v{} (first_run={first_run}, autostart_launch={launched_by_os})", core::VERSION));

    let inner: St = Arc::new(Inner {
        cfg: Mutex::new(cfg.clone()),
        visible: AtomicBool::new(false),
        seq: AtomicU64::new(0),
        gen: AtomicU64::new(0),
        wait: Mutex::new(None),
        hotkey_error: Mutex::new(None),
        server_error: Mutex::new(None),
        tray: Mutex::new(None),
    });

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // launched again (Start menu, terminal): act in the running instance
            log(&format!("second instance: {argv:?}"));
            if argv.iter().any(|a| a == "--toggle") {
                toggle(app)
            } else if !argv.iter().any(|a| a == "--autostart") {
                open_settings(app)
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _sc, ev| {
                    if ev.state() == ShortcutState::Pressed {
                        toggle(app)
                    }
                })
                .build(),
        )
        .manage(inner.clone())
        .invoke_handler(tauri::generate_handler![get_state, save_config, dismiss, toggle_overlay, overlay_ready, setup_agent, open_config_folder])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();

            #[allow(unused_mut)]
            let mut b = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
                .title("Meanwhile")
                .inner_size(WIN_W, WIN_H)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .focused(false)
                .visible(false);
            #[cfg(target_os = "macos")]
            {
                b = b.visible_on_all_workspaces(true).accept_first_mouse(true);
            }
            b.build()?;

            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            let tray = TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("Meanwhile")
                .menu(&build_menu(&handle, &cfg)?)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()))
                .on_tray_icon_event(|t, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        toggle(t.app_handle())
                    }
                })
                .build(app)?;
            *inner.tray.lock().unwrap() = Some(tray);

            if let Err(e) = apply_hotkey(&handle, &cfg.hotkey) {
                log(&e);
                *inner.hotkey_error.lock().unwrap() = Some(e);
            }
            if std::env::var("MEANWHILE_NO_AUTOSTART").map(|v| v == "1").unwrap_or(false) {
                log("MEANWHILE_NO_AUTOSTART=1: not touching OS autostart");
            } else {
                apply_autostart(&handle, cfg.autostart);
            }
            refresh_tray(&handle);

            let mut addrs = vec![format!("127.0.0.1:{}", cfg.port)];
            for a in &cfg.extra_listen {
                if !a.trim().is_empty() {
                    addrs.push(if a.contains(':') { a.clone() } else { format!("{a}:{}", cfg.port) });
                }
            }
            for a in addrs {
                let h = handle.clone();
                std::thread::spawn(move || serve(h, a));
            }

            if first_run && !launched_by_os {
                open_settings(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start Meanwhile");

    app.run(|_app, ev| {
        // keep running in the tray when the settings window closes
        if let tauri::RunEvent::ExitRequested { api, code, .. } = ev {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
