use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn default_true() -> bool {
    true
}

fn default_cache_mode() -> String {
    "default".to_string()
}

fn default_ua_mode() -> String {
    "append".to_string()
}

fn default_min_sdk() -> u32 {
    24
}

fn default_target_sdk() -> u32 {
    34
}

fn default_compile_sdk() -> u32 {
    34
}

fn default_progress_color() -> String {
    "#2E7D32".to_string()
}

fn default_progress_height() -> u32 {
    2
}

fn default_status_bar_css_var() -> String {
    "--status-bar-height".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidBuildRequest {
    pub app_name: String,
    /// 项目 id（只用于构建水印溯源）
    #[serde(default)]
    pub project_id: String,
    pub package_name: String,
    pub version_name: String,
    pub version_code: u32,
    pub start_url: String,
    /// 附加到启动地址的查询参数（可选），如 key=value
    #[serde(default)]
    pub start_query: String,
    pub fullscreen: bool,
    pub status_bar_color: String,
    #[serde(default = "default_true")]
    pub edge_to_edge: bool,
    #[serde(default = "default_status_bar_css_var")]
    pub status_bar_css_var: String,
    #[serde(default)]
    pub hide_navigation_bar: bool,
    #[serde(default)]
    pub screen_orientation: String,
    pub ignore_ssl_errors: bool,
    pub allow_cleartext: bool,
    pub permissions: Vec<String>,
    pub icon_path: Option<String>,
    pub icon_background_color: String,
    pub source_kind: String,
    pub local_dir: Option<String>,
    pub text_zoom: u32,
    pub allow_autoplay: bool,
    pub allow_zoom: bool,
    pub disable_long_press: bool,
    pub hide_scrollbars: bool,
    pub allow_third_party_cookies: bool,
    pub video_fullscreen: bool,
    pub debug_enabled: bool,
    /// 构建调试包：包名加 .debug 后缀（可与正式包共存）、名称加「调试」、强制开启 WebView 调试
    #[serde(default)]
    pub debug_build: bool,
    /// 调试包注入的页内调试面板：vconsole | eruda | none
    #[serde(default = "default_debug_console")]
    pub debug_console: String,
    #[serde(default)]
    pub open_external_in_browser: bool,
    #[serde(default = "default_cache_mode")]
    pub cache_mode: String,
    #[serde(default = "default_ua_mode")]
    pub ua_mode: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub pull_to_refresh: bool,
    #[serde(default = "default_true")]
    pub exit_on_back_pressed: bool,
    #[serde(default)]
    pub disable_back_exit: bool,
    #[serde(default)]
    pub progress_bar: bool,
    #[serde(default = "default_progress_color")]
    pub progress_color: String,
    #[serde(default = "default_progress_height")]
    pub progress_height: u32,
    #[serde(default)]
    pub nav_bar: bool,
    #[serde(default = "default_min_sdk")]
    pub min_sdk: u32,
    #[serde(default = "default_target_sdk")]
    pub target_sdk: u32,
    #[serde(default = "default_compile_sdk")]
    pub compile_sdk: u32,
    #[serde(default)]
    pub light_status_bar: bool,
    #[serde(default)]
    pub keep_screen_on: bool,
    #[serde(default)]
    pub secure_flag: bool,
    #[serde(default)]
    pub local_server: crate::config::LocalServerConfig,
    #[serde(default)]
    pub splash: crate::config::SplashConfig,
    pub permission_prompts: std::collections::HashMap<String, crate::config::PermissionPrompt>,
    pub keystore_path: Option<String>,
    pub keystore_alias: Option<String>,
    pub keystore_store_password: Option<String>,
    pub keystore_key_password: Option<String>,
    pub sdk_path: Option<String>,
    pub jdk_path: Option<String>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildResult {
    /// 本次构建的水印 ID
    pub build_id: String,
    pub apk_path: String,
    pub size_bytes: u64,
    pub signed: bool,
    pub log_path: String,
    pub work_dir: String,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLine {
    stream: String,
    line: String,
}

fn validate(req: &AndroidBuildRequest) -> Result<(), String> {
    let pkg = req.package_name.trim();
    let segments: Vec<&str> = pkg.split('.').collect();
    let valid = segments.len() >= 2
        && segments.iter().all(|seg| {
            !seg.is_empty()
                && seg.starts_with(|c: char| c.is_ascii_alphabetic())
                && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
    if !valid {
        return Err("包名不合法：需形如 com.example.app，至少两段，每段以字母开头且只含字母数字下划线".into());
    }
    if req.app_name.trim().is_empty() {
        return Err("应用名不能为空".into());
    }
    if req.source_kind == "local" {
        let dir = req
            .local_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("已选择本地静态包模式，但未指定 H5 目录或 zip 文件")?;
        if !Path::new(dir).exists() {
            return Err(format!("本地 H5 路径不存在：{dir}"));
        }
    } else if !(req.start_url.starts_with("http://") || req.start_url.starts_with("https://")) {
        return Err("H5 首页地址需以 http:// 或 https:// 开头".into());
    }
    if let Some(icon) = req.icon_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if !Path::new(icon).is_file() {
            return Err(format!("图标文件不存在：{icon}"));
        }
    }
    if let Some(ks) = req.keystore_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if !Path::new(ks).is_file() {
            return Err(format!("证书文件不存在：{ks}"));
        }
        if req
            .keystore_alias
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return Err("已选择证书，但未填写别名（alias）".into());
        }
        if req
            .keystore_store_password
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return Err("已选择证书，但未填写证书密码".into());
        }
        if req
            .keystore_key_password
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return Err("已选择证书，但未填写密钥密码".into());
        }
    }
    Ok(())
}

fn prune_builds(root: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("build-"))
                .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    if dirs.len() < keep {
        return;
    }
    let keep_from = dirs.len().saturating_sub(keep - 1);
    for old in &dirs[..keep_from] {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// 把每项权限的弹窗配置序列化成 JSON，模板里用 org.json 解析。
fn permission_prompts_json(
    map: &std::collections::HashMap<String, crate::config::PermissionPrompt>,
) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string())
}

fn manifest_permissions(perms: &[String]) -> String {
    let has = |name: &str| perms.iter().any(|p| p == name);
    let mut lines: Vec<String> = vec![
        "    <!-- 网络（必需） -->".to_string(),
        "    <uses-permission android:name=\"android.permission.INTERNET\" />".to_string(),
        "    <uses-permission android:name=\"android.permission.ACCESS_NETWORK_STATE\" />".to_string(),
        "    <uses-permission android:name=\"android.permission.WAKE_LOCK\" />".to_string(),
    ];

    if has("camera") {
        lines.push(String::new());
        lines.push("    <!-- 相机 -->".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.CAMERA\" />".to_string());
        lines.push(
            "    <uses-feature android:name=\"android.hardware.camera\" android:required=\"false\" />"
                .to_string(),
        );
        lines.push(
            "    <uses-feature android:name=\"android.hardware.camera.autofocus\" android:required=\"false\" />"
                .to_string(),
        );
    }
    if has("microphone") {
        lines.push(String::new());
        lines.push("    <!-- 麦克风 -->".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.RECORD_AUDIO\" />".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.MODIFY_AUDIO_SETTINGS\" />".to_string(),
        );
        lines.push(
            "    <uses-feature android:name=\"android.hardware.microphone\" android:required=\"false\" />"
                .to_string(),
        );
    }
    if has("storage") {
        lines.push(String::new());
        lines.push("    <!-- 文件读写 -->".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.READ_EXTERNAL_STORAGE\" android:maxSdkVersion=\"32\" />"
                .to_string(),
        );
        lines.push(
            "    <uses-permission android:name=\"android.permission.WRITE_EXTERNAL_STORAGE\" android:maxSdkVersion=\"28\" />"
                .to_string(),
        );
        lines.push("    <uses-permission android:name=\"android.permission.READ_MEDIA_IMAGES\" />".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.READ_MEDIA_VIDEO\" />".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.READ_MEDIA_AUDIO\" />".to_string());
    }
    if has("location") {
        lines.push(String::new());
        lines.push("    <!-- 定位 -->".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.ACCESS_FINE_LOCATION\" />".to_string(),
        );
        lines.push(
            "    <uses-permission android:name=\"android.permission.ACCESS_COARSE_LOCATION\" />".to_string(),
        );
    }
    if has("notification") {
        lines.push(String::new());
        lines.push("    <!-- 通知 -->".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.POST_NOTIFICATIONS\" />".to_string(),
        );
    }
    if has("bluetooth") {
        lines.push(String::new());
        lines.push("    <!-- 蓝牙（Web Bluetooth / 外设） -->".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.BLUETOOTH_SCAN\" android:usesPermissionFlags=\"neverForLocation\" />"
                .to_string(),
        );
        lines.push("    <uses-permission android:name=\"android.permission.BLUETOOTH_CONNECT\" />".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.BLUETOOTH\" android:maxSdkVersion=\"30\" />"
                .to_string(),
        );
        lines.push(
            "    <uses-permission android:name=\"android.permission.BLUETOOTH_ADMIN\" android:maxSdkVersion=\"30\" />"
                .to_string(),
        );
        lines.push(
            "    <uses-feature android:name=\"android.hardware.bluetooth\" android:required=\"false\" />"
                .to_string(),
        );
    }
    if has("vibrate") {
        lines.push(String::new());
        lines.push("    <!-- 振动 -->".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.VIBRATE\" />".to_string());
    }
    if has("nfc") {
        lines.push(String::new());
        lines.push("    <!-- NFC（Web NFC 需要） -->".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.NFC\" />".to_string());
        lines.push(
            "    <uses-feature android:name=\"android.hardware.nfc\" android:required=\"false\" />".to_string(),
        );
    }
    if has("foreground") {
        lines.push(String::new());
        lines.push("    <!-- 前台服务（后台通话 / 录音保活） -->".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE\" />".to_string(),
        );
        lines.push(
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE_MICROPHONE\" />"
                .to_string(),
        );
        lines.push(
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE_CAMERA\" />"
                .to_string(),
        );
    }
    if has("call") {
        lines.push(String::new());
        lines.push("    <!-- 直接拨号（tel: 链接） -->".to_string());
        lines.push("    <uses-permission android:name=\"android.permission.CALL_PHONE\" />".to_string());
    }
    if has("install") {
        lines.push(String::new());
        lines.push("    <!-- 应用内安装 APK（自更新） -->".to_string());
        lines.push(
            "    <uses-permission android:name=\"android.permission.REQUEST_INSTALL_PACKAGES\" />".to_string(),
        );
    }

    lines.join("\r\n")
}

