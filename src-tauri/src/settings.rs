// 设置:默认值、读写(带备份)、合并。
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 恢复默认外观时重置的键,也是导入/导出配置时会带上的样式键
pub(crate) const STYLE_KEYS: &[&str] = &[
    "fontFamily", "fontFile", "fontSize", "fontWeight", "letterSpacing", "textColor", "warnEnabled", "warnSec",
    "warnColor", "bgStyle", "bgColor", "bgColor2", "bgOpacity", "windowOpacity", "radius", "borderWidth",
    "borderColor", "shadow", "textShadow", "glow", "padX", "padY", "showLabel", "showProgress", "progressHeight",
    "progressPosition", "showTitle", "titleText", "titleScale", "titleColor", "timeFormat", "decimals",
];

pub(crate) fn defaults() -> Value {
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
        // 添加过的文件夹:启动和打开设置时自动重新扫描
        "watchFolders": [],
        // 放映时悬浮窗跟到放映所在的屏幕;screenPos 记住每块屏幕上的位置
        "followScreen": true, "screenPos": {},
        // 向前翻页:none 不处理 / restart 重新计时
        "prevPage": "none",
        // 到时后继续计时并显示超时(-00:12)
        "overtime": false,
        // 每个演示文件单独的计时方案:{ "路径": { "segments": [...], "loop": false } }
        "fileProfiles": {},
        "hotkeys": { "toggle": "Ctrl+Alt+S", "reset": "Ctrl+Alt+R", "show": "Ctrl+Alt+H", "through": "Ctrl+Alt+T", "next": "Ctrl+Alt+N" }
    })
}

// ---------- 设置 ----------
pub(crate) fn backup_path(file: &Path) -> PathBuf {
    file.with_extension("bak.json")
}

pub(crate) fn read_json(file: &Path) -> Option<Value> {
    let txt = std::fs::read_to_string(file).ok()?;
    match serde_json::from_str::<Value>(&txt) {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

/// 读取设置。主文件损坏时改读备份,并返回一条提示。
pub(crate) fn load_settings(file: &Path) -> (Value, Option<String>) {
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
pub(crate) fn write_settings(file: &Path, txt: &str) -> std::io::Result<()> {
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
pub(crate) fn merge(dst: &mut Value, p: &Value) {
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

