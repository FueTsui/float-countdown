// 演示联动:检测前台窗口是否在放映已绑定的演示文件,并监听翻页按键。
// 目前只在 Windows 上实现;其他系统上这些函数什么都不做。
#[cfg(windows)]
use crate::{cmd, settings};
use crate::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// 切换联动状态。file 是命中的演示文件路径,前端据此切换该文件专属的计时方案。
pub fn set_link<R: Runtime>(app: &AppHandle<R>, file: Option<String>) {
    {
        let st = app.state::<AppState>();
        let mut g = st.inner.lock().unwrap();
        if g.link_file == file {
            return;
        }
        g.link_active = file.is_some();
        g.link_file = file.clone();
    }
    let _ = app.emit("link", json!({ "active": file.is_some(), "file": file }));
    crate::update_visibility(app);
}

/// 文件名在标题里的出现是否“完整”:前后必须是标题的边界或分隔符,
/// 避免“题库”误匹配“题库-8秒倒计时版”
#[cfg_attr(not(windows), allow(dead_code))]
fn whole_occurrence(t: &str, name: &str) -> bool {
    const BEFORE: &[char] = &[' ', '[', '(', '【', '"', '“', '「', '《', '\\', '/', '-', '–', '—', ':', '：'];
    const AFTER: &[char] = &[' ', '.', ']', ')', '】', '"', '”', '」', '》', '|', ':', '：'];
    t.match_indices(name).any(|(i, _)| {
        let pre = t[..i].chars().next_back();
        let post = t[i + name.len()..].chars().next();
        pre.map_or(true, |c| BEFORE.contains(&c)) && post.map_or(true, |c| AFTER.contains(&c))
    })
}

/// 在已绑定的文件里找出与窗口标题最匹配的一个:
/// 先比“文件名.扩展名”,再比不带扩展名的文件名(要求完整出现);多个命中时取最长的。
#[cfg_attr(not(windows), allow(dead_code))]
pub fn best_match(files: &[String], title: &str) -> Option<String> {
    let t = title.to_lowercase();
    if t.trim().is_empty() {
        return None;
    }
    let mut best: Option<(usize, &String)> = None;
    for f in files {
        // 同时按 \ 和 / 拆分,Windows 路径在任何系统上都能正确取到文件名
        let full = f.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
        let stem = match full.rfind('.') {
            Some(i) if i > 0 => full[..i].to_string(),
            _ => full.clone(),
        };
        let score = if !full.is_empty() && t.contains(&full) {
            full.chars().count() + 1000
        } else if !stem.is_empty() && whole_occurrence(&t, &stem) {
            stem.chars().count()
        } else {
            continue;
        };
        if best.map_or(true, |(s, _)| score > s) {
            best = Some((score, f));
        }
    }
    best.map(|(_, f)| f.clone())
}