fn runtime_permissions(perms: &[String]) -> String {
    let has = |name: &str| perms.iter().any(|p| p == name);
    let mut entries: Vec<&str> = Vec::new();

    if has("camera") {
        entries.push("Manifest.permission.CAMERA");
    }
    if has("microphone") {
        entries.push("Manifest.permission.RECORD_AUDIO");
    }
    if has("storage") {
        entries.push("Manifest.permission.READ_EXTERNAL_STORAGE");
        entries.push("Manifest.permission.WRITE_EXTERNAL_STORAGE");
        entries.push("Manifest.permission.READ_MEDIA_IMAGES");
        entries.push("Manifest.permission.READ_MEDIA_VIDEO");
        entries.push("Manifest.permission.READ_MEDIA_AUDIO");
    }
    if has("location") {
        entries.push("Manifest.permission.ACCESS_FINE_LOCATION");
        entries.push("Manifest.permission.ACCESS_COARSE_LOCATION");
    }
    if has("notification") {
        entries.push("Manifest.permission.POST_NOTIFICATIONS");
    }
    if has("bluetooth") {
        entries.push("Manifest.permission.BLUETOOTH_SCAN");
        entries.push("Manifest.permission.BLUETOOTH_CONNECT");
    }

    if entries.is_empty() {
        return "            // 未勾选任何运行时权限".to_string();
    }
    entries
        .iter()
        .map(|e| format!("            {e}"))
        .collect::<Vec<_>>()
        .join(",\r\n")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "\\'")
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn collect_text_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_text_files(&path, out);
        } else {
            let is_text = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("java") | Some("xml") | Some("gradle") | Some("properties")
            );
            if is_text {
                out.push(path);
            }
        }
    }
}

fn escape_java(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "")
        .replace('\n', "\\n")
}

fn install_local_content(work_dir: &Path, source: &Path) -> Result<(), String> {
    let dest = work_dir.join("app/src/main/assets/www");
    write_local_content(&dest, source)?;
    if !dest.join("index.html").is_file() {
        return Err("本地 H5 根目录缺少 index.html".into());
    }
    Ok(())
}

/// 把启动屏图片复制到目标目录，返回复制后的文件名。
fn copy_splash_image(
    source: &Option<String>,
    dest_dir: &Path,
    stem: &str,
) -> Result<Option<String>, String> {
    let Some(raw) = source.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let path = Path::new(raw);
    if !path.is_file() {
        return Err(format!("启动屏图片不存在：{raw}"));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| "png".to_string());
    let name = format!("{stem}.{ext}");
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("创建启动屏目录失败：{e}"))?;
    std::fs::copy(path, dest_dir.join(&name)).map_err(|e| format!("复制启动屏图片失败：{e}"))?;
    Ok(Some(name))
}

fn splash_title(cfg: &crate::config::SplashConfig, app_name: &str) -> String {
    let title = cfg.title.trim();
    if title.is_empty() {
        app_name.trim().to_string()
    } else {
        title.to_string()
    }
}

/// 启动屏背景色按 CSS 规则输出（#RRGGBB 或 #RRGGBBAA），与 Android 资源用的 ARGB 不同。
fn splash_background(cfg: &crate::config::SplashConfig) -> String {
    let raw = cfg.background_color.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return format!("#{}", hex.to_uppercase());
    }
    "#FFFFFF".to_string()
}

/// 写入启动屏配置与图片，供 MainActivity 读取 assets 使用。
fn write_android_splash(
    work_dir: &Path,
    cfg: &crate::config::SplashConfig,
    app_name: &str,
    icon_path: Option<&str>,
) -> Result<(), String> {
    let assets = work_dir.join("app/src/main/assets");
    std::fs::create_dir_all(&assets).map_err(|e| format!("创建 assets 目录失败：{e}"))?;
    let custom_mode = cfg.mode.trim() == "custom";
    let image = if custom_mode {
        copy_splash_image(&cfg.image_path, &assets, "splash")?
    } else {
        None
    };
    let background_image = if custom_mode {
        None
    } else {
        copy_splash_image(&cfg.background_image, &assets, "splash-bg")?
    };
    // 没配 logo 时用应用图标
    let logo_source = cfg
        .logo_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            icon_path
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        });
    let logo = copy_splash_image(&logo_source, &assets, "logo")?;
    let value = serde_json::json!({
        "enabled": cfg.enabled,
        "mode": if custom_mode { "custom" } else { "default" },
        "title": splash_title(cfg, app_name),
        "background": splash_background(cfg),
        "backgroundImage": background_image,
        "image": image,
        "logo": logo,
        "minShowMs": cfg.min_show_ms.min(10000)
    });
    let raw = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(assets.join("jianling-splash.json"), raw)
        .map_err(|e| format!("写入启动屏配置失败：{e}"))
}

/// 写入本地服务配置，供 MainActivity 里的 LocalServer 在运行时读取。
fn write_android_local_server(work_dir: &Path, cfg: &crate::config::LocalServerConfig) -> Result<(), String> {
    let proxies: Vec<serde_json::Value> = cfg
        .proxies
        .iter()
        .filter(|rule| !rule.prefix.trim().is_empty() && !rule.target.trim().is_empty())
        .map(|rule| {
            serde_json::json!({
                "prefix": rule.prefix.trim(),
                "target": rule.target.trim(),
                "rewriteOrigin": rule.rewrite_origin,
                "insecure": rule.insecure,
                "resolveIp": rule.resolve_ip.trim(),
                "headers": rule.headers.trim()
            })
        })
        .collect();
    let value = serde_json::json!({
        "enabled": cfg.enabled,
        "port": cfg.port,
        "basePath": crate::config::normalize_base_path(&cfg.base_path),
        "proxies": proxies
    });
    let raw = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    let path = work_dir.join("app/src/main/assets/jianling-local-server.json");
    std::fs::write(&path, raw).map_err(|e| format!("写入本地服务配置失败：{e}"))
}

/// 把本地目录或 zip 内容写入 dest（Android 的 assets/www 与 Windows 的 www 共用）。
pub(crate) fn write_local_content(dest: &Path, source: &Path) -> Result<(), String> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| format!("清理目标目录失败: {e}"))?;
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("创建目标目录失败: {e}"))?;

    if source.is_dir() {
        copy_dir(source, dest).map_err(|e| format!("复制本地目录失败: {e}"))?;
    } else {
        let file = std::fs::File::open(source).map_err(|e| format!("打开 zip 失败: {e}"))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("解析 zip 失败: {e}"))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| format!("读取 zip 条目失败: {e}"))?;
            let Some(relative) = entry.enclosed_name() else {
                continue;
            };
            let target = dest.join(&relative);
            if entry.is_dir() {
                std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut out =
                    std::fs::File::create(&target).map_err(|e| format!("写入失败: {e}"))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压失败: {e}"))?;
            }
        }
    }
    Ok(())
}

fn apply_placeholders(root: &Path, map: &[(&str, String)]) -> Result<(), String> {
    let mut files = Vec::new();
    collect_text_files(root, &mut files);
    for file in files {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("读取 {} 失败: {e}", file.display()))?;
        let mut replaced = text.clone();
        for (key, value) in map {
            replaced = replaced.replace(key, value);
        }
        if replaced != text {
            std::fs::write(&file, replaced)
                .map_err(|e| format!("写入 {} 失败: {e}", file.display()))?;
        }
    }
    Ok(())
}

fn decode(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => {
            let (cow, _, _) = encoding_rs::GBK.decode(&bytes);
            cow.to_string()
        }
    }
}

fn stream_output<R: Read + Send + 'static>(
    reader: R,
    app: AppHandle,
    stream: &'static str,
    sink: Arc<Mutex<String>>,
) {
    let mut buf = BufReader::new(reader);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match buf.read_until(b'\n', &mut raw) {
            Ok(0) => break,
            Ok(_) => {
                let line = decode(raw.clone())
                    .trim_end_matches(['\r', '\n'])
                    .to_string();
                if line.is_empty() {
                    continue;
                }
                if let Ok(mut log) = sink.lock() {
                    log.push_str(&line);
                    log.push('\n');
                }
                let _ = app.emit(
                    "build-log",
                    LogLine {
                        stream: stream.to_string(),
                        line,
                    },
                );
            }
            Err(_) => break,
        }
    }
}

