#![recursion_limit = "256"]
// 悬浮倒计时 · 主进程(Rust / Tauri 2)
// 负责:设置读写、窗口管理、托盘菜单、全局快捷键、文件/文件夹导入、演示联动。
mod files;
mod link;
mod settings;
mod update;

use files::*;
use settings::*;

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 悬浮窗内部留给阴影的边距(逻辑像素),需与 widget.html 一致
const PAD: f64 = 24.0;

pub(crate) struct Drag {
    cx: f64,
    cy: f64,
    x: i32,
    y: i32,
}

pub struct Inner {
    pub s: Value,
    timer_state: String,
    user_visible: bool,
    pub link_active: bool,
    pub link_file: Option<String>,
    force_show_until: Option<Instant>,
    drag: Option<Drag>,
}

pub struct AppState {
    pub inner: Mutex<Inner>,
    save_gen: AtomicU64,
    file: PathBuf,
}

pub(crate) fn st<R: Runtime>(app: &AppHandle<R>) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}
pub fn settings<R: Runtime>(app: &AppHandle<R>) -> Value {
    st(app).inner.lock().unwrap().s.clone()
}
pub(crate) fn widget<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window("widget")
}
pub fn cmd<R: Runtime>(app: &AppHandle<R>, c: &str) {
    let _ = app.emit_to("widget", "cmd", c);
}
pub fn toast<R: Runtime>(app: &AppHandle<R>, m: impl Into<String>) {
    // 发给所有窗口:悬浮窗显示在名称行,设置窗口显示在顶部提示条
    let _ = app.emit("toast", m.into());
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn save_soon<R: Runtime>(app: &AppHandle<R>) {
    let state = st(app);
    let gen = state.save_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        let state = st(&app);
        if state.save_gen.load(Ordering::SeqCst) != gen {
            return;
        }
        let txt = serde_json::to_string_pretty(&state.inner.lock().unwrap().s).unwrap_or_default();
        if let Err(e) = write_settings(&state.file, &txt) {
            toast(&app, format!("设置保存失败: {e}"));
        }
    });
}

pub fn patch<R: Runtime>(app: &AppHandle<R>, p: Value) {
    let (prev, cur) = {
        let state = st(app);
        let mut g = state.inner.lock().unwrap();
        let prev = g.s.clone();
        merge(&mut g.s, &p);
        (prev, g.s.clone())
    };
    save_soon(app);
    after_patch(app, &prev, &cur);
    let _ = app.emit("settings", &cur);
}

fn after_patch<R: Runtime>(app: &AppHandle<R>, prev: &Value, cur: &Value) {
    let changed = |k: &str| prev.get(k) != cur.get(k);
    if let Some(w) = widget(app) {
        if changed("alwaysOnTop") {
            let _ = w.set_always_on_top(b(cur, "alwaysOnTop"));
        }
        if changed("clickThrough") {
            let on = b(cur, "clickThrough");
            let _ = w.set_ignore_cursor_events(on);
            if prev.get("clickThrough").is_some() {
                let hk = cur["hotkeys"]["through"].as_str().unwrap_or("");
                toast(app, if on { format!("已开启点击穿透 ({hk} 关闭)") } else { "已关闭点击穿透".into() });
            }
        }
    }
    if changed("hotkeys") {
        register_hotkeys(app);
    }
    if changed("linkShow") {
        update_visibility(app);
    }
    if changed("linkFiles") && cur["linkFiles"].as_array().map_or(true, |a| a.is_empty()) {
        link::set_link(app, None);
    }
    if changed("autoLaunch") && prev.get("autoLaunch").is_some() {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let _ = if b(cur, "autoLaunch") { al.enable() } else { al.disable() };
    }
    if ["locked", "clickThrough", "alwaysOnTop", "showProgress", "showTitle", "hotkeys"].iter().any(|k| changed(k)) {
        refresh_tray(app);
    }
}

// ---------- 窗口 ----------
/// WebView2 的启动参数需要所有窗口一致,这里统一加上“允许自动播放”以便提示音无需点击即可响起
#[cfg(windows)]
const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

