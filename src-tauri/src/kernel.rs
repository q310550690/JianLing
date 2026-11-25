//! Windows 内核（WebView2 固定版本运行时）管理：
//! 拉取可下载版本、下载并解包、导入本地目录、删除。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::WindowsKernel;

const DOWNLOAD_PAGE: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2/";
const MARKER: &str = "Microsoft.WebView2.FixedVersionRuntime.";
const ARCHIVE_SUFFIX: &str = ".x64.cab";

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct KernelVersion {
    pub version: String,
    pub url: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KernelProgress {
    stage: String,
    received: u64,
    total: u64,
}

fn kernels_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("无法定位本地数据目录: {e}"))?
        .join("kernels");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// 从微软 WebView2 页面里解析固定版本运行时（x64）的下载直链。
fn parse_versions(html: &str) -> Vec<KernelVersion> {
    let text = html.replace("\\u002F", "/").replace("\\/", "/");
    let mut out: Vec<KernelVersion> = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = text[cursor..].find(MARKER) {
        let start = cursor + found;
        let tail = &text[start + MARKER.len()..];
        let Some(end) = tail.find(ARCHIVE_SUFFIX) else {
            cursor = start + MARKER.len();
            continue;
        };
        let version = tail[..end].to_string();
        let stop = start + MARKER.len() + end + ARCHIVE_SUFFIX.len();
        let valid = version.chars().all(|c| c.is_ascii_digit() || c == '.');
        if valid {
            let before = &text[..start];
            if let Some(scheme) = before.rfind("https://") {
                let url = before[scheme..].to_string() + &text[start..stop];
                if !out.iter().any(|item| item.version == version) {
                    out.push(KernelVersion { version, url });
                }
            }
            cursor = stop;
        } else {
            // 非 x64 的条目（x86/arm64）会先命中，跳过这一个标记继续找
            cursor = start + MARKER.len();
        }
    }
    out
}

#[tauri::command]
pub async fn fetch_windows_kernel_versions() -> Result<Vec<KernelVersion>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 JianLing")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        let html = client
            .get(DOWNLOAD_PAGE)
            .send()
            .map_err(|e| format!("访问微软下载页失败: {e}"))?
            .text()
            .map_err(|e| format!("读取下载页失败: {e}"))?;
        let versions = parse_versions(&html);
        if versions.is_empty() {
            return Err("未从微软页面解析到固定版本运行时，请改用「导入本地目录」".into());
        }
        Ok(versions)
    })
    .await
    .map_err(|e| format!("线程异常: {e}"))?
}

/// 解包 cab，返回含 msedgewebview2.exe 的目录。
fn extract_cab(cab: &Path, dest: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("expand.exe")
        .arg("-F:*")
        .arg(cab)
        .arg(dest)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("调用 expand 解包失败: {e}"))?;
    if !status.success() {
        return Err("cab 解包失败（expand.exe 返回非 0）".into());
    }
    find_runtime_dir(dest).ok_or_else(|| "解包后未找到 msedgewebview2.exe".to_string())
}

fn find_runtime_dir(root: &Path) -> Option<PathBuf> {
    if root.join("msedgewebview2.exe").is_file() {
        return Some(root.to_path_buf());
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("msedgewebview2.exe").is_file() {
                return Some(path);
            }
            if let Some(nested) = find_runtime_dir(&path) {
                return Some(nested);
            }
        }
    }
    None
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                total += dir_size(&entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

fn version_from_path(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    let mut best = String::new();
    let mut current = String::new();
    let mut flush = |buf: &mut String| {
        let candidate = buf.trim_matches('.').to_string();
        buf.clear();
        if candidate.matches('.').count() == 3 && candidate.ends_with(|c: char| c.is_ascii_digit()) {
            best = candidate;
        }
    };
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else {
            flush(&mut current);
        }
    }
    flush(&mut current);
    if best.is_empty() {
        "本地".to_string()
    } else {
        best
    }
}

fn make_entry(version: String, path: &Path, source: &str) -> WindowsKernel {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    WindowsKernel {
        id: format!("k{stamp}"),
        version,
        path: path.to_string_lossy().to_string(),
        source: source.to_string(),
        added_at: stamp as u64,
        size_bytes: dir_size(path),
    }
}