fn template_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource) = app.path().resource_dir() {
        let candidates = [
            resource.join("resources").join("android-template"),
            resource.join("android-template"),
        ];
        for candidate in candidates {
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("android-template");
    if dev.is_dir() {
        return Ok(dev);
    }
    Err("找不到内置 Android 模板目录，请重新安装应用".into())
}

fn default_debug_console() -> String {
    "eruda".to_string()
}

fn debug_inject_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource) = app.path().resource_dir() {
        let candidates = [
            resource.join("resources").join("debug-inject"),
            resource.join("debug-inject"),
        ];
        for candidate in candidates {
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("debug-inject");
    if dev.is_dir() {
        return Ok(dev);
    }
    Err("找不到内置调试面板资源目录，请重新安装应用".into())
}

/// 调试包启动后自动初始化的引导脚本（vConsole / eruda 通用）。
const DEBUG_CONSOLE_BOOT: &str = r#"(function () {
  try {
    if (window.VConsole) {
      window.__debugConsole = new VConsole();
      return;
    }
    if (window.eruda) {
      // 用 Eruda 自带形态：面板由它右下角的开关显隐，不再叠加自定义按钮
      eruda.init();
      window.__debugConsole = eruda;
    }
  } catch (e) {
    console.error("[debug-console] 初始化失败", e);
  }
})();
"#;

/// 把页内调试面板拷进 assets/www 并注入到 index.html 末尾。
fn inject_debug_console(app: &AppHandle, work_dir: &Path, kind: &str) -> Result<(), String> {
    let lib = match kind {
        "eruda" => "eruda.min.js",
        _ => "vconsole.min.js",
    };
    let source = debug_inject_dir(app)?.join(lib);
    if !source.is_file() {
        return Err(format!("调试面板文件缺失：{}", source.display()));
    }
    let www = work_dir.join("app/src/main/assets/www");
    std::fs::copy(&source, www.join("__debug-console.js"))
        .map_err(|e| format!("复制调试面板失败: {e}"))?;
    std::fs::write(www.join("__debug-console-boot.js"), DEBUG_CONSOLE_BOOT)
        .map_err(|e| format!("写入调试面板引导脚本失败: {e}"))?;

    let index = www.join("index.html");
    let bytes = std::fs::read(&index).map_err(|e| format!("读取 index.html 失败: {e}"))?;
    let html = String::from_utf8_lossy(&bytes).into_owned();
    let snippet = "<script src=\"__debug-console.js\"></script>\n\
                   <script src=\"__debug-console-boot.js\"></script>\n";
    let updated = match html.rfind("</body>") {
        Some(pos) => format!("{}{}{}", &html[..pos], snippet, &html[pos..]),
        None => format!("{html}\n{snippet}"),
    };
    std::fs::write(&index, updated).map_err(|e| format!("注入调试面板失败: {e}"))?;
    Ok(())
}

fn normalize_color(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "#FF000000".to_string();
    }
    if let Some(hex) = trimmed.strip_prefix('#') {
        let body = hex.trim();
        if body.len() == 6 && body.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("#FF{}", body.to_uppercase());
        }
        if body.len() == 8 && body.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("#{}", body.to_uppercase());
        }
    }
    "#FF000000".to_string()
}

/// Windows 壳把进度条颜色直接拼进 CSS：要 #RRGGBB（CSS 的 8 位是 RGBA，不是 ARGB）。
/// 兼容误传的 #AARRGGBB（alpha 为 FF 时丢掉前缀），其余回退默认绿。
fn normalize_css_color(input: &str) -> String {
    let hex = input.trim().trim_start_matches('#').to_uppercase();
    let valid = |s: &str| s.chars().all(|c| c.is_ascii_hexdigit());
    if hex.len() == 8 && valid(&hex) && hex.starts_with("FF") {
        return format!("#{}", &hex[2..]);
    }
    if (hex.len() == 6 || hex.len() == 8) && valid(&hex) {
        return format!("#{hex}");
    }
    "#2E7D32".to_string()
}

/// 注入给页面的状态栏高度变量名：必须是合法的 CSS 自定义属性（`--name`），
/// 非法字符直接剔除，空值回退 `--status-bar-height`（uni-app 同名约定）。
fn status_bar_css_var(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return default_status_bar_css_var();
    }
    let prefixed = if trimmed.starts_with("--") {
        trimmed.to_string()
    } else {
        format!("--{trimmed}")
    };
    let cleaned: String = prefixed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.trim_start_matches('-').is_empty() {
        default_status_bar_css_var()
    } else {
        cleaned
    }
}

/// 屏幕方向：auto 交给系统（跟随旋转设置），其余锁定方向。
fn android_orientation(value: &str) -> String {
    match value.trim() {
        "landscape" => "landscape".to_string(),
        "portrait" => "portrait".to_string(),
        _ => "unspecified".to_string(),
    }
}

fn sanitize_filename(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('_');
    if trimmed.is_empty() {
        "app".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 自动生成（或复用）调试签名证书，保证未配置证书时产物仍可安装。
fn ensure_debug_keystore(cache_root: &Path, jdk: Option<&Path>) -> Result<PathBuf, String> {
    let path = cache_root.join("jianling-debug.keystore");
    if path.is_file() {
        return Ok(path);
    }
    let exe = jdk
        .map(|j| {
            j.join("bin")
                .join(if cfg!(windows) { "keytool.exe" } else { "keytool" })
        })
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("keytool"));
    let mut cmd = Command::new(&exe);
    cmd.arg("-genkeypair")
        .arg("-keystore")
        .arg(&path)
        .args([
            "-storetype",
            "PKCS12",
            "-storepass",
            "android",
            "-keypass",
            "android",
            "-alias",
            "androiddebugkey",
            "-keyalg",
            "RSA",
            "-keysize",
            "2048",
            "-validity",
            "10000",
            "-dname",
            "CN=Android Debug,O=Android,C=US",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("自动生成调试签名失败（找不到 keytool）：{e}"))?;
    if !status.success() || !path.is_file() {
        return Err("自动生成调试签名失败，请在项目里配置签名证书".into());
    }
    Ok(path)
}

/// 生成传统密度图标（API < 26 的启动器用）与圆形图标。
/// `background` 有值时把 source 当作前景图叠在纯色背景上（模板默认图标走这条）。
fn generate_legacy_icons(
    icon: &Path,
    res_dir: &Path,
    background: Option<[u8; 3]>,
) -> Result<usize, String> {
    let source = image::open(icon)
        .map_err(|e| format!("无法解析图标 {}: {e}", icon.display()))?
        .to_rgba8();
    let densities = [
        ("mipmap-mdpi", 48u32),
        ("mipmap-hdpi", 72),
        ("mipmap-xhdpi", 96),
        ("mipmap-xxhdpi", 144),
        ("mipmap-xxxhdpi", 192),
    ];
    let mut count = 0usize;
    for (dir, size) in densities {
        let out_dir = res_dir.join(dir);
        std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
        let canvas = match background {
            Some(bg) => {
                let mut canvas =
                    image::RgbaImage::from_pixel(size, size, image::Rgba([bg[0], bg[1], bg[2], 255]));
                let inner = ((size as f32) * 0.72).round().max(1.0) as u32;
                let scaled = image::imageops::resize(
                    &source,
                    inner,
                    inner,
                    image::imageops::FilterType::Lanczos3,
                );
                let offset = ((size - inner) / 2) as i64;
                image::imageops::overlay(&mut canvas, &scaled, offset, offset);
                canvas
            }
            None => image::imageops::resize(
                &source,
                size,
                size,
                image::imageops::FilterType::Lanczos3,
            ),
        };
        let path = out_dir.join("ic_launcher.png");
        canvas
            .save(&path)
            .map_err(|e| format!("写入 {} 失败: {e}", path.display()))?;
        let round_path = out_dir.join("ic_launcher_round.png");
        round_mask(&canvas)
            .save(&round_path)
            .map_err(|e| format!("写入 {} 失败: {e}", round_path.display()))?;
        count += 2;
    }
    Ok(count)
}

/// 解析 #RGB / #RRGGBB / #RRGGBBAA
fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let raw = value.trim().trim_start_matches('#');
    let full = match raw.len() {
        3 => raw.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => raw[..6].to_string(),
        _ => return None,
    };
    let r = u8::from_str_radix(&full[0..2], 16).ok()?;
    let g = u8::from_str_radix(&full[2..4], 16).ok()?;
    let b = u8::from_str_radix(&full[4..6], 16).ok()?;
    Some([r, g, b])
}

/// 圆外像素置透明（边缘 1px 内线性过渡，避免锯齿）。
fn round_mask(src: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = src.dimensions();
    let mut out = src.clone();
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let radius = w.min(h) as f32 / 2.0;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let factor = (radius - dist).clamp(0.0, 1.0);
            let pixel = out.get_pixel_mut(x, y);
            pixel[3] = (pixel[3] as f32 * factor) as u8;
        }
    }
    out
}

