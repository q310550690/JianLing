use serde::Serialize;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Error,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnvItem {
    pub id: String,
    pub label: String,
    pub status: Status,
    pub value: String,
    pub path: Option<String>,
    pub hint: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnvReport {
    pub platform: String,
    pub major: Option<u32>,
    pub items: Vec<EnvItem>,
    pub sdk_platforms: Vec<String>,
    pub sdk_build_tools: Vec<String>,
}

impl EnvItem {
    fn new(
        id: &str,
        label: &str,
        status: Status,
        value: impl Into<String>,
        path: Option<PathBuf>,
        hint: Option<&str>,
    ) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status,
            value: value.into(),
            path: path.map(|p| p.to_string_lossy().to_string()),
            hint: hint.map(|s| s.to_string()),
        }
    }
}

fn hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn push_unique(list: &mut Vec<PathBuf>, candidate: PathBuf) {
    let normalized = candidate.to_string_lossy().to_lowercase();
    if normalized.trim().is_empty() {
        return;
    }
    if list
        .iter()
        .any(|p| p.to_string_lossy().to_lowercase() == normalized)
    {
        return;
    }
    list.push(candidate);
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn parse_java_major(text: &str) -> Option<u32> {
    for line in text.lines() {
        let Some(start) = line.find('"') else { continue };
        let rest = &line[start + 1..];
        let Some(end) = rest.find('"') else { continue };
        let version = &rest[..end];
        let mut parts = version.split('.');
        let Some(first) = parts.next() else { continue };
        if first == "1" {
            return parts.next().and_then(|s| s.parse().ok());
        }
        if let Ok(major) = first.parse() {
            return Some(major);
        }
    }
    None
}

fn java_binary(jdk_home: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    let candidate = jdk_home.join("bin").join(name);
    if candidate.is_file() {
        return Some(candidate);
    }
    let direct = jdk_home.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    None
}

fn jdk_candidates(override_path: Option<&str>) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();

    if let Some(p) = override_path.map(str::trim).filter(|p| !p.is_empty()) {
        push_unique(&mut list, PathBuf::from(p));
    }
    if let Some(home) = env_path("JAVA_HOME") {
        push_unique(&mut list, home);
    }
    if let Ok(raw) = std::env::var("PATH") {
        for dir in std::env::split_paths(&raw) {
            let exe = dir.join(if cfg!(windows) { "java.exe" } else { "java" });
            if exe.is_file() {
                if let Some(home) = dir.parent() {
                    push_unique(&mut list, home.to_path_buf());
                }
            }
        }
    }
    list
}

fn detect_jdk(override_path: Option<&str>) -> EnvItem {
    for home in jdk_candidates(override_path) {
        let Some(java) = java_binary(&home) else {
            continue;
        };
        let Ok(output) = hidden(&java).arg("-version").output() else {
            continue;
        };
        let raw = if output.stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            String::from_utf8_lossy(&output.stderr).to_string()
        };
        let Some(major) = parse_java_major(&raw) else {
            continue;
        };
        let (status, hint) = if major >= 17 {
            (Status::Ok, None)
        } else {
            (
                Status::Warn,
                Some("Android Gradle Plugin 8.x 需要 JDK 17 及以上"),
            )
        };
        let first_line = raw.lines().next().unwrap_or_default().trim().to_string();
        return EnvItem::new(
            "jdk",
            "JDK",
            status,
            first_line,
            Some(home),
            hint,
        );
    }
    EnvItem::new(
        "jdk",
        "JDK",
        Status::Error,
        "未找到可用 JDK",
        None,
        Some("设置 JAVA_HOME，或在下方手动指定 JDK 目录"),
    )
}

fn sdk_candidates(override_path: Option<&str>) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();
    if let Some(p) = override_path.map(str::trim).filter(|p| !p.is_empty()) {
        push_unique(&mut list, PathBuf::from(p));
    }
    if let Some(p) = env_path("ANDROID_HOME") {
        push_unique(&mut list, p);
    }
    if let Some(p) = env_path("ANDROID_SDK_ROOT") {
        push_unique(&mut list, p);
    }
    if let Some(local) = env_path("LOCALAPPDATA") {
        push_unique(&mut list, local.join("Android").join("Sdk"));
    }
    if let Some(profile) = env_path("USERPROFILE") {
        push_unique(&mut list, profile.join("Android").join("Sdk"));
    }
    list
}

fn list_versions(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    out
}

fn version_key(name: &str) -> Vec<u32> {
    name.split(['-', '.'])
        .map(|s| s.parse::<u32>().unwrap_or(0))
        .collect()
}

fn count_license_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count())
        .unwrap_or(0)
}

fn detect_sdk(override_path: Option<&str>) -> (EnvItem, Vec<String>, Vec<String>) {
    for root in sdk_candidates(override_path) {
        if !root.is_dir() {
            continue;
        }
        let platforms = list_versions(&root.join("platforms"));
        let build_tools = list_versions(&root.join("build-tools"));
        if platforms.is_empty() && build_tools.is_empty() {
            continue;
        }

        let mut missing: Vec<&str> = Vec::new();
        if platforms.is_empty() {
            missing.push("platforms");
        }
        if build_tools.is_empty() {
            missing.push("build-tools");
        }
        if !root.join("platform-tools").is_dir() {
            missing.push("platform-tools");
        }
        if list_versions(&root.join("cmdline-tools")).is_empty() {
            missing.push("cmdline-tools");
        }

        let value = if missing.is_empty() {
            format!(
                "{} 个 platform / {} 个 build-tools",
                platforms.len(),
                build_tools.len()
            )
        } else {
            format!("缺少 {}", missing.join("、"))
        };
        let (status, hint) = if missing.is_empty() {
            (Status::Ok, None)
        } else {
            (Status::Warn, Some("可运行 sdkmanager 补齐缺失组件"))
        };

        let item = EnvItem::new("sdk", "Android SDK", status, value, Some(root), hint);
        return (item, platforms, build_tools);
    }

    (
        EnvItem::new(
            "sdk",
            "Android SDK",
            Status::Error,
            "未找到 Android SDK",
            None,
            Some("设置 ANDROID_HOME，或手动指定 SDK 目录"),
        ),
        Vec::new(),
        Vec::new(),
    )
}