fn create_widget<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let s = settings(app);
    #[allow(unused_mut)]
    let mut wb = WebviewWindowBuilder::new(app, "widget", WebviewUrl::App("widget.html".into()))
        .title("CDWidget")
        .inner_size(320.0, 160.0)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .visible_on_all_workspaces(true)
        // 悬浮窗经常处于隐藏状态,必须关闭后台节流,否则计时、提示音和自动翻页会被延迟
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    #[cfg(windows)]
    {
        wb = wb.additional_browser_args(BROWSER_ARGS);
    }
    let win = wb.build()?;
    // 恢复上次位置(位置无效则放到主屏右上角)
    let pos = match (s["x"].as_f64(), s["y"].as_f64()) {
        (Some(x), Some(y)) if valid_pos(app, x, y) => Some(PhysicalPosition::new(x as i32, y as i32)),
        _ => None,
    };
    if let Some(p) = pos {
        let _ = win.set_position(p);
    } else if let Ok(Some(m)) = win.primary_monitor() {
        let sf = m.scale_factor();
        let wa = m.work_area();
        let x = wa.position.x + wa.size.width as i32 - (340.0 * sf) as i32;
        let y = wa.position.y + (20.0 * sf) as i32;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    if b(&s, "clickThrough") {
        let _ = win.set_ignore_cursor_events(true);
    }
    Ok(())
}

fn valid_pos<R: Runtime>(app: &AppHandle<R>, x: f64, y: f64) -> bool {
    app.available_monitors().map_or(false, |ms| {
        ms.iter().any(|m| {
            let (p, sz) = (m.position(), m.size());
            let (bx, by, bw, bh) = (p.x as f64, p.y as f64, sz.width as f64, sz.height as f64);
            x >= bx - 100.0 && x < bx + bw - 60.0 && y >= by - 100.0 && y < by + bh - 40.0
        })
    })
}

fn monitor_key(r: (i32, i32, i32, i32)) -> String {
    format!("{},{},{},{}", r.0, r.1, r.2, r.3)
}

/// 放映开始时让悬浮窗跟到放映所在的屏幕(rect = 该屏幕的 x, y, 宽, 高)。
/// 已经在这块屏幕上就不动;去过的屏幕用上次的位置,第一次去放在右上角。
pub fn move_widget_to_monitor<R: Runtime>(app: &AppHandle<R>, rect: (i32, i32, i32, i32)) {
    let Some(w) = widget(app) else { return };
    let (Ok(p), Ok(sz)) = (w.outer_position(), w.outer_size()) else { return };
    let (cx, cy) = (p.x + sz.width as i32 / 2, p.y + sz.height as i32 / 2);
    if cx >= rect.0 && cx < rect.0 + rect.2 && cy >= rect.1 && cy < rect.1 + rect.3 {
        return;
    }
    let s = settings(app);
    let remembered = s["screenPos"][monitor_key(rect)].as_array().and_then(|a| Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32)));
    let (x, y) = remembered.unwrap_or_else(|| {
        let sf = w.scale_factor().unwrap_or(1.0);
        (rect.0 + rect.2 - sz.width as i32 - (20.0 * sf) as i32 + (PAD * sf) as i32, rect.1 + (20.0 * sf) as i32 - (PAD * sf) as i32)
    });
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// 显示悬浮窗但不抢焦点。
/// 必须用 w.show() 而不是直接调 ShowWindow:窗口库自己记着“是否可见”,不同步的话,
/// 之后切换点击穿透/置顶等任何窗口属性时,它会按旧状态把窗口重新隐藏(悬浮窗“自己消失”)。
/// 悬浮窗创建时设了 focusable(false),所以 show() 不会激活窗口。
fn show_inactive<R: Runtime>(w: &WebviewWindow<R>) {
    let _ = w.show();
    #[cfg(windows)]
    if let Ok(h) = w.hwnd() {
        link::bring_topmost(h.0 as isize);
    }
}

pub fn update_visibility<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = widget(app) else { return };
    let want = {
        let state = st(app);
        let g = state.inner.lock().unwrap();
        let forced = g.force_show_until.map_or(false, |t| Instant::now() < t);
        g.user_visible && (!b(&g.s, "linkShow") || g.link_active || forced)
    };
    let vis = w.is_visible().unwrap_or(false);
    if want && !vis {
        show_inactive(&w);
        let _ = w.set_always_on_top(b(&settings(app), "alwaysOnTop"));
    } else if !want && vis {
        let _ = w.hide();
    }
}

