use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize)]
pub struct InstallProgress {
    pub kind: String,
    pub phase: String,
    pub percent: u32,
    pub message: String,
}

const JDK_URL: &str = "https://aka.ms/download-jdk/microsoft-jdk-21-windows-x64.zip";
const JDK_CN_URL: &str =
    "https://mirrors.huaweicloud.com/openjdk/21.0.2/openjdk-21.0.2_windows-x64_bin.zip";
const CMDLINE_TOOLS_URL: &str =
    "https://dl.google.com/android/repository/commandlinetools-win-11076708_latest.zip";
const CMDLINE_TOOLS_CN_URL: &str =
    "https://mirrors.cloud.tencent.com/AndroidSDK/commandlinetools-win-11076708_latest.zip";

fn emit(app: &AppHandle, kind: &str, phase: &str, percent: u32, message: &str) {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            kind: kind.to_string(),
            phase: phase.to_string(),
            percent,
            message: message.to_string(),
        },
    );
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("jianling-install")
}

fn download_to(app: &AppHandle, url: &str, dest: &Path, kind: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .build()
        .map_err(|e| format!("初始化下载器失败：{e}"))?;
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| format!("连接下载地址失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败：HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::File::create(dest).map_err(|e| format!("创建文件失败：{e}"))?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done: u64 = 0;
    let mut last_percent = u32::MAX;
    loop {
        let read = resp
            .read(&mut buf)
            .map_err(|e| format!("读取下载数据失败：{e}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|e| format!("写入文件失败：{e}"))?;
        done += read as u64;
        let percent = if total > 0 {
            ((done * 100 / total) as u32).min(99)
        } else {
            0
        };
        if percent != last_percent {
            last_percent = percent;
            let message = if total > 0 {
                format!("已下载 {} / {}", mb(done), mb(total))
            } else {
                format!("已下载 {}", mb(done))
            };
            emit(app, kind, "download", percent, &message);
        }
    }
    if done == 0 {
        return Err("下载内容为空，请检查网络".into());
    }
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path, strip_top: bool) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开压缩包失败：{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析压缩包失败：{e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取压缩包条目失败：{e}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let relative = if strip_top {
            let mut components = relative.components();
            components.next();
            components.as_path().to_path_buf()
        } else {
            relative.to_path_buf()
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("复制文件失败：{e}"))?;
        }
    }
    Ok(())
}

fn try_download(app: &AppHandle, urls: &[&str], dest: &Path, kind: &str) -> Result<String, String> {
    let mut last_error = String::new();
    for url in urls {
        emit(app, kind, "download", 0, &format!("尝试下载：{url}"));
        match download_to(app, url, dest, kind) {
            Ok(()) => return Ok((*url).to_string()),
            Err(e) => {
                last_error = e;
                let _ = std::fs::remove_file(dest);
            }
        }
    }
    Err(last_error)
}

fn resolve_target(app: &AppHandle, target_dir: &str, fallback: &str) -> Result<PathBuf, String> {
    let trimmed = target_dir.trim();
    if !trimmed.is_empty() {
        return Ok(PathBuf::from(trimmed));
    }
    let base = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("获取默认目录失败：{e}"))?;
    Ok(base.join(fallback))
}

/// 下载并安装 JDK 21，返回 JDK 根目录。
pub fn install_jdk(app: &AppHandle, target_dir: String, mirror: bool) -> Result<String, String> {
    let target = resolve_target(app, &target_dir, "jdk21")?;
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let zip_path = cache.join("jdk21.zip");

    let urls: Vec<&str> = if mirror {
        vec![JDK_CN_URL, JDK_URL]
    } else {
        vec![JDK_URL]
    };
    try_download(app, &urls, &zip_path, "jdk")?;

    emit(app, "jdk", "extract", 100, "正在解压 JDK…");
    let extract_dir = cache.join("jdk-extract");
    if extract_dir.exists() {
        let _ = std::fs::remove_dir_all(&extract_dir);
    }
    extract_zip(&zip_path, &extract_dir, true)?;

    let java = find_file(&extract_dir, "java.exe").ok_or("解压后未找到 java.exe")?;
    let jdk_home = java
        .parent()
        .and_then(|p| p.parent())
        .ok_or("JDK 目录结构异常")?
        .to_path_buf();

    if target.exists() {
        emit(app, "jdk", "extract", 100, "清理旧版本…");
        std::fs::remove_dir_all(&target).map_err(|e| format!("删除旧 JDK 失败：{e}"))?;
    }
    if std::fs::rename(&jdk_home, &target).is_err() {
        emit(app, "jdk", "extract", 100, "正在复制到目标目录…");
        copy_dir_all(&jdk_home, &target)?;
    }
    let _ = std::fs::remove_dir_all(&extract_dir);
    let _ = std::fs::remove_file(&zip_path);

    emit(app, "jdk", "done", 100, "JDK 安装完成");
    Ok(target.to_string_lossy().to_string())
}

/// 下载 Android cmdline-tools 并安装平台组件，返回 SDK 根目录。
pub fn install_sdk(
    app: &AppHandle,
    target_dir: String,
    jdk_home: String,
    mirror: bool,
) -> Result<String, String> {
    let target = resolve_target(app, &target_dir, "android-sdk")?;
    let jdk = PathBuf::from(jdk_home.trim());
    if jdk_home.trim().is_empty() || !jdk.join("bin").join("java.exe").is_file() {
        return Err("JDK 路径无效，请先安装 JDK 或在上方填写 JDK 目录".into());
    }

    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let zip_path = cache.join("cmdline-tools.zip");

    let urls: Vec<&str> = if mirror {
        vec![CMDLINE_TOOLS_CN_URL, CMDLINE_TOOLS_URL]
    } else {
        vec![CMDLINE_TOOLS_URL]
    };
    try_download(app, &urls, &zip_path, "sdk")?;

    emit(app, "sdk", "extract", 100, "正在解压 cmdline-tools…");
    let tools_dir = target.join("cmdline-tools").join("latest");
    if tools_dir.exists() {
        let _ = std::fs::remove_dir_all(&tools_dir);
    }
    extract_zip(&zip_path, &tools_dir, true)?;

    write_licenses(&target)?;

    let sdkmanager = tools_dir.join("bin").join("sdkmanager.bat");
    if !sdkmanager.is_file() {
        return Err("未找到 sdkmanager.bat，安装包可能不完整".into());
    }

    emit(app, "sdk", "configure", 0, "正在安装 platform-tools / android-34 / build-tools…");
    let packages = ["platform-tools", "platforms;android-34", "build-tools;34.0.0"];
    let mut args: Vec<String> = vec![
        "/c".to_string(),
        sdkmanager.to_string_lossy().to_string(),
        format!("--sdk_root={}", target.to_string_lossy()),
        "--install".to_string(),
    ];
    args.extend(packages.iter().map(|p| (*p).to_string()));

    let output = Command::new("cmd")
        .args(&args)
        .env("JAVA_HOME", &jdk)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("启动 sdkmanager 失败：{e}"))?;

    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let err = String::from_utf8_lossy(&output.stderr);
        let tail: String = text
            .chars()
            .rev()
            .take(800)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return Err(format!(
            "sdkmanager 执行失败（{}）\n{}\n{}",
            output.status, tail, err
        ));
    }

    let _ = std::fs::remove_file(&zip_path);
    emit(app, "sdk", "done", 100, "Android SDK 安装完成");
    Ok(target.to_string_lossy().to_string())
}