fn detect_licenses(sdk_root: Option<&Path>) -> EnvItem {
    let Some(root) = sdk_root else {
        return EnvItem::new(
            "licenses",
            "构建授权",
            Status::Warn,
            "未知（SDK 未找到）",
            None,
            None,
        );
    };
    let dir = root.join("licenses");
    let count = count_license_files(&dir);
    if count > 0 {
        EnvItem::new(
            "licenses",
            "构建授权",
            Status::Ok,
            format!("已接受 {} 项", count),
            Some(dir),
            None,
        )
    } else {
        EnvItem::new(
            "licenses",
            "构建授权",
            Status::Warn,
            "未接受 SDK 许可协议",
            Some(dir),
            Some("构建会失败，需运行 sdkmanager --licenses 接受协议"),
        )
    }
}

fn detect_gradle() -> EnvItem {
    let base = env_path("GRADLE_USER_HOME").or_else(|| env_path("USERPROFILE").map(|p| p.join(".gradle")));

    let Some(base) = base else {
        return EnvItem::new(
            "gradle",
            "Gradle",
            Status::Warn,
            "未找到 GRADLE_USER_HOME",
            None,
            Some("首次构建会自动下载 Gradle 发行包"),
        );
    };

    let dists = base.join("wrapper").join("dists");
    let mut found: Vec<String> = std::fs::read_dir(&dists)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with("gradle-"))
                .collect()
        })
        .unwrap_or_default();
    found.sort();

    if found.is_empty() {
        EnvItem::new(
            "gradle",
            "Gradle",
            Status::Warn,
            "无已缓存的发行包",
            Some(dists),
            Some("首次构建需要联网下载，或由本工具下载到缓存目录"),
        )
    } else {
        EnvItem::new(
            "gradle",
            "Gradle",
            Status::Ok,
            found.join(" / "),
            Some(dists),
            None,
        )
    }
}

fn probe(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    for addr in addrs.take(3) {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(1500)).is_ok() {
            return true;
        }
    }
    false
}

fn detect_network() -> EnvItem {
    let targets = [
        ("maven.aliyun.com", 443),
        ("dl.google.com", 443),
        ("services.gradle.org", 443),
    ];
    let results: Vec<(&str, bool)> = targets
        .iter()
        .map(|(host, port)| (*host, probe(host, *port)))
        .collect();

    let reachable: Vec<&str> = results
        .iter()
        .filter(|(_, ok)| *ok)
        .map(|(host, _)| *host)
        .collect();

    if reachable.is_empty() {
        EnvItem::new(
            "network",
            "网络",
            Status::Error,
            "依赖仓库均不可达",
            None,
            Some("检查网络、代理或防火墙设置"),
        )
    } else {
        let value = format!("可达 {} / {}", reachable.len(), targets.len());
        let status = if reachable.len() == targets.len() {
            Status::Ok
        } else {
            Status::Warn
        };
        EnvItem::new("network", "网络", status, value, None, None)
    }
}

fn detect_disk() -> EnvItem {
    let target = env_path("USERPROFILE").unwrap_or_else(|| PathBuf::from("."));

    match fs2::available_space(&target) {
        Ok(bytes) => {
            let gb = bytes as f64 / 1024.0 / 1024.0 / 1024.0;
            let (status, hint) = if gb >= 3.0 {
                (Status::Ok, None)
            } else {
                (Status::Warn, Some("建议预留 3 GB 以上用于构建缓存与产物"))
            };
            EnvItem::new(
                "disk",
                "磁盘空间",
                status,
                format!("{:.1} GB 可用", gb),
                Some(target),
                hint,
            )
        }
        Err(_) => EnvItem::new(
            "disk",
            "磁盘空间",
            Status::Warn,
            "无法读取",
            Some(target),
            None,
        ),
    }
}

pub fn check(jdk_path: Option<String>, sdk_path: Option<String>) -> EnvReport {
    let jdk = detect_jdk(jdk_path.as_deref());
    let jdk_major = parse_java_major(&jdk.value);
    let jdk_home = jdk.path.clone().map(PathBuf::from);

    let (sdk, platforms, build_tools) = detect_sdk(sdk_path.as_deref());
    let sdk_root = sdk.path.clone().map(PathBuf::from);

    let licenses = detect_licenses(sdk_root.as_deref());
    let gradle = detect_gradle();
    let network = detect_network();
    let disk = detect_disk();

    let items = vec![jdk, sdk, licenses, gradle, network, disk];

    let platform = match jdk_home {
        Some(_) => std::env::consts::OS.to_string(),
        None => std::env::consts::OS.to_string(),
    };

    EnvReport {
        platform,
        major: jdk_major,
        items,
        sdk_platforms: platforms,
        sdk_build_tools: build_tools,
    }
}