fn toggle_visible<R: Runtime>(app: &AppHandle<R>) {
    {
        let state = st(app);
        let mut g = state.inner.lock().unwrap();
        g.user_visible = !g.user_visible;
    }
    update_visibility(app);
}

/// Windows 11 才有 Mica 背景(内部版本号 ≥ 22000)
#[cfg(windows)]
fn windows_build() -> u32 {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn open_settings<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    #[allow(unused_mut)]
    let mut wb = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("悬浮倒计时 设置")
        .inner_size(900.0, 680.0)
        .min_inner_size(720.0, 520.0)
        .center();
    #[cfg(windows)]
    {
        use tauri::window::{Effect, EffectsBuilder};
        wb = wb.additional_browser_args(BROWSER_ARGS);
        if windows_build() >= 22000 {
            wb = wb.transparent(true).effects(EffectsBuilder::new().effect(Effect::Mica).build());
        }
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::window::{Effect, EffectState, EffectsBuilder};
        wb = wb
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .transparent(true)
            .effects(EffectsBuilder::new().effect(Effect::Sidebar).state(EffectState::FollowsWindowActiveState).build());
    }
    let _ = wb.build();
}

// ---------- 菜单 / 托盘 / 快捷键 ----------
fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let s = settings(app);
    let running = st(app).inner.lock().unwrap().timer_state == "running";
    let hk = |k: &str| s["hotkeys"][k].as_str().unwrap_or("").to_string();
    let label = |t: &str, k: &str| {
        let h = hk(k);
        if h.is_empty() { t.to_string() } else { format!("{t}\t{h}") }
    };
    let item = |id: &str, t: String| MenuItem::with_id(app, id, t, true, None::<&str>);
    let check = |id: &str, t: String, on: bool| CheckMenuItem::with_id(app, id, t, true, on, None::<&str>);
    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &item("toggle", label(if running { "暂停" } else { "开始" }, "toggle"))?,
            &item("reset", label("重置", "reset"))?,
            &item("next", label("下一环节", "next"))?,
            &item("prev", "上一环节".into())?,
            &sep()?,
            &item("settings", "设置…".into())?,
            &item("pick-files", "添加演示文件…".into())?,
            &item("pick-folder", "从文件夹添加演示文件…".into())?,
            &item("import", "导入计时配置…".into())?,
            &sep()?,
            &check("showTitle", "显示标题".into(), b(&s, "showTitle"))?,
            &check("showProgress", "显示进度条".into(), b(&s, "showProgress"))?,
            &check("locked", "锁定位置".into(), b(&s, "locked"))?,
            &check("clickThrough", label("点击穿透", "through"), b(&s, "clickThrough"))?,
            &check("alwaysOnTop", "始终置顶".into(), b(&s, "alwaysOnTop"))?,
            &item("show", label("显示/隐藏", "show"))?,
            &sep()?,
            &item("quit", "退出".into())?,
        ],
    )
}

