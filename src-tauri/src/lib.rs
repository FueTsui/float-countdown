#![recursion_limit = "256"]
// 悬浮倒计时 · 主进程(Rust / Tauri 2)
// 负责:设置读写、窗口管理、托盘菜单、全局快捷键、文件/文件夹导入、演示联动。
mod link;

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
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 悬浮窗内部留给阴影的边距(逻辑像素),需与 widget.html 一致
const PAD: f64 = 24.0;

/// 演示文件扩展名
pub const PRESENTATION_EXT: &[&str] = &[
    "pptx", "ppt", "pptm", "ppsx", "pps", "ppsm", "potx", "pdf", "odp", "key", "dps", "dpt",
];
/// 计时配置文件扩展名
const CONFIG_EXT: &[&str] = &["txt", "csv", "json", "md", "cfg", "ini", "list"];

/// 恢复默认外观时重置的键,也是导入/导出配置时会带上的样式键
const STYLE_KEYS: &[&str] = &[
    "fontFamily", "fontFile", "fontSize", "fontWeight", "letterSpacing", "textColor", "warnEnabled", "warnSec",
    "warnColor", "bgStyle", "bgColor", "bgColor2", "bgOpacity", "windowOpacity", "radius", "borderWidth",
    "borderColor", "shadow", "textShadow", "glow", "padX", "padY", "showLabel", "showProgress", "progressHeight",
    "progressPosition", "showTitle", "titleText", "titleScale", "titleColor", "timeFormat", "decimals",
];

fn defaults() -> Value {
    json!({
        "x": null, "y": null,
        "segments": [{ "name": "", "sec": 60 }], "loop": false, "autoNext": true,
        "endText": "时间到", "beep": true, "beepVolume": 0.5, "tickBeep": false, "flashOnEnd": true,
        "fontFamily": "Bahnschrift", "fontFile": "", "fontSize": 72, "fontWeight": 700, "letterSpacing": 0,
        "textColor": "#ffffff", "warnEnabled": true, "warnSec": 10, "warnColor": "#ff4d4f",
        "bgStyle": "solid", "bgColor": "#0f172a", "bgColor2": "#334155", "bgOpacity": 0.78, "windowOpacity": 1,
        "radius": 18, "borderWidth": 0, "borderColor": "#ffffff", "shadow": true, "textShadow": false, "glow": false,
        "padX": 30, "padY": 14, "showLabel": true, "timeFormat": "auto", "decimals": false,
        // 进度条
        "showProgress": true, "progressHeight": 0, "progressPosition": "bottom",
        // 自定义标题
        "showTitle": false, "titleText": "", "titleScale": 0.34, "titleColor": "",
        "locked": false, "clickThrough": false, "alwaysOnTop": true, "snap": true, "autoLaunch": false,
        "linkFiles": [], "linkOnlyFullscreen": true, "linkShow": false, "linkAutoStart": true, "pageTurn": "restart",
        "autoPage": false, "autoPageDelay": 1000, "scanSubfolders": true,
        // 向前翻页:none 不处理 / restart 重新计时
        "prevPage": "none",
        // 到时后继续计时并显示超时(-00:12)
        "overtime": false,
        // 每个演示文件单独的计时方案:{ "路径": { "segments": [...], "loop": false } }
        "fileProfiles": {},
        "hotkeys": { "toggle": "Ctrl+Alt+S", "reset": "Ctrl+Alt+R", "show": "Ctrl+Alt+H", "through": "Ctrl+Alt+T", "next": "Ctrl+Alt+N" }
    })
}

struct Drag {
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

fn st<R: Runtime>(app: &AppHandle<R>) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}
pub fn settings<R: Runtime>(app: &AppHandle<R>) -> Value {
    st(app).inner.lock().unwrap().s.clone()
}
fn widget<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
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

// ---------- 设置 ----------
fn backup_path(file: &Path) -> PathBuf {
    file.with_extension("bak.json")
}