pub fn run_build(app: &AppHandle, req: AndroidBuildRequest) -> Result<BuildResult, String> {
    let started = SystemTime::now();
    validate(&req)?;

    // 调试包：包名加 .debug 便于与正式包共存，名称加后缀便于在桌面区分
    let debug_build = req.debug_build;
    let base_package = req.package_name.trim().to_string();
    let package_name = if debug_build && !base_package.is_empty() {
        format!("{base_package}.debug")
    } else {
        base_package.clone()
    };
    let app_name = if debug_build {
        format!("{} (调试)", req.app_name.trim())
    } else {
        req.app_name.clone()
    };

    let template = template_dir(app)?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("无法定位缓存目录: {e}"))?
        .join("builds");
    std::fs::create_dir_all(&cache_root).map_err(|e| e.to_string())?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let work_dir = cache_root.join(format!("build-{stamp}"));
    if work_dir.exists() {
        std::fs::remove_dir_all(&work_dir).map_err(|e| e.to_string())?;
    }
    prune_builds(&cache_root, 5);

    let _ = app.emit(
        "build-log",
        LogLine {
            stream: "info".to_string(),
            line: format!("准备构建目录：{}", work_dir.display()),
        },
    );

    copy_dir(&template, &work_dir).map_err(|e| format!("复制模板失败: {e}"))?;

    // 包名目录
    let old_pkg_dir = work_dir.join("app/src/main/java/__PACKAGE_PATH__");
    let pkg_rel: Vec<&str> = package_name.split('.').collect();
    let mut new_pkg_dir = work_dir.join("app/src/main/java");
    for seg in &pkg_rel {
        new_pkg_dir.push(seg);
    }
    if let Some(parent) = new_pkg_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&old_pkg_dir, &new_pkg_dir).map_err(|e| format!("整理包目录失败: {e}"))?;

    let ua_suffix = format!(" {}/{}", sanitize_filename(&app_name), req.version_name);
    let mixed_mode = if req.allow_cleartext {
        "WebSettings.MIXED_CONTENT_ALWAYS_ALLOW"
    } else {
        "WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE"
    };
    let use_asset_loader = req.source_kind == "local";
    let start_url = if use_asset_loader {
        "https://appassets.androidplatform.net/assets/www/index.html".to_string()
    } else {
        req.start_url.trim().to_string()
    };
    let start_query = req
        .start_query
        .trim()
        .trim_start_matches(|c: char| c == '?' || c == '&')
        .to_string();

    // SDK 版本：给出上下限，避免写成 Gradle 不认的组合
    let compile_sdk = req.compile_sdk.clamp(30, 36);
    let target_sdk = req.target_sdk.clamp(21, compile_sdk);
    let min_sdk = req.min_sdk.clamp(21, target_sdk);
    let cache_mode = match req.cache_mode.trim() {
        "no_cache" => "no_cache",
        "cache_first" => "cache_first",
        _ => "default",
    }
    .to_string();
    let ua_mode = if req.ua_mode.trim() == "override" {
        "override".to_string()
    } else {
        "append".to_string()
    };

    let build_id = crate::watermark::new_build_id(
        &format!("{}-{}", req.package_name.trim(), req.app_name.trim()),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    );
    // 构建元数据：写进包内 build.json / 清单 meta / 页面痕迹（图片盲水印只放 ID）
    let proxy_count = req
        .local_server
        .proxies
        .iter()
        .filter(|r| !r.prefix.trim().is_empty() && !r.target.trim().is_empty())
        .count();
    let output_name = format!(
        "{}-{}{}.apk",
        sanitize_filename(&app_name),
        sanitize_filename(&req.version_name),
        if debug_build { "-debug" } else { "" }
    );
    let build_meta = crate::watermark::BuildMeta::new(
        build_id.clone(),
        app_name.clone(),
        package_name.clone(),
        req.version_name.trim().to_string(),
        req.version_code,
        "android",
        req.project_id.trim(),
        &output_name,
        proxy_count,
    );
    let build_time = build_meta.created_at.to_string();
    if use_asset_loader {
        let source = req
            .local_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("已选择本地静态包模式，但未指定 H5 目录或 zip")?;
        install_local_content(&work_dir, Path::new(source))?;
        write_android_local_server(&work_dir, &req.local_server)?;
        if req.local_server.enabled {
            let count = req
                .local_server
                .proxies
                .iter()
                .filter(|r| !r.prefix.trim().is_empty() && !r.target.trim().is_empty())
                .count();
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "info".to_string(),
                    line: format!("已启用本地服务：静态托管 + {count} 条转发规则"),
                },
            );
        }
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "info".to_string(),
                line: format!("已导入本地 H5 内容：{source}"),
            },
        );

        let console_kind = req.debug_console.trim().to_lowercase();
        if debug_build && console_kind != "none" && !console_kind.is_empty() {
            inject_debug_console(app, &work_dir, &console_kind)?;
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "info".to_string(),
                    line: format!("已注入页内调试面板：{console_kind}"),
                },
            );
        }
    } else if debug_build {
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "warn".to_string(),
                line: "远程站点模式无法注入页内调试面板，仅开启 WebView 调试".to_string(),
            },
        );
    }

    // 页面痕迹 + 隐藏构建信息：本地静态包才改得了 index.html
    if use_asset_loader {
        let www = work_dir.join("app/src/main/assets/www");
        match crate::watermark::watermark_index_html(&www, &build_meta) {
            Ok(true) => {
                let _ = app.emit(
                    "build-log",
                    LogLine {
                        stream: "info".to_string(),
                        line: format!("已写入页面水印：{build_id}"),
                    },
                );
            }
            Ok(false) => {}
            Err(e) => {
                let _ = app.emit(
                    "build-log",
                    LogLine {
                        stream: "warn".to_string(),
                        line: format!("页面水印写入失败（不影响打包）：{e}"),
                    },
                );
            }
        }
    } else {
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "info".to_string(),
                line: "远程站点模式无法改写页面，仅写入包内水印".to_string(),
            },
        );
    }

    // 包内元数据：assets/.jianling/build.json，解压即可看到
    let meta_dir = work_dir.join("app/src/main/assets/.jianling");
    if let Err(e) = std::fs::create_dir_all(&meta_dir) {
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "warn".to_string(),
                line: format!("创建水印目录失败：{e}"),
            },
        );
    } else {
        if let Err(e) = std::fs::write(meta_dir.join("build.json"), build_meta.pretty()) {
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "warn".to_string(),
                    line: format!("写入包内水印失败：{e}"),
                },
            );
        }
    }

    let map: Vec<(&str, String)> = vec![
        ("__BUILD_ID__", build_id.clone()),
        ("__BUILD_VERSION__", xml_escape(req.version_name.trim())),
        ("__BUILD_TIME__", build_time.clone()),
        ("__BUILD_APP__", xml_escape(&app_name)),
        ("__APPLICATION_ID__", package_name.clone()),
        (
            "__PROJECT_NAME__",
            sanitize_filename(&app_name).replace(' ', ""),
        ),
        ("__APP_NAME__", xml_escape(&app_name)),
        ("__FILE_CHOOSER_TITLE__", "选择文件".to_string()),
        ("__START_URL__", start_url),
        ("__START_QUERY__", escape_java(&start_query)),
        ("__FULLSCREEN__", req.fullscreen.to_string()),
        ("__STATUS_BAR_COLOR__", normalize_color(&req.status_bar_color)),
        ("__EDGE_TO_EDGE__", req.edge_to_edge.to_string()),
        ("__STATUS_BAR_CSS_VAR__", status_bar_css_var(&req.status_bar_css_var)),
        ("__SCREEN_ORIENTATION__", android_orientation(&req.screen_orientation)),
        ("__IGNORE_SSL_ERRORS__", req.ignore_ssl_errors.to_string()),
        ("__UA_SUFFIX__", ua_suffix),
        ("__MIXED_CONTENT_MODE__", mixed_mode.to_string()),
        ("__USES_CLEARTEXT__", req.allow_cleartext.to_string()),
        ("__ALLOW_BACKUP__", "true".to_string()),
        ("__COMPILE_SDK__", compile_sdk.to_string()),
        ("__MIN_SDK__", min_sdk.to_string()),
        ("__TARGET_SDK__", target_sdk.to_string()),
        ("__VERSION_CODE__", req.version_code.to_string()),
        ("__VERSION_NAME__", req.version_name.trim().to_string()),
        ("__PERMISSIONS__", manifest_permissions(&req.permissions)),
        ("__RUNTIME_PERMISSIONS__", runtime_permissions(&req.permissions)),
        (
            "__ICON_BACKGROUND__",
            // 自定义图标保持自带的透明底：自适应图标背景层用透明色，
            // 否则透明 PNG 后面会透出一块纯色，看起来像给图标垫了底色
            if req
                .icon_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_some()
            {
                "#00000000".to_string()
            } else {
                normalize_color(&req.icon_background_color)
            },
        ),
        ("__USE_ASSET_LOADER__", use_asset_loader.to_string()),
        ("__TEXT_ZOOM__", req.text_zoom.clamp(50, 200).to_string()),
        ("__MEDIA_GESTURE__", (!req.allow_autoplay).to_string()),
        ("__ALLOW_ZOOM__", req.allow_zoom.to_string()),
        ("__DISABLE_LONG_PRESS__", req.disable_long_press.to_string()),
        ("__HIDE_SCROLLBARS__", req.hide_scrollbars.to_string()),
        (
            "__THIRD_PARTY_COOKIES__",
            req.allow_third_party_cookies.to_string(),
        ),
        ("__VIDEO_FULLSCREEN__", req.video_fullscreen.to_string()),
        (
            "__DEBUG_ENABLED__",
            (req.debug_enabled || debug_build).to_string(),
        ),
        (
            "__OPEN_EXTERNAL_IN_BROWSER__",
            req.open_external_in_browser.to_string(),
        ),
        ("__CACHE_MODE__", cache_mode.clone()),
        ("__UA_MODE__", ua_mode.clone()),
        ("__USER_AGENT__", escape_java(req.user_agent.trim())),
        ("__PULL_TO_REFRESH__", req.pull_to_refresh.to_string()),
        ("__EXIT_ON_BACK__", req.exit_on_back_pressed.to_string()),
        ("__DISABLE_BACK_EXIT__", req.disable_back_exit.to_string()),
        ("__PROGRESS_BAR__", req.progress_bar.to_string()),
        (
            "__PROGRESS_COLOR__",
            normalize_color(&req.progress_color),
        ),
        (
            "__PROGRESS_HEIGHT__",
            req.progress_height.clamp(1, 12).to_string(),
        ),
        ("__NAV_BAR__", req.nav_bar.to_string()),
        ("__HIDE_NAV_BAR__", req.hide_navigation_bar.to_string()),
        (
            "__LIGHT_STATUS_BAR__",
            req.light_status_bar.to_string(),
        ),
        ("__KEEP_SCREEN_ON__", req.keep_screen_on.to_string()),
        ("__SECURE_FLAG__", req.secure_flag.to_string()),
        (
            "__PERMISSION_PROMPTS_JSON__",
            escape_java(&permission_prompts_json(&req.permission_prompts)),
        ),
    ];
    apply_placeholders(&work_dir, &map)?;

    if let Some(icon) = req.icon_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let dst = work_dir.join("app/src/main/res/drawable-nodpi/ic_launcher_foreground_img.png");
        std::fs::copy(icon, &dst).map_err(|e| format!("写入应用图标失败: {e}"))?;
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "info".to_string(),
                line: format!("已应用自定义图标：{icon}"),
            },
        );
    }

    // 传统密度图标 + 圆形图标（API < 26 启动器用）；没自定义图标时用模板默认图
    let res_dir = work_dir.join("app/src/main/res");
    let custom_icon = req
        .icon_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let icon_source = custom_icon
        .map(PathBuf::from)
        .unwrap_or_else(|| res_dir.join("drawable-nodpi/ic_launcher_foreground_img.png"));
    let icon_background = if custom_icon.is_some() {
        None
    } else {
        parse_hex_color(&req.icon_background_color)
    };
    match generate_legacy_icons(&icon_source, &res_dir, icon_background) {
        Ok(count) => {
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "info".to_string(),
                    line: format!("已生成 {count} 张密度图标（含圆形）"),
                },
            );
        }
        Err(err) => {
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "warn".to_string(),
                    line: format!("跳过密度图标：{err}"),
                },
            );
        }
    }

    // 启动屏资源与配置（图片直接进 APK 的 assets）
    write_android_splash(
        &work_dir,
        &req.splash,
        &req.app_name,
        req.icon_path.as_deref(),
    )?;
    if req.splash.enabled {
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "info".to_string(),
                line: "已启用启动屏".to_string(),
            },
        );
    }

    // 启动屏图片的频域盲水印（仅 PNG；图片太小时自动跳过）
    let assets_dir = work_dir.join("app/src/main/assets");
    if let Ok(entries) = std::fs::read_dir(&assets_dir) {
        let mut marked = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let is_target = (name.starts_with("splash") || name.starts_with("logo"))
                && name.to_lowercase().ends_with(".png");
            if !is_target {
                continue;
            }
            match crate::watermark::watermark_png_file(&path, &build_id) {
                Ok(true) => marked += 1,
                Ok(false) => {}
                Err(e) => {
                    let _ = app.emit(
                        "build-log",
                        LogLine {
                            stream: "warn".to_string(),
                            line: format!("图片盲水印写入失败 {name}：{e}"),
                        },
                    );
                }
            }
        }
        if marked > 0 {
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "info".to_string(),
                    line: format!("已写入 {marked} 张图片盲水印"),
                },
            );
        }
    }

    // Android SDK 定位
    let sdk = req
        .sdk_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var("ANDROID_HOME").ok().map(PathBuf::from))
        .or_else(|| std::env::var("ANDROID_SDK_ROOT").ok().map(PathBuf::from))
        .ok_or("未配置 Android SDK 路径，请先在环境检测页指定")?;
    if !sdk.is_dir() {
        return Err(format!("Android SDK 目录不存在：{}", sdk.display()));
    }

    let sdk_prop = sdk.to_string_lossy().replace('\\', "\\\\").replace(':', "\\:");
    std::fs::write(work_dir.join("local.properties"), format!("sdk.dir={sdk_prop}\n"))
        .map_err(|e| format!("写入 local.properties 失败: {e}"))?;

    // JDK
    let jdk = req
        .jdk_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var("JAVA_HOME").ok().map(PathBuf::from));

    let gradlew = work_dir.join("gradlew.bat");
    if !gradlew.is_file() {
        return Err("模板缺少 gradlew.bat".into());
    }

    let log_sink = Arc::new(Mutex::new(String::new()));
    let mut command = Command::new(&gradlew);
    command
        .arg("-p")
        .arg(&work_dir)
        .arg(":app:assembleRelease")
        .arg("--no-daemon")
        .arg("--console=plain")
        .env("ANDROID_HOME", &sdk)
        .env("ANDROID_SDK_ROOT", &sdk)
        .current_dir(&work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(jdk) = &jdk {
        command.env("JAVA_HOME", jdk);
    }

    let keystore = req
        .keystore_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match keystore {
        Some(ks) => {
            let store_pass = req.keystore_store_password.clone().unwrap_or_default();
            let key_pass = req.keystore_key_password.clone().unwrap_or_default();
            // 诊断用指纹（不含明文）：签名失败时能核对「传进来的密码」和「保存的密码」是否同一个值
            let fingerprint = |s: &str| -> String {
                let sum: u32 = s.bytes().map(|b| b as u32).sum();
                let first = s.chars().next().map(|c| c as u32).unwrap_or(0);
                let last = s.chars().last().map(|c| c as u32).unwrap_or(0);
                format!(
                    "len={} sum={} first={} last={} clean={}",
                    s.len(),
                    sum,
                    first,
                    last,
                    s.trim() == s
                )
            };
            eprintln!(
                "[ks-debug] store({}) key({}) alias({}) path({})",
                fingerprint(&store_pass),
                fingerprint(&key_pass),
                req.keystore_alias.clone().unwrap_or_default(),
                ks
            );
            command
                .env("JIANLING_KS_PATH", ks)
                .env("JIANLING_KS_STORE_PASS", store_pass)
                .env(
                    "JIANLING_KS_ALIAS",
                    req.keystore_alias.clone().unwrap_or_default(),
                )
                .env("JIANLING_KS_KEY_PASS", key_pass);
        }
        None => {
            // 没配证书时用自动生成的调试签名，保证产物可安装
            let debug_ks = ensure_debug_keystore(&cache_root, jdk.as_deref())?;
            command
                .env("JIANLING_KS_PATH", &debug_ks)
                .env("JIANLING_KS_STORE_PASS", "android")
                .env("JIANLING_KS_ALIAS", "androiddebugkey")
                .env("JIANLING_KS_KEY_PASS", "android");
            let _ = app.emit(
                "build-log",
                LogLine {
                    stream: "info".to_string(),
                    line: format!(
                        "未配置签名证书，已使用自动生成的调试签名（可安装，不可上架）：{}",
                        debug_ks.display()
                    ),
                },
            );
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let _ = app.emit(
        "build-log",
        LogLine {
            stream: "info".to_string(),
            line: "开始执行 Gradle 构建…".to_string(),
        },
    );

    let mut child = command
        .spawn()
        .map_err(|e| format!("启动 Gradle 失败: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut handles = Vec::new();
    if let Some(out) = stdout {
        handles.push(std::thread::spawn({
            let app = app.clone();
            let sink = Arc::clone(&log_sink);
            move || stream_output(out, app, "out", sink)
        }));
    }
    if let Some(err) = stderr {
        handles.push(std::thread::spawn({
            let app = app.clone();
            let sink = Arc::clone(&log_sink);
            move || stream_output(err, app, "err", sink)
        }));
    }

    let status = child.wait().map_err(|e| format!("等待构建结束失败: {e}"))?;
    for handle in handles {
        let _ = handle.join();
    }

    let log_path = work_dir.join("build.log");
    if let Ok(log) = log_sink.lock() {
        let _ = std::fs::write(&log_path, log.as_str());
    }

    if !status.success() {
        let _ = app.emit(
            "build-log",
            LogLine {
                stream: "err".to_string(),
                line: format!("构建失败，退出码 {:?}", status.code()),
            },
        );
        return Err(format!(
            "Gradle 构建失败（退出码 {:?}），完整日志见 {}",
            status.code(),
            log_path.display()
        ));
    }

    let release_dir = work_dir.join("app/build/outputs/apk/release");
    let signed_name = release_dir.join("app-release.apk");
    let unsigned_name = release_dir.join("app-release-unsigned.apk");
    let apk = if signed_name.is_file() {
        signed_name
    } else if unsigned_name.is_file() {
        unsigned_name
    } else {
        return Err("构建成功但未找到 APK 产物".into());
    };

    let output_dir = req
        .output_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| work_dir.clone());
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("创建输出目录失败: {e}"))?;

    let file_name = format!(
        "{}-{}{}.apk",
        sanitize_filename(&app_name),
        sanitize_filename(&req.version_name),
        if debug_build { "-debug" } else { "" }
    );
    let target = output_dir.join(file_name);
    std::fs::copy(&apk, &target).map_err(|e| format!("复制产物失败: {e}"))?;

    let size_bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    let duration_secs = SystemTime::now()
        .duration_since(started)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let _ = app.emit(
        "build-log",
        LogLine {
            stream: "info".to_string(),
            line: format!("构建完成：{}", target.display()),
        },
    );

    Ok(BuildResult {
        build_id: build_id.clone(),
        apk_path: target.to_string_lossy().to_string(),
        size_bytes,
        signed: keystore.is_some(),
        log_path: log_path.to_string_lossy().to_string(),
        work_dir: work_dir.to_string_lossy().to_string(),
        duration_secs,
    })
}

#[tauri::command]
pub async fn build_android(
    app: AppHandle,
    request: AndroidBuildRequest,
) -> Result<BuildResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_build(&app, request))
        .await
        .map_err(|e| format!("构建线程异常: {e}"))?
}

