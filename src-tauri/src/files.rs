// 演示文件 / 文件夹 / 计时配置的识别、添加、导入导出。
use crate::settings::STYLE_KEYS;
use crate::*;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

/// 演示文件扩展名
pub(crate) const PRESENTATION_EXT: &[&str] = &[
    "pptx", "ppt", "pptm", "ppsx", "pps", "ppsm", "potx", "pdf", "odp", "key", "dps", "dpt",
];
/// 计时配置文件扩展名
pub(crate) const CONFIG_EXT: &[&str] = &["txt", "csv", "json", "md", "cfg", "ini", "list"];

// ---------- 文件:演示文件 / 文件夹 / 配置 ----------
pub(crate) fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}
pub(crate) fn is_presentation(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // 跳过 Office 打开时产生的临时文件 "~$xxx.pptx" 和隐藏文件
    !name.starts_with("~$") && !name.starts_with('.') && PRESENTATION_EXT.contains(&ext_of(p).as_str())
}

/// 扫描文件夹里的演示文件
pub(crate) fn scan_dir(dir: &Path, depth: u32, out: &mut Vec<String>) {
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
pub(crate) fn add_paths<R: Runtime>(app: AppHandle<R>, paths: Vec<String>) -> Value {
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
    // 添加的文件夹记下来,以后自动重新扫描
    let mut watch: Vec<String> = s["watchFolders"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    for p in &paths {
        if PathBuf::from(p).is_dir() && !watch.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            watch.push(p.clone());
        }
    }
    if watch.len() != s["watchFolders"].as_array().map_or(0, |a| a.len()) {
        patch(&app, json!({ "watchFolders": watch }));
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

pub(crate) fn settings_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window("settings")
}

pub(crate) fn pick_link_files<R: Runtime>(app: &AppHandle<R>) -> Value {
    let mut d = app.dialog().file().set_title("选择演示文件").add_filter("演示文件", PRESENTATION_EXT);
    if let Some(w) = settings_window(app) {
        d = d.set_parent(&w);
    }
    match d.blocking_pick_files() {
        Some(fs) => add_paths(app.clone(), fs.into_iter().map(|p| p.to_string()).collect()),
        None => Value::Null,
    }
}

pub(crate) fn pick_link_folder<R: Runtime>(app: &AppHandle<R>) -> Value {
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
pub(crate) async fn pick_files<R: Runtime>(app: AppHandle<R>) -> Value {
    pick_link_files(&app)
}
#[tauri::command]
pub(crate) async fn pick_folder<R: Runtime>(app: AppHandle<R>) -> Value {
    pick_link_folder(&app)
}

#[tauri::command]
pub(crate) async fn pick_config<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    let mut d = app.dialog().file().set_title("导入计时配置").add_filter("计时配置", &["txt", "csv", "json", "md"]);
    if let Some(w) = settings_window(&app) {
        d = d.set_parent(&w);
    }
    d.blocking_pick_file().map(|p| p.to_string())
}

#[tauri::command]
pub(crate) async fn pick_font<R: Runtime>(app: AppHandle<R>) {
    let mut d = app.dialog().file().set_title("选择字体文件").add_filter("字体", &["ttf", "otf", "woff", "woff2", "ttc"]);
    if let Some(w) = settings_window(&app) {
        d = d.set_parent(&w);
    }
    if let Some(p) = d.blocking_pick_file() {
        patch(&app, json!({ "fontFile": p.to_string() }));
    }
}

#[tauri::command]
pub(crate) async fn export_config<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
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
pub(crate) fn read_text(path: String) -> Result<String, String> {
    let buf = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => encoding_rs::GBK.decode(e.as_bytes()).0.into_owned(),
    })
}