fn refresh_tray<R: Runtime>(app: &AppHandle<R>) {
    if let (Some(t), Ok(m)) = (app.tray_by_id("main"), build_menu(app)) {
        let _ = t.set_menu(Some(m));
    }
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let s = settings(app);
    match id {
        "toggle" | "reset" | "next" | "prev" => cmd(app, id),
        "settings" => open_settings(app),
        "pick-files" => spawn_task(app, |a| { pick_link_files(a); }),
        "pick-folder" => spawn_task(app, |a| { pick_link_folder(a); }),
        "import" => {
            open_settings(app);
            let _ = app.emit_to("settings", "do-import", ());
        }
        "showTitle" | "showProgress" | "locked" | "clickThrough" | "alwaysOnTop" => {
            let mut m = Map::new();
            m.insert(id.to_string(), Value::Bool(!b(&s, id)));
            patch(app, Value::Object(m));
        }
        "show" => toggle_visible(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

fn spawn_task<R: Runtime>(app: &AppHandle<R>, f: impl FnOnce(&AppHandle<R>) + Send + 'static) {
    let app = app.clone();
    std::thread::spawn(move || f(&app));
}

fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let icon = app.default_window_icon().cloned();
    let mut tb = TrayIconBuilder::with_id("main")
        .tooltip("悬浮倒计时")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| on_menu(app, e.id().as_ref()))
        .on_tray_icon_event(|t, e| {
            let app = t.app_handle();
            match e {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => {
                    toggle_visible(app)
                }
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => open_settings(app),
                _ => {}
            }
        });
    if let Some(i) = icon {
        tb = tb.icon(i);
    }
    #[cfg(target_os = "macos")]
    {
        tb = tb.icon_as_template(false);
    }
    tb.build(app)?;
    Ok(())
}

fn register_hotkeys<R: Runtime>(app: &AppHandle<R>) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let s = settings(app);
    let mut fails = vec![];
    for k in ["toggle", "reset", "show", "through", "next"] {
        let acc = s["hotkeys"][k].as_str().unwrap_or("").to_string();
        if acc.is_empty() {
            continue;
        }
        let key = k.to_string();
        let r = gs.on_shortcut(acc.as_str(), move |app, _sc, ev| {
            if ev.state != ShortcutState::Pressed {
                return;
            }
            match key.as_str() {
                "show" => toggle_visible(app),
                "through" => {
                    let on = b(&settings(app), "clickThrough");
                    patch(app, json!({ "clickThrough": !on }));
                }
                c => cmd(app, c),
            }
        });
        if r.is_err() {
            fails.push(acc);
        }
    }
    if !fails.is_empty() {
        toast(app, format!("快捷键被占用或无效: {}", fails.join(", ")));
    }
}

// ---------- 其余 IPC ----------
#[tauri::command]
fn get_settings<R: Runtime>(app: AppHandle<R>) -> Value {
    settings(&app)
}
#[tauri::command]
fn set_settings<R: Runtime>(app: AppHandle<R>, p: Value) {
    patch(&app, p);
}
#[tauri::command]
fn reset_style<R: Runtime>(app: AppHandle<R>) {
    let d = defaults();
    let mut o = Map::new();
    for k in STYLE_KEYS {
        o.insert(k.to_string(), d[*k].clone());
    }
    patch(&app, Value::Object(o));
}
#[tauri::command]
fn remove_link<R: Runtime>(app: AppHandle<R>, path: Option<String>) {
    let s = settings(&app);
    // 按路径移除(而不是按序号),两个窗口同时操作也不会删错
    let (list, mut profiles) = match &path {
        Some(p) => (
            s["linkFiles"].as_array().cloned().unwrap_or_default().into_iter().filter(|v| v.as_str() != Some(p.as_str())).collect::<Vec<_>>(),
            s["fileProfiles"].as_object().cloned().unwrap_or_default(),
        ),
        None => (vec![], Map::new()),
    };
    if let Some(p) = &path {
        profiles.remove(p);
    } else {
        profiles.clear();
    }
    patch(&app, json!({ "linkFiles": list, "fileProfiles": profiles }));
}

