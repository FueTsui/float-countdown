// 检查更新:读取 GitHub Releases 的最新版本,有新版时下载安装包并运行。
// 不使用 Tauri 自带更新器(它要求额外的签名密钥和 latest.json),下载只信任 https 的 GitHub 地址。
use serde_json::{json, Value};
use std::io::Read;
use tauri::{AppHandle, Runtime};

const API: &str = "https://api.github.com/repos/FueTsui/float-countdown/releases/latest";
const DL_PREFIX: &str = "https://github.com/FueTsui/float-countdown/releases/download/";
const MAX_SIZE: u64 = 200 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("float-countdown-updater")
        .build()
}

/// "v2.1.10" -> [2, 1, 10]
pub(crate) fn parse_ver(s: &str) -> Vec<u64> {
    s.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
}

pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse_ver(latest), parse_ver(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

pub(crate) fn fetch_latest(current: &str) -> Result<Value, String> {
    let r: Value = agent()
        .get(API)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => "还没有发布过版本".to_string(),
            ureq::Error::Status(c, _) => format!("GitHub 返回 {c}"),
            e => format!("无法连接 GitHub:{e}"),
        })?
        .into_json()
        .map_err(|e| e.to_string())?;
    let tag = r["tag_name"].as_str().unwrap_or("").to_string();
    let asset = r["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.ends_with("-setup.exe"))))
        .and_then(|x| x["browser_download_url"].as_str())
        .unwrap_or("")
        .to_string();
    Ok(json!({
        "current": current, "latest": tag.trim_start_matches(['v', 'V']), "newer": is_newer(&tag, current),
        "notes": r["body"].as_str().unwrap_or(""), "page": r["html_url"].as_str().unwrap_or(""), "asset": asset,
    }))
}

pub(crate) fn open_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://github.com/FueTsui/") || url.starts_with("https://fuetsui.com")) {
        return Err("不允许打开该地址".into());
    }
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// 下载安装包到临时目录并运行,随后退出本程序让安装器替换文件(仅 Windows)
pub(crate) fn install<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), String> {
    if !url.starts_with(DL_PREFIX) || !url.ends_with("-setup.exe") {
        return Err("安装包地址不可信".into());
    }
    let resp = agent().get(url).call().map_err(|e| format!("下载失败:{e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_SIZE + 1).read_to_end(&mut buf).map_err(|e| format!("下载失败:{e}"))?;
    if buf.len() as u64 > MAX_SIZE || buf.len() < 100_000 || &buf[..2] != b"MZ" {
        return Err("下载的文件不是有效的安装包".into());
    }
    let path = std::env::temp_dir().join("FloatCountdown-update-setup.exe");
    std::fs::write(&path, &buf).map_err(|e| e.to_string())?;
    std::process::Command::new(&path).spawn().map_err(|e| format!("无法运行安装包:{e}"))?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_compare() {
        assert!(is_newer("v2.1.10", "2.1.9"));
        assert!(is_newer("2.2", "2.1.9"));
        assert!(!is_newer("v2.1.2", "2.1.2"));
        assert!(!is_newer("2.1", "2.1.0"));
        assert!(!is_newer("1.9.9", "2.0.0"));
    }
}