// ============================================================================
// Windows 桌面套壳：复制 shell-host.exe + H5 内容 + shell.json
// ============================================================================
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsBuildResult {
    /// 本次构建的水印 ID
    pub build_id: String,
    pub dir: String,
    pub exe_path: String,
    pub size_bytes: u64,
    pub signed: bool,
    pub duration_secs: u64,
    pub log_path: String,
    /// 单文件 exe（内嵌页面资源）
    pub single_exe: Option<String>,
    /// NSIS 安装包
    pub installer: Option<String>,
    /// 绿色版 zip
    pub portable_zip: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsBuildRequest {
    pub app_name: String,
    /// 项目 id（只用于构建水印溯源）
    #[serde(default)]
    pub project_id: String,
    pub version_name: String,
    /// 产物输出目录（项目级，必填）
    pub output_dir: Option<String>,
    pub source_kind: String,
    pub start_url: String,
    /// 附加到启动地址的查询参数（可选），如 key=value
    #[serde(default)]
    pub start_query: String,
    pub local_dir: Option<String>,
    /// 应用图标（启动屏 logo 缺省时使用）
    #[serde(default)]
    pub icon_path: Option<String>,
    pub size_mode: String,
    pub width: u32,
    pub height: u32,
    pub aspect_w: u32,
    pub aspect_h: u32,
    pub resizable: bool,
    pub frameless: bool,
    pub always_on_top: bool,
    pub maximized: bool,
    pub debug_enabled: bool,
    pub single_instance: bool,
    pub auto_start: bool,
    pub minimize_on_close: bool,
    /// 关闭按钮行为：exit | minimize | tray（空 = 按 minimizeOnClose 推断）
    #[serde(default)]
    pub close_mode: String,
    pub zoom: f64,
    pub autoplay: bool,
    pub block_context_menu: bool,
    pub block_text_select: bool,
    pub hide_scrollbars: bool,
    pub block_ctrl_zoom: bool,
    pub block_browser_keys: bool,
    pub external_links_in_browser: bool,
    pub min_font_size: u32,
    #[serde(default)]
    pub ignore_cert_errors: bool,
    #[serde(default = "default_true")]
    pub progress_bar: bool,
    #[serde(default)]
    pub nav_bar: bool,
    #[serde(default = "default_progress_color")]
    pub progress_color: String,
    #[serde(default = "default_progress_height")]
    pub progress_height: u32,
    #[serde(default)]
    pub ua_mode: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default = "default_true")]
    pub download_to_subfolder: bool,
    #[serde(default)]
    pub min_width: u32,
    #[serde(default)]
    pub min_height: u32,
    #[serde(default)]
    pub remember_window: bool,
    pub local_server: crate::config::LocalServerConfig,
    #[serde(default)]
    pub splash: crate::config::SplashConfig,
    pub permissions: crate::config::WindowsPermissions,
    /// 产物形态：单文件 exe / 安装包 / 绿色版 zip
    #[serde(default)]
    pub output_single: bool,
    #[serde(default)]
    pub output_installer: bool,
    #[serde(default)]
    pub output_portable: bool,
    /// 安装包安装范围：user | machine（默认 user）
    #[serde(default)]
    pub install_scope: String,
    /// 自带的 Windows 内核目录（含 msedgewebview2.exe），空 = 跟随系统
    #[serde(default)]
    pub kernel_path: Option<String>,
    pub cert_path: Option<String>,
    pub cert_password: Option<String>,
}