/// 重新扫描所有监视的文件夹,把新出现的演示文件加进列表
pub(crate) fn rescan_folders_inner<R: Runtime>(app: &AppHandle<R>) -> usize {
    let s = settings(app);
    let depth = if b(&s, "scanSubfolders") { 6 } else { 0 };
    let mut list: Vec<String> = s["linkFiles"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let before = list.len();
    for d in s["watchFolders"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let mut found = vec![];
        scan_dir(Path::new(d), depth, &mut found);
        for p in found {
            if !list.iter().any(|x| x.eq_ignore_ascii_case(&p)) {
                list.push(p);
            }
        }
    }
    let added = list.len() - before;
    if added > 0 {
        patch(app, json!({ "linkFiles": list }));
        toast(app, format!("监视的文件夹里发现 {added} 个新的演示文件,已添加"));
    }
    added
}
#[tauri::command]
fn rescan_folders<R: Runtime>(app: AppHandle<R>) -> usize {
    rescan_folders_inner(&app)
}
#[tauri::command]
fn remove_folder<R: Runtime>(app: AppHandle<R>, path: String) {
    let list: Vec<Value> = settings(&app)["watchFolders"].as_array().cloned().unwrap_or_default().into_iter().filter(|v| v.as_str() != Some(path.as_str())).collect();
    patch(&app, json!({ "watchFolders": list }));
}

/// 哪些已添加的演示文件现在找不到了(被移动或删除)
#[tauri::command]
fn check_files<R: Runtime>(app: AppHandle<R>) -> Value {
    let s = settings(&app);
    let missing: Vec<&str> = s["linkFiles"].as_array().map(|a| a.iter().filter_map(Value::as_str).filter(|p| !Path::new(p).exists()).collect()).unwrap_or_default();
    json!({ "missing": missing, "active": st(&app).inner.lock().unwrap().link_file })
}
#[tauri::command]
fn send_cmd<R: Runtime>(app: AppHandle<R>, c: String) {
    cmd(&app, &c);
}
#[tauri::command]
fn send_toast<R: Runtime>(app: AppHandle<R>, msg: String) {
    toast(&app, msg);
}
#[tauri::command]
fn timer_state<R: Runtime>(app: AppHandle<R>, state: String) {
    st(&app).inner.lock().unwrap().timer_state = state;
    refresh_tray(&app);
}
#[tauri::command]
fn show_menu<R: Runtime>(app: AppHandle<R>) {
    if let (Some(w), Ok(m)) = (widget(&app), build_menu(&app)) {
        let _ = w.popup_menu(&m);
    }
}
#[tauri::command]
fn open_settings_cmd<R: Runtime>(app: AppHandle<R>) {
    open_settings(&app);
}
#[tauri::command]
fn next_slide<R: Runtime>(app: AppHandle<R>) {
    if st(&app).inner.lock().unwrap().link_active {
        link::send_right_key();
    }
}

/// 平台信息:用于前端切换 Windows 11 / macOS 风格
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Value, String> {
    let cur = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || update::fetch_latest(&cur)).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("当前系统请到下载页手动安装".into());
    }
    tauri::async_runtime::spawn_blocking(move || update::install(&app, &url)).await.map_err(|e| e.to_string())?
}
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    update::open_url(&url)
}

#[tauri::command]
fn platform_info<R: Runtime>(app: AppHandle<R>) -> Value {
    let mut v = platform_info_inner();
    v["version"] = json!(app.package_info().version.to_string());
    v
}
fn platform_info_inner() -> Value {
    #[cfg(windows)]
    {
        let build = windows_build();
        json!({ "os": "windows", "build": build, "mica": build >= 22000, "accent": link::accent_color(), "linkSupported": true })
    }
    #[cfg(target_os = "macos")]
    {
        json!({ "os": "macos", "mica": false, "vibrancy": true, "accent": null, "linkSupported": false })
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        json!({ "os": "linux", "mica": false, "accent": null, "linkSupported": false })
    }
}