#[tauri::command]
pub async fn download_windows_kernel(
    app: AppHandle,
    version: String,
    url: String,
) -> Result<WindowsKernel, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = kernels_root(&app)?;
        let target = root.join(sanitize(&version));
        if target.is_dir() && target.join("msedgewebview2.exe").is_file() {
            return Ok(make_entry(version, &target, "auto"));
        }

        let cab = std::env::temp_dir().join(format!("jianling-webview2-{}.cab", sanitize(&version)));
        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 JianLing")
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| e.to_string())?;
        let mut response = client
            .get(&url)
            .send()
            .map_err(|e| format!("下载内核失败: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("下载内核失败: HTTP {}", response.status()));
        }
        let total = response.content_length().unwrap_or(0);
        let mut file = std::fs::File::create(&cab).map_err(|e| e.to_string())?;
        let mut buffer = vec![0u8; 256 * 1024];
        let mut received = 0u64;
        loop {
            let read = response.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
            received += read as u64;
            let _ = app.emit(
                "kernel-progress",
                KernelProgress {
                    stage: "download".into(),
                    received,
                    total,
                },
            );
        }
        file.flush().map_err(|e| e.to_string())?;
        drop(file);

        let _ = app.emit(
            "kernel-progress",
            KernelProgress {
                stage: "extract".into(),
                received,
                total,
            },
        );

        if target.exists() {
            std::fs::remove_dir_all(&target).ok();
        }
        let runtime = extract_cab(&cab, &target)?;
        std::fs::remove_file(&cab).ok();
        // 统一成 <root>/<version>/msedgewebview2.exe
        if runtime != target {
            let moved = root.join(format!("{}-tmp", sanitize(&version)));
            std::fs::rename(&runtime, &moved).map_err(|e| format!("整理内核目录失败: {e}"))?;
            std::fs::remove_dir_all(&target).ok();
            std::fs::rename(&moved, &target).map_err(|e| format!("整理内核目录失败: {e}"))?;
        }

        let _ = app.emit(
            "kernel-progress",
            KernelProgress {
                stage: "done".into(),
                received,
                total,
            },
        );
        Ok(make_entry(version, &target, "auto"))
    })
    .await
    .map_err(|e| format!("下载线程异常: {e}"))?
}

/// 导入本地已解压的固定版本运行时目录（含 msedgewebview2.exe）。
#[tauri::command]
pub fn import_windows_kernel(app: AppHandle, dir: String) -> Result<WindowsKernel, String> {
    let source = PathBuf::from(dir.trim());
    if !source.is_dir() {
        return Err(format!("目录不存在：{}", source.display()));
    }
    let runtime = find_runtime_dir(&source)
        .ok_or_else(|| "该目录里没有 msedgewebview2.exe，请选择解压后的运行时根目录".to_string())?;
    let version = version_from_path(&runtime);
    let root = kernels_root(&app)?;
    let target = root.join(sanitize(&version));
    if !(target.is_dir() && target.join("msedgewebview2.exe").is_file()) {
        copy_dir(&runtime, &target)?;
    }
    Ok(make_entry(version, &target, "local"))
}

#[tauri::command]
pub fn delete_windows_kernel(app: AppHandle, id: String, path: String) -> Result<(), String> {
    let _ = id;
    let root = kernels_root(&app)?;
    let target = PathBuf::from(path.trim());
    // 只删除我们自己的内核目录
    if target.starts_with(&root) && target.is_dir() {
        std::fs::remove_dir_all(&target).map_err(|e| format!("删除内核目录失败: {e}"))?;
    }
    Ok(())
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "kernel".to_string()
    } else {
        cleaned
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest).map_err(|e| format!("复制失败 {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixed_version_cab_urls() {
        // 微软页面里 URL 是 \u002F 转义的 JSON 片段
        let html = r#"{"architecture":12,"url":"https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\u002Faaaa\u002FMicrosoft.WebView2.FixedVersionRuntime.157.0.4341.60.x86.cab"},{"architecture":15,"url":"https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\u002Fbbbb\u002FMicrosoft.WebView2.FixedVersionRuntime.157.0.4341.60.x64.cab"},{"url":"https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\u002Fcccc\u002FMicrosoft.WebView2.FixedVersionRuntime.156.0.4314.58.x64.cab"}"#;
        let versions = parse_versions(html);
        assert_eq!(versions.len(), 2, "只取 x64: {versions:?}");
        assert_eq!(versions[0].version, "157.0.4341.60");
        assert!(versions[0]
            .url
            .starts_with("https://msedge.sf.dl.delivery.mp.microsoft.com/"));
        assert!(versions[0].url.ends_with("Microsoft.WebView2.FixedVersionRuntime.157.0.4341.60.x64.cab"));
        assert_eq!(versions[1].version, "156.0.4314.58");
    }

    #[test]
    fn detects_version_from_runtime_path() {
        let path = Path::new("C:/kernels/Microsoft.WebView2.FixedVersionRuntime.157.0.4341.60.x64");
        assert_eq!(version_from_path(path), "157.0.4341.60");
        assert_eq!(version_from_path(Path::new("C:/kernels/WebView2")), "本地");
    }
}