fn shell_host_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("shell-host.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
        let nested = resource_dir.join("binaries").join("shell-host.exe");
        if nested.is_file() {
            return Ok(nested);
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/shell-host.exe");
    if dev.is_file() {
        return Ok(dev);
    }
    Err("未找到 shell-host.exe，请先执行：npm run sync-host".into())
}

fn emit_win_log(app: &AppHandle, stream: &str, line: &str) {
    let _ = app.emit(
        "build-log-windows",
        LogLine {
            stream: stream.to_string(),
            line: line.to_string(),
        },
    );
}

/// 在 PATH 与 Windows SDK 安装目录里找 signtool.exe。
fn find_signtool() -> Option<PathBuf> {
    if let Ok(output) = Command::new("where").arg("signtool.exe").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(first) = text.lines().map(str::trim).find(|l| !l.is_empty()) {
                let path = PathBuf::from(first);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    for key in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Ok(base) = std::env::var(key) else {
            continue;
        };
        let bin = Path::new(&base).join("Windows Kits").join("10").join("bin");
        let Ok(entries) = std::fs::read_dir(&bin) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        versions.sort();
        versions.reverse();
        for version in versions {
            for arch in ["x64", "x86"] {
                let candidate = version.join(arch).join("signtool.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn sign_windows_exe(exe: &Path, cert: &Path, password: &str) -> Result<(), String> {
    let signtool =
        find_signtool().ok_or("未找到 signtool.exe，请安装 Windows SDK（含签名工具）或加入 PATH")?;

    let run = |timestamp: bool| -> Result<std::process::Output, String> {
        let mut cmd = Command::new(&signtool);
        cmd.arg("sign")
            .arg("/f")
            .arg(cert)
            .arg("/p")
            .arg(password)
            .arg("/fd")
            .arg("sha256");
        if timestamp {
            cmd.arg("/tr")
                .arg("http://timestamp.digicert.com")
                .arg("/td")
                .arg("sha256");
        }
        cmd.arg(exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.output().map_err(|e| format!("启动 signtool 失败：{e}"))
    };

    let with_ts = run(true)?;
    if with_ts.status.success() {
        return Ok(());
    }
    // 时间戳服务器不可达时退回不带时间戳，保证签名本身成功
    let plain = run(false)?;
    if plain.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&plain.stderr);
    let stdout = String::from_utf8_lossy(&plain.stdout);
    Err(format!(
        "签名失败：{}",
        if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        }
    ))
}

/// 递归把目录写进 zip；include_root_exe=false 时跳过根目录的 exe（单文件版自己不能套自己）。
fn zip_add_dir<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    base: &Path,
    dir: &Path,
    include_root_exe: bool,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("读取目录失败：{e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            zip_add_dir(writer, base, &path, include_root_exe)?;
            continue;
        }
        let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
        let is_root_exe = rel.components().count() == 1
            && path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("exe"))
                .unwrap_or(false);
        if is_root_exe && !include_root_exe {
            continue;
        }
        let name = rel.to_string_lossy().replace('\\', "/");
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file(name, options)
            .map_err(|e| format!("写入压缩条目失败：{e}"))?;
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, writer).map_err(|e| format!("写入压缩条目失败：{e}"))?;
    }
    Ok(())
}

/// 把目录打成 zip 包。
fn zip_directory(src: &Path, dest: &Path, include_root_exe: bool) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(dest).map_err(|e| format!("创建压缩包失败：{e}"))?;
    let mut writer = zip::ZipWriter::new(file);
    zip_add_dir(&mut writer, src, src, include_root_exe)?;
    writer
        .finish()
        .map_err(|e| format!("写入压缩包失败：{e}"))?;
    Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
}

/// 单文件版：宿主 exe + 内嵌 zip + 16 字节尾部（标记 + 长度），运行期自解压到缓存目录。
fn write_single_exe(host: &Path, payload: &Path, dest: &Path) -> Result<u64, String> {
    use std::io::Write;
    std::fs::copy(host, dest).map_err(|e| format!("复制宿主失败：{e}"))?;
    let len = std::fs::metadata(payload)
        .map_err(|e| e.to_string())?
        .len();
    let mut out = std::fs::OpenOptions::new()
        .append(true)
        .open(dest)
        .map_err(|e| format!("打开单文件产物失败：{e}"))?;
    let mut src = std::fs::File::open(payload).map_err(|e| e.to_string())?;
    std::io::copy(&mut src, &mut out).map_err(|e| format!("写入内嵌内容失败：{e}"))?;
    out.write_all(b"APSHPAY1")
        .and_then(|_| out.write_all(&len.to_le_bytes()))
        .and_then(|_| out.flush())
        .map_err(|e| format!("写入尾部标记失败：{e}"))?;
    Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
}

/// 找 NSIS：PATH 优先，其次 Tauri 自带的 NSIS 目录。
fn find_makensis() -> Option<PathBuf> {
    if let Ok(output) = Command::new("where").arg("makensis.exe").output() {
        if output.status.success() {
            let first = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            if let Some(path) = first.filter(|p| p.is_file()) {
                return Some(path);
            }
        }
    }
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let candidate = PathBuf::from(local)
        .join("tauri")
        .join("NSIS")
        .join("makensis.exe");
    candidate.is_file().then_some(candidate)
}