fn read_json(file: &Path) -> Option<Value> {
    let txt = std::fs::read_to_string(file).ok()?;
    match serde_json::from_str::<Value>(&txt) {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

/// 读取设置。主文件损坏时改读备份,并返回一条提示。
fn load_settings(file: &Path) -> (Value, Option<String>) {
    let mut d = defaults();
    let mut warn = None;
    let j = match read_json(file) {
        Some(j) => Some(j),
        None if file.exists() => {
            let bak = read_json(&backup_path(file));
            warn = Some(if bak.is_some() { "设置文件损坏,已从备份恢复" } else { "设置文件损坏,已恢复默认设置" }.to_string());
            bak
        }
        None => None,
    };
    if let Some(j) = j {
        merge(&mut d, &j);
    }
    (d, warn)
}

/// 安全写入:先写临时文件,旧文件留作备份,再替换。中途断电也不会留下半个文件。
fn write_settings(file: &Path, txt: &str) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(txt.as_bytes())?;
        f.sync_all()?;
    }
    if read_json(file).is_some() {
        let _ = std::fs::copy(file, backup_path(file));
    }
    std::fs::rename(&tmp, file)
}

/// 只接受默认值里存在的键;hotkeys 做浅合并
fn merge(dst: &mut Value, p: &Value) {
    let Some(p) = p.as_object() else { return };
    let dmap = dst.as_object_mut().unwrap();
    for (k, v) in p {
        if !dmap.contains_key(k) {
            continue;
        }
        if k == "hotkeys" {
            if let (Some(Value::Object(h)), Some(nv)) = (dmap.get_mut("hotkeys"), v.as_object()) {
                for (hk, hv) in nv {
                    h.insert(hk.clone(), hv.clone());
                }
            }
        } else {
            dmap.insert(k.clone(), v.clone());
        }
    }
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

/// 显示悬浮窗但不抢焦点
fn show_inactive<R: Runtime>(w: &WebviewWindow<R>) {
    #[cfg(windows)]
    {
        if let Ok(h) = w.hwnd() {
            link::show_no_activate(h.0 as isize);
            return;
        }
    }
    let _ = w.show();
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

// ---------- 文件:演示文件 / 文件夹 / 配置 ----------
fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}
fn is_presentation(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // 跳过 Office 打开时产生的临时文件 "~$xxx.pptx" 和隐藏文件
    !name.starts_with("~$") && !name.starts_with('.') && PRESENTATION_EXT.contains(&ext_of(p).as_str())
}

/// 扫描文件夹里的演示文件
fn scan_dir(dir: &Path, depth: u32, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if out.len() >= 1000 {
            return;
        }
        if p.is_dir() {
            let hidden = p.file_name().and_then(|n| n.to_str()).map_or(true, |n| n.starts_with('.') || n.starts_with('$'));
            if depth > 0 && !hidden {
                scan_dir(&p, depth - 1, out);
            }
        } else if is_presentation(&p) {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

/// 处理拖入或选中的路径:文件夹自动识别其中的演示文件;演示文件直接绑定;
/// 配置文件交给前端解析(返回在 configs 里)。
#[tauri::command]
fn add_paths<R: Runtime>(app: AppHandle<R>, paths: Vec<String>) -> Value {
    let s = settings(&app);
    let depth = if b(&s, "scanSubfolders") { 6 } else { 0 };
    let (mut found, mut configs, mut skipped, mut dirs) = (vec![], vec![], 0u32, 0u32);
    for p in &paths {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            dirs += 1;
            scan_dir(&pb, depth, &mut found);
        } else if is_presentation(&pb) {
            found.push(p.clone());
        } else if CONFIG_EXT.contains(&ext_of(&pb).as_str()) {
            configs.push(p.clone());
        } else {
            skipped += 1;
        }
    }
    let mut list: Vec<String> = s["linkFiles"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let before = list.len();
    for p in &found {
        if !list.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            list.push(p.clone());
        }
    }
    let added = list.len() - before;
    if added > 0 {
        patch(&app, json!({ "linkFiles": list }));
    }
    let msg = if added > 0 {
        format!("已添加 {added} 个演示文件")
    } else if !found.is_empty() {
        "这些演示文件已经在列表里了".into()
    } else if dirs > 0 {
        "文件夹里没有找到演示文件".into()
    } else if skipped > 0 && configs.is_empty() {
        "不支持的文件类型".into()
    } else {
        String::new()
    };
    if !msg.is_empty() {
        toast(&app, msg.clone());
    }
    json!({ "added": added, "found": found.len(), "configs": configs, "skipped": skipped, "message": msg })
}

fn settings_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window("settings")
}

fn pick_link_files<R: Runtime>(app: &AppHandle<R>) -> Value {
    let mut d = app.dialog().file().set_title("选择演示文件").add_filter("演示文件", PRESENTATION_EXT);
    if let Some(w) = settings_window(app) {
        d = d.set_parent(&w);
    }
    match d.blocking_pick_files() {
        Some(fs) => add_paths(app.clone(), fs.into_iter().map(|p| p.to_string()).collect()),
        None => Value::Null,
    }
}

fn pick_link_folder<R: Runtime>(app: &AppHandle<R>) -> Value {
    let mut d = app.dialog().file().set_title("选择包含演示文件的文件夹");
    if let Some(w) = settings_window(app) {
        d = d.set_parent(&w);
    }
    match d.blocking_pick_folders() {
        Some(fs) => add_paths(app.clone(), fs.into_iter().map(|p| p.to_string()).collect()),
        None => Value::Null,
    }
}

#[tauri::command]
async fn pick_files<R: Runtime>(app: AppHandle<R>) -> Value {
    pick_link_files(&app)
}
#[tauri::command]
async fn pick_folder<R: Runtime>(app: AppHandle<R>) -> Value {
    pick_link_folder(&app)
}

#[tauri::command]
async fn pick_config<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    let mut d = app.dialog().file().set_title("导入计时配置").add_filter("计时配置", &["txt", "csv", "json", "md"]);
    if let Some(w) = settings_window(&app) {
        d = d.set_parent(&w);
    }
    d.blocking_pick_file().map(|p| p.to_string())
}