// 拖动悬浮窗(在主进程读取鼠标位置,避免多屏缩放造成的偏移),支持吸附屏幕边缘
#[tauri::command]
fn drag_start<R: Runtime>(app: AppHandle<R>) {
    let Some(w) = widget(&app) else { return };
    let state = st(&app);
        let mut g = state.inner.lock().unwrap();
    if b(&g.s, "locked") {
        return;
    }
    if let (Ok(c), Ok(p)) = (w.cursor_position(), w.outer_position()) {
        g.drag = Some(Drag { cx: c.x, cy: c.y, x: p.x, y: p.y });
    }
}
#[tauri::command]
fn drag_move<R: Runtime>(app: AppHandle<R>) {
    let Some(w) = widget(&app) else { return };
    let (d, snap) = {
        let state = st(&app);
        let g = state.inner.lock().unwrap();
        match &g.drag {
            Some(d) => ((d.cx, d.cy, d.x, d.y), b(&g.s, "snap")),
            None => return,
        }
    };
    let Ok(c) = w.cursor_position() else { return };
    let mut x = d.2 as f64 + c.x - d.0;
    let mut y = d.3 as f64 + c.y - d.1;
    if snap {
        if let (Ok(Some(m)), Ok(sz)) = (app.monitor_from_point(c.x, c.y), w.outer_size()) {
            let sf = m.scale_factor();
            let pad = PAD * sf;
            let t = 14.0 * sf;
            let (bx, by) = (m.position().x as f64, m.position().y as f64);
            let (bw, bh) = (m.size().width as f64, m.size().height as f64);
            let (ww, wh) = (sz.width as f64, sz.height as f64);
            if (x + pad - bx).abs() < t {
                x = bx - pad;
            } else if (x + ww - pad - (bx + bw)).abs() < t {
                x = bx + bw - ww + pad;
            }
            if (y + pad - by).abs() < t {
                y = by - pad;
            } else if (y + wh - pad - (by + bh)).abs() < t {
                y = by + bh - wh + pad;
            }
        }
    }
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}
#[tauri::command]
fn drag_end<R: Runtime>(app: AppHandle<R>) {
    let had = st(&app).inner.lock().unwrap().drag.take().is_some();
    if let (true, Some(w)) = (had, widget(&app)) {
        if let Ok(p) = w.outer_position() {
            // 同时记住在当前屏幕上的位置,放映时跟随到别的屏幕再回来能回到原处
            let mut pos = settings(&app)["screenPos"].as_object().cloned().unwrap_or_default();
            if let Ok(Some(m)) = w.current_monitor() {
                pos.insert(monitor_key((m.position().x, m.position().y, m.size().width as i32, m.size().height as i32)), json!([p.x, p.y]));
            }
            patch(&app, json!({ "x": p.x, "y": p.y, "screenPos": pos }));
        }
    }
}
#[tauri::command]
fn resize<R: Runtime>(app: AppHandle<R>, w: f64, h: f64) {
    let Some(win) = widget(&app) else { return };
    if st(&app).inner.lock().unwrap().drag.is_some() {
        return;
    }
    let w = w.ceil().clamp(60.0, 4000.0);
    let h = h.ceil().clamp(40.0, 2000.0);
    let sf = win.scale_factor().unwrap_or(1.0);
    if let Ok(cur) = win.inner_size() {
        let cur = cur.to_logical::<f64>(sf);
        if (cur.width - w).abs() <= 1.0 && (cur.height - h).abs() <= 1.0 {
            return;
        }
    }
    let _ = win.set_size(LogicalSize::new(w, h));
}