#[cfg(test)]
mod tests {
    use super::best_match;
    fn files() -> Vec<String> {
        vec![r"C:\a\题库.pptx".into(), r"C:\a\题库-8秒倒计时版.pptx".into(), r"D:\年度汇报.pdf".into()]
    }
    #[test]
    fn picks_the_longer_name() {
        assert_eq!(best_match(&files(), "PowerPoint 幻灯片放映 - 题库-8秒倒计时版").as_deref(), Some(r"C:\a\题库-8秒倒计时版.pptx"));
        assert_eq!(best_match(&files(), "PowerPoint 幻灯片放映 - 题库").as_deref(), Some(r"C:\a\题库.pptx"));
        assert_eq!(best_match(&files(), "题库.pptx - PowerPoint").as_deref(), Some(r"C:\a\题库.pptx"));
    }
    #[test]
    fn only_the_short_file_bound_does_not_match_the_long_one() {
        let only = vec![r"C:\a\题库.pptx".to_string()];
        assert_eq!(best_match(&only, "PowerPoint 幻灯片放映 - 题库-8秒倒计时版"), None);
        assert_eq!(best_match(&only, "题库2 - PowerPoint"), None);
        assert!(best_match(&only, "PowerPoint 幻灯片放映 - [题库]").is_some());
    }
    #[test]
    fn pdf_in_browser() {
        assert!(best_match(&files(), "年度汇报.pdf - Microsoft Edge").is_some());
        assert_eq!(best_match(&files(), ""), None);
    }
}

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_RIGHT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, SetWindowPos, ShowWindow,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE,
    };

    pub struct Fg {
        pub pid: u32,
        pub fullscreen: bool,
        pub title: String,
    }

    pub fn foreground() -> Option<Fg> {
        unsafe {
            let h = GetForegroundWindow();
            if h.0.is_null() {
                return None;
            }
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(h, &mut buf);
            let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            let mut r = RECT::default();
            let _ = GetWindowRect(h, &mut r);
            let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let fullscreen = GetMonitorInfoW(mon, &mut mi).as_bool() && {
                let m = mi.rcMonitor;
                r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
            };
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            Some(Fg { pid, fullscreen, title })
        }
    }

    pub fn key_down(vk: i32) -> bool {
        unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
    }

    pub fn send_right() {
        let k = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(VK_RIGHT.0), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        };
        let inputs = [k(KEYBD_EVENT_FLAGS(0)), k(KEYEVENTF_KEYUP)];
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    pub fn show_no_activate(h: isize) {
        unsafe {
            let h = HWND(h as *mut _);
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    pub fn bring_topmost(h: isize) {
        unsafe {
            let _ = SetWindowPos(HWND(h as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// 系统强调色(注册表 DWM\AccentColor,格式 0xAABBGGRR)
    pub fn accent_color() -> Option<String> {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let v: u32 = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\DWM").ok()?.get_value("AccentColor").ok()?;
        Some(format!("#{:02x}{:02x}{:02x}", v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff))
    }
}

#[cfg(windows)]
pub use win::{accent_color, bring_topmost, show_no_activate};

pub fn send_right_key() {
    #[cfg(windows)]
    win::send_right();
}

/// 启动后台检测线程(Windows)
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::spawn(move || poll_loop(app));
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn poll_loop<R: Runtime>(app: AppHandle<R>) {
    use std::time::{Duration, Instant};
    // 监测的按键: 鼠标左键 空格 回车 右 下 PgDn N 左 上 PgUp P
    const KEYS: [i32; 11] = [1, 32, 13, 39, 40, 34, 78, 37, 38, 33, 80];
    let mut down = [false; 11];
    let me = std::process::id();
    let (mut i, mut miss) = (0u64, 0u32);
    let mut last_turn = Instant::now() - Duration::from_secs(1);
    loop {
        std::thread::sleep(Duration::from_millis(40));
        i += 1;
        let s = settings(&app);
        let has_files = s["linkFiles"].as_array().map_or(false, |a| !a.is_empty());
        if !has_files {
            down = [false; 11];
            continue;
        }
        let active = app.state::<AppState>().inner.lock().unwrap().link_active;
        for (j, &k) in KEYS.iter().enumerate() {
            let d = win::key_down(k);
            if d && !down[j] && active && s["pageTurn"].as_str() == Some("restart") {
                if on_key(&app, k) && last_turn.elapsed() >= Duration::from_millis(200) {
                    last_turn = Instant::now();
                    cmd(&app, "pageturn");
                }
            }
            down[j] = d;
        }
        if i % 8 != 0 {
            continue;
        }
        let Some(fg) = win::foreground() else { continue };
        if fg.pid == me {
            continue; // 前台是本程序自己时保持原状态
        }
        let only_full = s["linkOnlyFullscreen"].as_bool().unwrap_or(true);
        let files: Vec<String> = s["linkFiles"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        let hit = if !only_full || fg.fullscreen { best_match(&files, &fg.title) } else { None };
        if let Some(f) = hit {
            miss = 0;
            set_link(&app, Some(f));
        } else if active {
            miss += 1;
            if miss >= 3 {
                miss = 0;
                set_link(&app, None);
            }
        }
    }
}

/// 这次按键是否算翻页(鼠标点在悬浮窗上不算)
#[cfg(windows)]
fn on_key<R: Runtime>(app: &AppHandle<R>, code: i32) -> bool {
    if code != 1 {
        return true;
    }
    let Some(w) = app.get_webview_window("widget") else { return true };
    let (Ok(c), Ok(p), Ok(sz)) = (w.cursor_position(), w.outer_position(), w.outer_size()) else { return true };
    !(c.x >= p.x as f64 && c.x <= (p.x + sz.width as i32) as f64 && c.y >= p.y as f64 && c.y <= (p.y + sz.height as i32) as f64)
}