/// 生成 NSIS 安装包：默认按当前用户装到 %LOCALAPPDATA%\Programs，可切全机器安装。
fn build_installer(
    dir: &Path,
    app_name: &str,
    exe_stem: &str,
    version: &str,
    dest: &Path,
    machine_scope: bool,
) -> Result<u64, String> {
    let makensis = find_makensis().ok_or("未找到 makensis.exe（NSIS），无法生成安装包")?;
    let exe_name = format!("{exe_stem}.exe");
    let src_dir = dir.display().to_string();
    let out_file = dest.display().to_string();

    let (level, install_dir, reg_root, ctx) = if machine_scope {
        (
            "admin",
            format!("$PROGRAMFILES64\\{app_name}"),
            "HKLM",
            "  SetShellVarContext all\n",
        )
    } else {
        (
            "user",
            format!("$LOCALAPPDATA\\Programs\\{app_name}"),
            "HKCU",
            "",
        )
    };
    // 有 app.ico 就用它做安装包/卸载图标，没有则用 NSIS 默认
    let icon_defs = if dir.join("app.ico").is_file() {
        let ico = dir.join("app.ico").display().to_string();
        format!("!define MUI_ICON \"{ico}\"\n!define MUI_UNICON \"{ico}\"\n")
    } else {
        String::new()
    };

    let script = format!(
        r#"Unicode true
!include "MUI2.nsh"
{icon_defs}
Name "{app_name}"
OutFile "{out_file}"
InstallDir "{install_dir}"
InstallDirRegKey {reg_root} "Software\{app_name}" "InstallDir"
RequestExecutionLevel {level}
SetCompressor /SOLID lzma
ShowInstDetails hide

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

Section "Install"
{ctx}  ; 覆盖安装前先结束正在运行的实例，避免文件被占用
  nsExec::ExecToLog 'taskkill /F /IM "{exe_name}"'
  SetOutPath "$INSTDIR"
  File /r "{src_dir}\*.*"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$DESKTOP\{app_name}.lnk" "$INSTDIR\{exe_name}"
  CreateDirectory "$SMPROGRAMS\{app_name}"
  CreateShortcut "$SMPROGRAMS\{app_name}\{app_name}.lnk" "$INSTDIR\{exe_name}"
  CreateShortcut "$SMPROGRAMS\{app_name}\卸载 {app_name}.lnk" "$INSTDIR\uninstall.exe"
  WriteRegStr {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "DisplayName" "{app_name}"
  WriteRegStr {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "DisplayVersion" "{version}"
  WriteRegStr {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "DisplayIcon" "$INSTDIR\{exe_name}"
  WriteRegStr {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "InstallLocation" "$INSTDIR"
  WriteRegStr {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "NoModify" 1
  WriteRegDWORD {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}" "NoRepair" 1
  WriteRegStr {reg_root} "Software\{app_name}" "InstallDir" "$INSTDIR"
SectionEnd

Section "Uninstall"
{ctx}  nsExec::ExecToLog 'taskkill /F /IM "{exe_name}"'
  Delete "$DESKTOP\{app_name}.lnk"
  RMDir /r "$SMPROGRAMS\{app_name}"
  RMDir /r "$INSTDIR"
  DeleteRegKey {reg_root} "Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_name}"
  DeleteRegKey {reg_root} "Software\{app_name}"
SectionEnd
"#
    );
    let script_path = dest.with_extension("nsi");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.as_bytes());
    std::fs::write(&script_path, bytes).map_err(|e| format!("写入安装脚本失败：{e}"))?;
    let output = Command::new(&makensis)
        .arg("/V2")
        .arg(&script_path)
        .output()
        .map_err(|e| format!("调用 makensis 失败：{e}"))?;
    let _ = std::fs::remove_file(&script_path);
    if !dest.is_file() {
        let detail = String::from_utf8_lossy(&output.stdout);
        return Err(format!("安装包生成失败：{}", detail.trim()));
    }
    Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
}

fn run_build_windows(
    app: &AppHandle,
    request: WindowsBuildRequest,
) -> Result<WindowsBuildResult, String> {
    let started = SystemTime::now();
    let mut log_lines: Vec<String> = Vec::new();
    let mut push = |app: &AppHandle, stream: &str, line: String| {
        emit_win_log(app, stream, &line);
        log_lines.push(line);
    };

    if request.app_name.trim().is_empty() {
        return Err("应用名不能为空".into());
    }
    push(
        app,
        "info",
        format!("开始构建 Windows 版本：{}", request.app_name.trim()),
    );

    let exe_src = shell_host_path(app)?;
    push(app, "info", format!("宿主程序：{}", exe_src.display()));

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let build_id = crate::watermark::new_build_id(
        &format!("{}|windows", request.app_name.trim()),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    );
    let base_name = sanitize_filename(&request.app_name).replace(' ', "_");
    let version = sanitize_filename(request.version_name.trim()).replace(' ', "_");
    let win_proxy_count = request
        .local_server
        .proxies
        .iter()
        .filter(|r| !r.prefix.trim().is_empty() && !r.target.trim().is_empty())
        .count();
    let exe_name = format!("{}.exe", base_name.replace('_', ""));
    let build_meta = crate::watermark::BuildMeta::new(
        build_id.clone(),
        request.app_name.trim().to_string(),
        exe_name.clone(),
        request.version_name.trim().to_string(),
        0,
        "windows",
        request.project_id.trim(),
        &exe_name,
        win_proxy_count,
    );
    let out_root = request
        .output_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or("未指定产物输出目录")?;
    if !(request.output_single || request.output_installer || request.output_portable) {
        return Err("至少选择一种产物（单文件 exe / 安装包 / 绿色版 zip）".into());
    }
    std::fs::create_dir_all(&out_root).map_err(|e| format!("创建输出目录失败：{e}"))?;
    // 中间目录放临时区：只有勾选的产物会写进输出目录
    let dir = std::env::temp_dir()
        .join("jianling-build")
        .join(format!("{base_name}-{version}-{stamp}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建输出目录失败：{e}"))?;
    push(app, "info", format!("输出目录：{}", out_root.display()));

    let exe_dst = dir.join(format!("{}.exe", base_name.replace('_', "")));
    std::fs::copy(&exe_src, &exe_dst).map_err(|e| format!("复制宿主程序失败：{e}"))?;
    push(app, "info", "已复制宿主程序".to_string());

    // 宿主 exe 全项目共用，图标与版本信息按项目改写资源
    let icon_bytes = request
        .icon_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|path| crate::winres::ico_bytes(Path::new(path)))
        .transpose();
    let icon_bytes = match icon_bytes {
        Ok(bytes) => bytes,
        Err(e) => {
            push(app, "warn", format!("图标转换失败，跳过自定义图标：{e}"));
            None
        }
    };
    if let Some(bytes) = icon_bytes.as_deref() {
        if let Err(e) = std::fs::write(dir.join("app.ico"), bytes) {
            push(app, "warn", format!("写出 app.ico 失败：{e}"));
        }
    }
    match crate::winres::stamp_exe(
        &exe_dst,
        icon_bytes.as_deref(),
        request.app_name.trim(),
        &version,
        &build_id,
    ) {
        Ok(()) => push(
            app,
            "info",
            format!("已写入 exe 图标与版本信息（水印 {build_id}）"),
        ),
        Err(e) => push(app, "warn", format!("写入 exe 资源失败：{e}")),
    }
    // 尾部 overlay：签名前写入，签名会把它一起纳入哈希
    if let Err(e) = crate::watermark::append_overlay(&exe_dst, &build_meta) {
        push(app, "warn", format!("写入 exe 水印失败：{e}"));
    }

    let local_index = if request.source_kind == "local" {
        let source = request
            .local_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("已选择本地静态包模式，但未指定 H5 目录或 zip")?;
        let www = dir.join("www");
        write_local_content(&www, Path::new(source))?;
        if !www.join("index.html").is_file() {
            return Err("本地 H5 根目录缺少 index.html".into());
        }
        match crate::watermark::watermark_index_html(&www, &build_meta) {
            Ok(true) => push(app, "info", format!("已写入页面水印：{build_id}")),
            Ok(false) => {}
            Err(e) => push(app, "warn", format!("页面水印写入失败（不影响打包）：{e}")),
        }
        push(app, "info", format!("已导入本地 H5 内容：{source}"));
        Some("www/index.html".to_string())
    } else {
        if !(request.start_url.starts_with("http://") || request.start_url.starts_with("https://")) {
            return Err("H5 首页地址需以 http:// 或 https:// 开头".into());
        }
        push(app, "info", format!("远程地址：{}", request.start_url.trim()));
        None
    };

    let aspect_w = request.aspect_w.max(1);
    let aspect_h = request.aspect_h.max(1);
    let width = request.width.clamp(320, 7680);
    let (win_width, win_height, fullscreen) = match request.size_mode.as_str() {
        "ratio" => {
            let height = ((width as f64) * (aspect_h as f64) / (aspect_w as f64)).round() as u32;
            push(
                app,
                "info",
                format!("窗口尺寸：按 {aspect_w}:{aspect_h} 固定比例，{width}×{height}"),
            );
            (width, height.clamp(240, 4320), false)
        }
        "fullscreen" => {
            push(app, "info", "窗口尺寸：全屏".to_string());
            (width, request.height.clamp(240, 4320), true)
        }
        _ => {
            let height = request.height.clamp(240, 4320);
            push(app, "info", format!("窗口尺寸：{width}×{height}"));
            (width, height, false)
        }
    };

    let proxies: Vec<serde_json::Value> = request
        .local_server
        .proxies
        .iter()
        .filter(|rule| !rule.prefix.trim().is_empty() && !rule.target.trim().is_empty())
        .map(|rule| {
            serde_json::json!({
                "prefix": rule.prefix.trim(),
                "target": rule.target.trim(),
                "rewriteOrigin": rule.rewrite_origin,
                "insecure": rule.insecure,
                "resolveIp": rule.resolve_ip.trim(),
                "headers": rule.headers.trim()
            })
        })
        .collect();

    let custom_mode = request.splash.mode.trim() == "custom";
    let splash_image = if custom_mode {
        copy_splash_image(&request.splash.image_path, &dir, "splash")?
    } else {
        None
    };
    let splash_bg = if custom_mode {
        None
    } else {
        copy_splash_image(&request.splash.background_image, &dir, "splash-bg")?
    };
    // 没配 logo 时用应用图标
    let logo_source = request
        .splash
        .logo_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            request
                .icon_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        });
    let splash_logo = copy_splash_image(&logo_source, &dir, "logo")?;

    // 自带内核：把固定版本运行时复制到 <dir>/WebView2，宿主启动时用它
    let kernel_rel = "WebView2";
    let kernel_path = request
        .kernel_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(kernel) = kernel_path {
        let source = Path::new(kernel);
        if !source.join("msedgewebview2.exe").is_file() {
            return Err(format!("内核目录缺少 msedgewebview2.exe：{kernel}"));
        }
        let dest = dir.join(kernel_rel);
        if dest.exists() {
            std::fs::remove_dir_all(&dest).ok();
        }
        copy_dir(source, &dest).map_err(|e| format!("复制内核失败: {e}"))?;
        emit_win_log(app, "info", &format!("已内置 WebView2 内核：{kernel}"));
    }
    // 内核版本号（内核库按 <版本> 命名的目录），宿主用来拼默认 UA
    let kernel_version = kernel_path
        .and_then(|path| Path::new(path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| {
            let parts: Vec<&str> = name.split('.').collect();
            parts.len() >= 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
        });

    let config = serde_json::json!({
        "title": request.app_name.trim(),
        "url": request.start_url.trim(),
        "startQuery": request
            .start_query
            .trim()
            .trim_start_matches(|c: char| c == '?' || c == '&'),
        "localIndex": local_index,
        "width": win_width as f64,
        "height": win_height as f64,
        "fullscreen": fullscreen,
        "maximized": request.maximized && request.size_mode == "fixed",
        "resizable": request.resizable,
        "frameless": request.frameless,
        "sizeMode": request.size_mode,
        "aspectW": aspect_w,
        "aspectH": aspect_h,
        "alwaysOnTop": request.always_on_top,
        "devtools": request.debug_enabled,
        "singleInstance": request.single_instance,
        "autoStart": request.auto_start,
        "minimizeOnClose": request.minimize_on_close,
        "closeMode": request.close_mode.trim(),
        "zoom": request.zoom.clamp(0.5, 3.0),
        "autoplay": request.autoplay,
        "blockContextMenu": request.block_context_menu,
        "blockTextSelect": request.block_text_select,
        "hideScrollbars": request.hide_scrollbars,
        "blockCtrlZoom": request.block_ctrl_zoom,
        "blockBrowserKeys": request.block_browser_keys,
        "externalLinksInBrowser": request.external_links_in_browser,
        "minFontSize": request.min_font_size.min(96),
        "ignoreCertErrors": request.ignore_cert_errors,
        "progressBar": request.progress_bar,
        "navBar": request.nav_bar,
        "progressColor": normalize_css_color(&request.progress_color),
        "progressHeight": request.progress_height.clamp(1, 12),
        "uaMode": request.ua_mode.trim(),
        "userAgent": request.user_agent.trim(),
        "downloadToSubfolder": request.download_to_subfolder,
        "minWidth": request.min_width,
        "minHeight": request.min_height,
        "rememberWindow": request.remember_window,
        "localServer": {
            "enabled": request.local_server.enabled,
            "port": request.local_server.port,
            "basePath": crate::config::normalize_base_path(&request.local_server.base_path),
            "proxies": proxies
        },
        "splash": {
            "enabled": request.splash.enabled,
            "title": splash_title(&request.splash, &request.app_name),
            "background": splash_background(&request.splash),
            "backgroundImage": splash_bg,
            "image": splash_image,
            "logo": splash_logo,
            "minShowMs": request.splash.min_show_ms.min(10000)
        },
        "webview2": {
            "fixedDir": if kernel_path.is_some() { Some(kernel_rel) } else { None::<&str> },
            "version": kernel_version
        },
        "permissions": {
            "camera": request.permissions.camera,
            "microphone": request.permissions.microphone,
            "geolocation": request.permissions.geolocation,
            "notifications": request.permissions.notifications,
            "clipboard": request.permissions.clipboard,
            "downloads": request.permissions.downloads,
            "fileReadWrite": request.permissions.file_read_write
        }
    });
    let raw = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("shell.json"), raw).map_err(|e| format!("写入配置失败：{e}"))?;
    push(app, "info", "已写入 shell.json".to_string());

    let mut signed = false;
    let cert = request
        .cert_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(cert_path) = cert {
        let password = request.cert_password.as_deref().unwrap_or("");
        let cert_pathbuf = PathBuf::from(cert_path);
        if !cert_pathbuf.is_file() {
            return Err(format!("签名证书不存在：{cert_path}"));
        }
        push(app, "info", "开始代码签名…".to_string());
        sign_windows_exe(&exe_dst, &cert_pathbuf, password)?;
        signed = true;
        push(app, "info", "代码签名完成".to_string());
    } else {
        push(app, "info", "未配置签名证书，输出未签名程序".to_string());
    }
    // 按勾选产出：安装包 / 绿色版 zip / 单文件 exe
    let stem = format!("{base_name}-{version}");
    let mut single_exe: Option<String> = None;
    let mut installer: Option<String> = None;
    let mut portable_zip: Option<String> = None;

    if request.output_installer {
        let setup_dest = out_root.join(format!("{stem}-setup.exe"));
        let machine_scope = request.install_scope.trim() == "machine";
        if machine_scope {
            push(app, "info", "安装包范围：全机器（需要管理员权限）".to_string());
        }
        match build_installer(
            &dir,
            request.app_name.trim(),
            &base_name.replace('_', ""),
            &version,
            &setup_dest,
            machine_scope,
        ) {
            Ok(size) => {
                push(
                    app,
                    "info",
                    format!(
                        "安装包：{}（{:.1} MB）",
                        setup_dest.display(),
                        size as f64 / 1048576.0
                    ),
                );
                installer = Some(setup_dest.to_string_lossy().to_string());
            }
            Err(e) => push(app, "warn", format!("跳过安装包：{e}")),
        }
    }

    if request.output_portable {
        let zip_dest = out_root.join(format!("{stem}-portable.zip"));
        let zip_size = zip_directory(&dir, &zip_dest, true)
            .map_err(|e| format!("生成绿色版 zip 失败：{e}"))?;
        push(
            app,
            "info",
            format!(
                "绿色版：{}（{:.1} MB）",
                zip_dest.display(),
                zip_size as f64 / 1048576.0
            ),
        );
        portable_zip = Some(zip_dest.to_string_lossy().to_string());
    }

    if request.output_single {
        let single_dest = out_root.join(format!("{stem}-single.exe"));
        let payload_tmp = dir.join("payload.zip");
        let single_result = zip_directory(&dir, &payload_tmp, false)
            .and_then(|_| write_single_exe(&exe_dst, &payload_tmp, &single_dest));
        let _ = std::fs::remove_file(&payload_tmp);
        match single_result {
            Ok(size) => {
                push(
                    app,
                    "info",
                    format!(
                        "单文件版：{}（{:.1} MB）",
                        single_dest.display(),
                        size as f64 / 1048576.0
                    ),
                );
                single_exe = Some(single_dest.to_string_lossy().to_string());
            }
            Err(e) => push(app, "warn", format!("跳过单文件版：{e}")),
        }
    }

    let primary = installer
        .clone()
        .or_else(|| single_exe.clone())
        .or_else(|| portable_zip.clone())
        .unwrap_or_else(|| exe_dst.to_string_lossy().to_string());
    let size_bytes = std::fs::metadata(&primary).map(|m| m.len()).unwrap_or(0);
    let duration_secs = SystemTime::now()
        .duration_since(started)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let log_path = out_root.join(format!("{stem}-build.log"));
    push(app, "info", format!("构建完成（{duration_secs}s）"));
    let _ = std::fs::write(&log_path, log_lines.join("\r\n"));
    let _ = std::fs::remove_dir_all(&dir);

    Ok(WindowsBuildResult {
        build_id: build_id.clone(),
        dir: out_root.to_string_lossy().to_string(),
        exe_path: primary,
        size_bytes,
        signed,
        duration_secs,
        log_path: log_path.to_string_lossy().to_string(),
        single_exe,
        installer,
        portable_zip,
    })
}

#[tauri::command]
pub async fn build_windows(
    app: AppHandle,
    request: WindowsBuildRequest,
) -> Result<WindowsBuildResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_build_windows(&app, request))
        .await
        .map_err(|e| format!("构建线程异常: {e}"))?
}
