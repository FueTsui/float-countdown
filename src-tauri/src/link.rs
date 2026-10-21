// 演示联动:检测前台窗口是否在放映已绑定的演示文件,并监听翻页按键。
// 目前只在 Windows 上实现;其他系统上这些函数什么都不做。
#[cfg(windows)]
use crate::{cmd, settings};
use crate::AppState;
#[cfg(windows)]
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub fn set_link<R: Runtime>(app: &AppHandle<R>, active: bool) {
    {
        let st = app.state::<AppState>();
        let mut g = st.inner.lock().unwrap();
        if g.link_active == active {
            return;
        }
        g.link_active = active;
    }
    let _ = app.emit_to("widget", "link", active);
    crate::update_visibility(app);
}

/// 前台窗口标题里是否包含某个已绑定文件的文件名(不含扩展名)
#[cfg(windows)]
fn title_matches(s: &Value, title: &str) -> bool {
    let t = title.to_lowercase();
    if t.is_empty() {
        return false;
    }
    s["linkFiles"].as_array().map_or(false, |a| {
        a.iter().filter_map(Value::as_str).any(|f| {
            let p = std::path::Path::new(f);
            let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            !stem.is_empty() && t.contains(&stem)
        })
    })
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
        let hit = title_matches(&s, &fg.title) && (!only_full || fg.fullscreen);
        if hit {
            miss = 0;
            if !active {
                set_link(&app, true);
            }
        } else if active {
            miss += 1;
            if miss >= 3 {
                miss = 0;
                set_link(&app, false);
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