#[tauri::command]
async fn pick_font<R: Runtime>(app: AppHandle<R>) {
    let mut d = app.dialog().file().set_title("选择字体文件").add_filter("字体", &["ttf", "otf", "woff", "woff2", "ttc"]);
    if let Some(w) = settings_window(&app) {
        d = d.set_parent(&w);
    }
    if let Some(p) = d.blocking_pick_file() {
        patch(&app, json!({ "fontFile": p.to_string() }));
    }
}

#[tauri::command]
async fn export_config<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let mut d = app.dialog().file().set_title("导出当前配置").set_file_name("倒计时配置.json").add_filter("JSON", &["json"]);
    if let Some(w) = settings_window(&app) {
        d = d.set_parent(&w);
    }
    let Some(p) = d.blocking_save_file() else { return Ok(()) };
    let p = p.into_path().map_err(|e| e.to_string())?;
    let s = settings(&app);
    let mut set = Map::new();
    for k in STYLE_KEYS.iter().chain(["endText", "autoNext"].iter()) {
        set.insert(k.to_string(), s[*k].clone());
    }
    let out = json!({ "segments": s["segments"], "loop": s["loop"], "settings": set });
    std::fs::write(&p, serde_json::to_string_pretty(&out).unwrap()).map_err(|e| e.to_string())?;
    toast(&app, format!("已导出: {}", p.file_name().and_then(|n| n.to_str()).unwrap_or("")));
    Ok(())
}

/// 读取文本文件,先按 UTF-8,失败再按 GBK
#[tauri::command]
fn read_text(path: String) -> Result<String, String> {
    let buf = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => encoding_rs::GBK.decode(e.as_bytes()).0.into_owned(),
    })
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
fn platform_info() -> Value {
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
            patch(&app, json!({ "x": p.x, "y": p.y }));
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
            get_settings, set_settings, reset_style, remove_link, check_files, send_cmd, send_toast, timer_state, show_menu,
            open_settings_cmd, next_slide, platform_info, add_paths, pick_files, pick_folder, pick_config, pick_font,
            export_config, read_text, drag_start, drag_move, drag_end, resize
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
            let first_run = !file.exists();
            // 第一次运行时沿用旧版(Electron)的设置
            let old = app.path().config_dir().map(|d| d.join("float-countdown").join("settings.json"));
            let load_from = match old {
                Ok(o) if first_run && o.exists() => o,
                _ => file.clone(),
            };
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