/// sdkmanager 首次运行会要求接受许可，这里直接写入已知许可哈希，避免交互阻塞。
fn write_licenses(sdk_root: &Path) -> Result<(), String> {
    let dir = sdk_root.join("licenses");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 licenses 目录失败：{e}"))?;
    let items: [(&str, &str); 4] = [
        (
            "android-sdk-license",
            "8933bad161af4178b1185d1a37fbf41ea5269c55\nd56f5187479451eabf01fb78af6dfcb131a6481e\n24333f8a63b6825ea9c5514f83c2829b004d1fee\n",
        ),
        (
            "android-sdk-preview-license",
            "84831b9409646a918e30573bab4c9c91346d8abd\n",
        ),
        (
            "android-googletv-license",
            "601085b94cd77f0b54ff86406957099ebe79c4d6\n",
        ),
        (
            "intel-android-extra-license",
            "d975f751698a77b662f1254ddbeed3901e976f5a\n",
        ),
    ];
    for (name, body) in items {
        std::fs::write(dir.join(name), body)
            .map_err(|e| format!("写入许可文件失败：{e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn install_jdk_command(
    app: AppHandle,
    target_dir: String,
    mirror: bool,
) -> Result<String, String> {
    install_jdk(&app, target_dir, mirror)
}

#[tauri::command]
pub fn install_sdk_command(
    app: AppHandle,
    target_dir: String,
    jdk_home: String,
    mirror: bool,
) -> Result<String, String> {
    install_sdk(&app, target_dir, jdk_home, mirror)
}