// ---------- 启动 ----------
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_settings(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            get_settings, set_settings, reset_style, remove_link, check_files, rescan_folders, remove_folder, send_cmd, send_toast, timer_state, show_menu,
            open_settings_cmd, next_slide, platform_info, add_paths, pick_files, pick_folder, pick_config, pick_font,
            export_config, read_text, check_update, install_update, open_url, drag_start, drag_move, drag_end, resize
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                // macOS 的复制/粘贴快捷键依赖应用菜单里的“编辑”菜单,没有它输入框里 Cmd+C/V 无效
                use tauri::menu::Submenu;
                let h = app.handle();
                let edit = Submenu::with_items(h, "编辑", true, &[
                    &PredefinedMenuItem::undo(h, None)?,
                    &PredefinedMenuItem::redo(h, None)?,
                    &PredefinedMenuItem::separator(h)?,
                    &PredefinedMenuItem::cut(h, None)?,
                    &PredefinedMenuItem::copy(h, None)?,
                    &PredefinedMenuItem::paste(h, None)?,
                    &PredefinedMenuItem::select_all(h, None)?,
                ])?;
                let app_menu = Submenu::with_items(h, "悬浮倒计时", true, &[
                    &PredefinedMenuItem::hide(h, None)?,
                    &PredefinedMenuItem::quit(h, None)?,
                ])?;
                app.set_menu(Menu::with_items(h, &[&app_menu, &edit])?)?;
            }
            let handle = app.handle().clone();
            let file = app.path().app_config_dir()?.join("settings.json");
            // 第一次运行时沿用旧版的设置:先找改名前的标识(com.dario.floatcountdown),再找 Electron 版
            let old = app.path().config_dir().map(|d| [d.join("com.dario.floatcountdown"), d.join("float-countdown")].map(|p| p.join("settings.json")));
            let migrate = if file.exists() { None } else { old.ok().and_then(|c| c.into_iter().find(|p| p.exists())) };
            let first_run = !file.exists() && migrate.is_none();
            let load_from = migrate.unwrap_or_else(|| file.clone());
            let (loaded, load_warn) = load_settings(&load_from);
            app.manage(AppState {
                inner: Mutex::new(Inner {
                    s: loaded,
                    timer_state: "idle".into(),
                    user_visible: true,
                    link_active: false,
                    link_file: None,
                    force_show_until: Some(Instant::now() + Duration::from_secs(4)),
                    drag: None,
                }),
                save_gen: AtomicU64::new(0),
                file,
            });
            create_widget(&handle)?;
            create_tray(&handle)?;
            register_hotkeys(&handle);
            link::start(&handle);
            // 首次显示,4 秒后再按“仅放映时显示”规则判断一次
            update_visibility(&handle);
            // 第一次运行时打开设置窗口,方便上手
            if first_run {
                open_settings(&handle);
            }
            let hs = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(2500));
                rescan_folders_inner(&hs);
            });
            // 启动后自动检查更新,有新版时提示一次(失败不打扰)
            let hu = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(12));
                if !settings(&hu).get("autoCheckUpdate").and_then(Value::as_bool).unwrap_or(true) {
                    return;
                }
                let cur = hu.package_info().version.to_string();
                if let Ok(r) = update::fetch_latest(&cur) {
                    if r["newer"].as_bool() == Some(true) {
                        toast(&hu, format!("发现新版本 {}(设置 → 关于 中更新)", r["latest"].as_str().unwrap_or("")));
                    }
                }
            });
            if let Some(w) = load_warn {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    toast(&h, w);
                });
            }
            let h2 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(4200));
                update_visibility(&h2);
                // 周期性重新置顶,避免被全屏放映窗口盖住
                loop {
                    std::thread::sleep(Duration::from_secs(3));
                    if let Some(w) = widget(&h2) {
                        if w.is_visible().unwrap_or(false) && b(&settings(&h2), "alwaysOnTop") {
                            #[cfg(windows)]
                            if let Ok(h) = w.hwnd() {
                                link::bring_topmost(h.0 as isize);
                                continue;
                            }
                            let _ = w.set_always_on_top(true);
                        }
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|w, e| {
            // 关闭设置窗口只是关掉它;悬浮窗不允许被关闭
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                if w.label() == "widget" {
                    api.prevent_close();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("启动失败")
        .run(|_app, e| {
            // 关闭所有窗口时不退出(托盘常驻)
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = e {
                api.prevent_exit();
            }
        });
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::scan_dir;

    #[test]
    fn folder_scan_finds_presentations() {
        let d = std::env::temp_dir().join("cd_scan_test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("子文件夹")).unwrap();
        for f in ["题库.pptx", "~$题库.pptx", "说明.txt", "汇报.PDF", "子文件夹/b.key", ".hidden.ppt"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let mut out = vec![];
        scan_dir(&d, 6, &mut out);
        let names: Vec<_> = out.iter().map(|p| Path::new(p).file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["b.key", "汇报.PDF", "题库.pptx"]);
        let mut top = vec![];
        scan_dir(&d, 0, &mut top);
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn corrupt_settings_fall_back_to_backup() {
        let d = std::env::temp_dir().join("cd_save_test");
        let _ = std::fs::remove_dir_all(&d);
        let f = d.join("settings.json");
        write_settings(&f, r#"{"showTitle": true}"#).unwrap();
        write_settings(&f, r#"{"showTitle": true, "titleText": "新"}"#).unwrap();
        assert_eq!(load_settings(&f).0["titleText"], json!("新"));
        std::fs::write(&f, "{ 写到一半").unwrap();
        let (s, warn) = load_settings(&f);
        assert_eq!(s["showTitle"], json!(true));
        assert!(warn.unwrap().contains("备份"));
    }

    #[test]
    fn merge_ignores_unknown_keys_and_merges_hotkeys() {
        let mut s = defaults();
        merge(&mut s, &json!({ "foo": 1, "showTitle": true, "hotkeys": { "next": "" } }));
        assert!(s.get("foo").is_none());
        assert_eq!(s["showTitle"], json!(true));
        assert_eq!(s["hotkeys"]["next"], json!(""));
        assert_eq!(s["hotkeys"]["toggle"], json!("Ctrl+Alt+S"));
    }
}
