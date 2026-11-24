use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

use crate::config;

/// 一台已连接的设备。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbDevice {
    pub serial: String,
    /// device | offline | unauthorized
    pub state: String,
    pub model: String,
    pub product: String,
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn run(path: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("执行 {path} 失败：{e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if err.is_empty() { out } else { err });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// 依次尝试：手动填写 → SDK 目录 → 常见安装位置 → PATH。
fn resolve_adb(explicit: Option<&str>, sdk: Option<&str>) -> Result<String, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(raw) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(path.to_string_lossy().to_string());
        }
        candidates.push(path);
    }
    for root in [sdk.map(str::trim).filter(|s| !s.is_empty()), std::env::var("ANDROID_HOME").ok().as_deref(), std::env::var("ANDROID_SDK_ROOT").ok().as_deref()] {
        if let Some(root) = root {
            candidates.push(Path::new(root).join("platform-tools").join("adb.exe"));
        }
    }
    candidates.push(PathBuf::from(r"D:\Software\Development\SDK\Android\platform-tools\adb.exe"));
    candidates.push(PathBuf::from(r"C:\Android\platform-tools\adb.exe"));
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    // 交给 PATH：能跑起来就能用
    Ok("adb".to_string())
}

pub(crate) fn adb_for(app: &AppHandle) -> Result<String, String> {
    let settings = config::load_settings(app.clone()).unwrap_or_default();
    resolve_adb(settings.adb_path.as_deref(), settings.sdk_path.as_deref())
}

#[tauri::command]
pub fn adb_devices(app: AppHandle) -> Result<Vec<AdbDevice>, String> {
    let adb = adb_for(&app)?;
    let out = run(&adb, &["devices", "-l"])?;
    let mut list = Vec::new();
    for line in out.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('*') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let Some(state) = parts.next() else { continue };
        let mut model = String::new();
        let mut product = String::new();
        for extra in parts {
            if let Some(v) = extra.strip_prefix("model:") {
                model = v.replace('_', " ");
            } else if let Some(v) = extra.strip_prefix("product:") {
                product = v.to_string();
            }
        }
        list.push(AdbDevice {
            serial: serial.to_string(),
            state: state.to_string(),
            model,
            product,
        });
    }
    Ok(list)
}

/// 读取设备当前 System WebView 版本，例如 `101.0.4951.61`。
#[tauri::command]
pub fn adb_webview_version(app: AppHandle, serial: String) -> Result<String, String> {
    let adb = adb_for(&app)?;
    let out = run(&adb, &["-s", &serial, "shell", "dumpsys", "webviewupdate"])?;
    for line in out.lines() {
        let line = line.trim();
        if !line.starts_with("Current WebView package") {
            continue;
        }
        // (com.android.webview, 101.0.4951.61)
        let Some(start) = line.find('(') else { continue };
        let inner = &line[start + 1..line.rfind(')').unwrap_or(line.len())];
        let mut parts = inner.split(',').map(str::trim);
        let _name = parts.next();
        if let Some(version) = parts.next() {
            return Ok(version.to_string());
        }
    }
    Err("未取到 WebView 版本（设备可能未启用 WebView 调试或系统版本较低）".into())
}

/// 读取最近的 logcat 文本（一次性尾部，前端定时轮询）。
#[tauri::command]
pub fn adb_logcat_tail(
    app: AppHandle,
    serial: String,
    lines: Option<u32>,
    filter: Option<String>,
) -> Result<String, String> {
    let adb = adb_for(&app)?;
    let count = lines.unwrap_or(300).clamp(50, 3000).to_string();
    let mut text = run(&adb, &["-s", &serial, "logcat", "-d", "-t", &count, "-v", "time"])?;
    if let Some(keyword) = filter.map(|f| f.trim().to_string()).filter(|f| !f.is_empty()) {
        text = text
            .lines()
            .filter(|line| line.contains(&keyword))
            .collect::<Vec<_>>()
            .join("\n");
    }
    Ok(text)
}

#[tauri::command]
pub fn adb_logcat_clear(app: AppHandle, serial: String) -> Result<(), String> {
    let adb = adb_for(&app)?;
    run(&adb, &["-s", &serial, "logcat", "-c"])?;
    Ok(())
}

/// 安装（覆盖）APK 到指定设备。
#[tauri::command]
pub fn adb_install_apk(app: AppHandle, serial: String, path: String) -> Result<String, String> {
    let adb = adb_for(&app)?;
    if !Path::new(&path).is_file() {
        return Err(format!("APK 不存在：{path}"));
    }
    let out = run(&adb, &["-s", &serial, "install", "-r", &path])?;
    let text = out.trim().to_string();
    if text.contains("Success") {
        Ok(text)
    } else {
        Err(text)
    }
}

/// 启动设备上的应用。
#[tauri::command]
pub fn adb_launch_app(app: AppHandle, serial: String, package: String) -> Result<(), String> {
    let adb = adb_for(&app)?;
    run(
        &adb,
        &[
            "-s",
            &serial,
            "shell",
            "monkey",
            "-p",
            &package,
            "-c",
            "android.intent.category.LAUNCHER",
            "1",
        ],
    )?;
    Ok(())
}

/// 从给定候选包名里挑出设备上已安装的那些（正式包 / 调试包判断用）。
#[tauri::command]
pub fn adb_installed_packages(
    app: AppHandle,
    serial: String,
    packages: Vec<String>,
) -> Result<Vec<String>, String> {
    let adb = adb_for(&app)?;
    let out = run(&adb, &["-s", &serial, "shell", "pm", "list", "packages"])?;
    let installed: Vec<String> = packages
        .into_iter()
        .filter(|name| {
            let needle = format!("package:{name}");
            out.lines().any(|line| line.trim() == needle)
        })
        .collect();
    Ok(installed)
}

/// 锁屏会让页面 visible:false，CDP 部分域被节流（看着像卡死）；这里一键常亮。
#[tauri::command]
pub fn adb_keep_screen(app: AppHandle, serial: String, on: bool) -> Result<(), String> {
    let adb = adb_for(&app)?;
    run(
        &adb,
        &[
            "-s",
            &serial,
            "shell",
            "svc",
            "power",
            "stayon",
            if on { "true" } else { "false" },
        ],
    )?;
    Ok(())
}

/// 复制文本到系统剪贴板（Chromium 不允许命令行直接打开 devtools:// / chrome:// 地址，
/// 只能把地址放进剪贴板让用户粘贴）。
#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", "clip"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("复制失败：{e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("复制失败：{e}"))?;
    }
    child.wait().map_err(|e| format!("复制失败：{e}"))?;
    Ok(())
}
