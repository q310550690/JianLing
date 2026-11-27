use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// 单项权限的说明配置（申请系统权限时在页面顶部展示用途）。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PermissionPrompt {
    pub enabled: bool,
    pub title: String,
    pub message: String,
}

impl Default for PermissionPrompt {
    fn default() -> Self {
        Self {
            enabled: true,
            title: "权限申请".to_string(),
            message: String::new(),
        }
    }
}

pub fn default_permission_prompts() -> HashMap<String, PermissionPrompt> {
    let items = [
        ("camera", "相机权限", "需要使用相机拍摄照片或录制视频，是否允许？"),
        ("microphone", "麦克风权限", "需要使用麦克风录制语音，是否允许？"),
        ("storage", "存储权限", "需要读取设备上的照片与文件，是否允许？"),
        ("location", "定位权限", "需要获取你的位置信息以提供附近内容，是否允许？"),
        ("notification", "通知权限", "需要发送消息提醒，是否允许？"),
        ("bluetooth", "蓝牙权限", "需要连接蓝牙设备（耳机、手环等），是否允许？"),
        ("nfc", "NFC 权限", "需要读取 NFC 标签，是否允许？"),
        ("vibrate", "振动权限", "需要使用振动反馈，是否允许？"),
        (
            "foreground",
            "前台服务",
            "需要在后台保持通话或录音运行，是否允许？",
        ),
        ("call", "拨号权限", "需要直接拨打电话，是否允许？"),
        ("install", "安装应用", "需要下载并安装应用更新，是否允许？"),
    ];
    let mut map = HashMap::new();
    for (key, title, message) in items {
        // 默认申请权限但不显示用途说明：需要时在权限页单独打开
        let prompt_on = !matches!(key, "camera" | "microphone" | "storage" | "location");
        map.insert(
            key.to_string(),
            PermissionPrompt {
                enabled: prompt_on,
                title: title.to_string(),
                message: message.to_string(),
            },
        );
    }
    map
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct AndroidProfile {
    pub app_name: String,
    pub package_name: String,
    pub version_name: String,
    pub version_code: u32,
    /// remote | local
    pub source_kind: String,
    pub start_url: String,
    /// 附加到启动地址的查询参数（可选），如 key=value
    pub start_query: String,
    pub local_dir: Option<String>,
    pub fullscreen: bool,
    pub status_bar_color: String,
    /// 状态栏透明、内容从屏幕顶端开始（边到边）；关闭时用 status_bar_color 绘制状态栏
    pub edge_to_edge: bool,
    /// 注入给页面的状态栏高度 CSS 变量名（默认 --status-bar-height，与 uni-app 同名）
    pub status_bar_css_var: String,
    /// 只隐藏导航栏（保留状态栏），底部上滑临时呼出；全屏启动时无意义
    pub hide_navigation_bar: bool,
    /// auto | landscape | portrait
    pub screen_orientation: String,
    pub ignore_ssl_errors: bool,
    pub allow_cleartext: bool,
    pub permissions: Vec<String>,
    pub icon_path: Option<String>,
    pub icon_background_color: String,
    pub keystore_path: Option<String>,
    pub keystore_alias: Option<String>,
    #[serde(with = "crate::secret::optional")]
    pub keystore_store_password: Option<String>,
    #[serde(with = "crate::secret::optional")]
    pub keystore_key_password: Option<String>,
    // WebView 行为
    pub text_zoom: u32,
    pub allow_autoplay: bool,
    pub allow_zoom: bool,
    pub disable_long_press: bool,
    pub hide_scrollbars: bool,
    pub allow_third_party_cookies: bool,
    pub video_fullscreen: bool,
    pub debug_enabled: bool,
    /// 调试包注入的页内调试面板：vconsole | eruda | none
    pub debug_console: String,
    /// http/https 链接改用系统浏览器打开
    pub open_external_in_browser: bool,
    /// default | no_cache | cache_first
    pub cache_mode: String,
    /// append | override
    pub ua_mode: String,
    pub user_agent: String,
    pub pull_to_refresh: bool,
    /// 返回键在无法后退时「再按一次退出」
    pub exit_on_back_pressed: bool,
    /// 禁止返回键退出应用（网页退不了时返回键无效）
    pub disable_back_exit: bool,
    /// 返回键策略：auto（原生历史优先，退不了再让网页后退一次）|
    /// web（只走网页 history.back()）| exit（不后退，直接按退出处理）|
    /// none（返回键被吃掉，无任何反应）
    pub back_mode: String,
    /// 顶部加载进度条
    pub progress_bar: bool,
    pub progress_color: String,
    /// 进度条高度（dp）
    pub progress_height: u32,
    /// 顶部浏览器操作栏（前进 / 后退 / 刷新）
    pub nav_bar: bool,
    pub min_sdk: u32,
    pub target_sdk: u32,
    pub compile_sdk: u32,
    /// 状态栏使用深色图标（浅色背景时开启）
    pub light_status_bar: bool,
    pub keep_screen_on: bool,
    /// 禁止截屏与录屏
    pub secure_flag: bool,
    /// 本地静态包的内置服务（静态托管 + 接口转发）
    pub local_server: LocalServerConfig,
    /// 启动屏
    pub splash: SplashConfig,
    // 每项权限各自的说明弹窗
    pub permission_prompts: HashMap<String, PermissionPrompt>,
}

impl Default for AndroidProfile {
    fn default() -> Self {
        Self {
            app_name: String::new(),
            package_name: "com.jianling.app".to_string(),
            version_name: "0.0.1".to_string(),
            version_code: 1,
            source_kind: "remote".to_string(),
            start_url: String::new(),
            start_query: String::new(),
            local_dir: None,
            fullscreen: false,
            status_bar_color: "#1F2430".to_string(),
            edge_to_edge: true,
            status_bar_css_var: "--status-bar-height".to_string(),
            hide_navigation_bar: false,
            screen_orientation: "auto".to_string(),
            ignore_ssl_errors: false,
            allow_cleartext: false,
            permissions: vec!["camera".into(), "microphone".into(), "storage".into(), "location".into()],
            icon_path: None,
            icon_background_color: "#ffffff".to_string(),
            keystore_path: None,
            keystore_alias: None,
            keystore_store_password: None,
            keystore_key_password: None,
            text_zoom: 100,
            allow_autoplay: true,
            allow_zoom: false,
            disable_long_press: false,
            hide_scrollbars: true,
            allow_third_party_cookies: true,
            video_fullscreen: false,
            debug_enabled: false,
            debug_console: "eruda".to_string(),
            open_external_in_browser: false,
            cache_mode: "default".to_string(),
            ua_mode: "append".to_string(),
            user_agent: String::new(),
            pull_to_refresh: false,
            exit_on_back_pressed: true,
            disable_back_exit: false,
            back_mode: "auto".to_string(),
            progress_bar: false,
            progress_color: "#2E7D32".to_string(),
            progress_height: 1,
            nav_bar: false,
            min_sdk: 24,
            target_sdk: 34,
            compile_sdk: 34,
            light_status_bar: false,
            keep_screen_on: false,
            secure_flag: false,
            local_server: LocalServerConfig::default(),
            splash: SplashConfig::default(),
            permission_prompts: default_permission_prompts(),
        }
    }
}

/// Windows 端每项权限的默认策略：ask（内核提示）/ allow（自动允许）/ deny（自动拒绝）
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowsPermissions {
    pub camera: String,
    pub microphone: String,
    pub geolocation: String,
    pub notifications: String,
    pub clipboard: String,
    pub downloads: String,
    pub file_read_write: String,
}

impl Default for WindowsPermissions {
    fn default() -> Self {
        Self {
            camera: "ask".to_string(),
            microphone: "ask".to_string(),
            geolocation: "ask".to_string(),
            notifications: "ask".to_string(),
            clipboard: "ask".to_string(),
            downloads: "ask".to_string(),
            file_read_write: "ask".to_string(),
        }
    }
}

/// 本地静态包的内置服务：静态托管 + 接口转发（等价 vite proxy）。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyRule {
    /// 路径前缀，例如 /api
    pub prefix: String,
    /// 上游地址，例如 https://api.example.com 或 wss://api.example.com
    pub target: String,
    /// 把 Origin 改写成上游自身的 Origin（部分后端的 nginx 会校验）
    pub rewrite_origin: bool,
    /// 上游是自签证书时跳过校验
    pub insecure: bool,
    /// 可选：把上游域名固定解析到该 IP（域名只能在 hosts 里解析、设备上解析不到时用）
    pub resolve_ip: String,
    /// 可选：追加到上游请求的 header，JSON 对象文本，例如 {"X-Token":"abc"}
    pub headers: String,
}

impl Default for ProxyRule {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            target: String::new(),
            rewrite_origin: true,
            insecure: false,
            resolve_ip: String::new(),
            headers: String::new(),
        }
    }
}

/// 本地静态包的内置服务配置。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalServerConfig {
    pub enabled: bool,
    /// 0 表示按包名派生固定端口（端口变化会让页面 localStorage / cookie 变成另一个 origin）
    pub port: u16,
    /// 部署目录，形如 "/" 或 "/h5/"；静态文件按此前缀映射
    pub base_path: String,
    /// 调试流量日志：只在调试包里生效，正式包一律不开启
    pub debug_log: bool,
    pub proxies: Vec<ProxyRule>,
}

impl Default for LocalServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 0,
            base_path: "/".to_string(),
            debug_log: false,
            proxies: vec![ProxyRule {
                prefix: "/api".to_string(),
                target: String::new(),
                rewrite_origin: true,
                insecure: false,
                resolve_ip: String::new(),
                headers: String::new(),
            }],
        }
    }
}

/// 统一成「以 / 开头并以 / 结尾」的部署目录，空值即根目录 /。
pub fn normalize_base_path(raw: &str) -> String {
    let text = raw.trim().trim_matches('/');
    if text.is_empty() {
        "/".to_string()
    } else {
        format!("/{text}/")
    }
}

/// 启动屏：先展示静态 HTML（logo + 名称或整屏图片），服务就绪后再切到网页。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct SplashConfig {
    pub enabled: bool,
    /// default（背景色/背景图 + 名称 + logo）| custom（整屏图片）
    pub mode: String,
    /// 显示的名称，留空时用应用名
    pub title: String,
    pub background_color: String,
    /// 默认模式的背景图（与背景色二选一，图片优先）
    pub background_image: Option<String>,
    /// 整屏图片源路径（构建时复制到输出目录，留空则不使用）
    pub image_path: Option<String>,
    /// logo 图片源路径（留空则用应用图标）
    pub logo_path: Option<String>,
    /// 最短展示时长（毫秒）
    pub min_show_ms: u32,
}

impl Default for SplashConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "default".to_string(),
            title: String::new(),
            background_color: "#FFFFFF".to_string(),
            background_image: None,
            image_path: None,
            logo_path: None,
            min_show_ms: 600,
        }
    }
}

/// Windows 桌面套壳的独立配置。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowsProfile {
    /// remote | local
    pub source_kind: String,
    pub start_url: String,
    /// 附加到启动地址的查询参数（可选），如 key=value
    pub start_query: String,
    pub local_dir: Option<String>,
    /// fixed（指定分辨率）| ratio（固定比例）| fullscreen（全屏）
    pub size_mode: String,
    pub width: u32,
    pub height: u32,
    /// 固定比例模式下的宽高比，例如 16 / 9
    pub aspect_w: u32,
    pub aspect_h: u32,
    pub resizable: bool,
    pub frameless: bool,
    pub always_on_top: bool,
    pub maximized: bool,
    pub debug_enabled: bool,
    // 壳行为
    pub single_instance: bool,
    pub auto_start: bool,
    pub minimize_on_close: bool,
    /// 关闭按钮行为：exit（默认）| minimize（收进任务栏）| tray（收进托盘）
    pub close_mode: String,
    // 浏览器行为
    pub zoom: f64,
    pub autoplay: bool,
    pub block_context_menu: bool,
    pub block_text_select: bool,
    pub hide_scrollbars: bool,
    pub block_ctrl_zoom: bool,
    pub block_browser_keys: bool,
    pub external_links_in_browser: bool,
    /// 最小字号（px），0 表示不限制
    pub min_font_size: u32,
    /// 忽略 https 证书错误（内网自签证书常用）
    pub ignore_cert_errors: bool,
    /// 页面加载时在顶部显示进度条
    pub progress_bar: bool,
    /// 顶部操作栏（后退 / 前进 / 刷新）
    pub nav_bar: bool,
    pub progress_color: String,
    /// 进度条高度（px）
    pub progress_height: u32,
    /// User-Agent 策略：keep（默认）| append（追加后缀）| override（整串替换）
    pub ua_mode: String,
    /// 追加或替换用的 UA 字符串
    pub user_agent: String,
    /// 下载文件统一存到「下载\<应用名>」子目录
    pub download_to_subfolder: bool,
    /// 最小窗口尺寸（0 表示不限制）
    pub min_width: u32,
    pub min_height: u32,
    /// 记住窗口位置与大小
    pub remember_window: bool,
    /// 本地静态包的内置服务（静态托管 + 接口转发）
    pub local_server: LocalServerConfig,
    /// 启动屏
    pub splash: SplashConfig,
    // 权限策略
    pub permissions: WindowsPermissions,
    // 产物形态（默认只出安装包）
    pub output_single: bool,
    pub output_installer: bool,
    pub output_portable: bool,
    /// 安装包安装范围：user（当前用户）| machine（全机器，需管理员）
    pub install_scope: String,
    /// Authenticode 代码签名证书（.pfx / .p12），留空则导出未签名可执行文件
    pub cert_path: Option<String>,
    #[serde(with = "crate::secret::optional")]
    pub cert_password: Option<String>,
    /// 打包时自带的 Windows 内核对（设置页维护的列表里的 id），None = 跟随系统
    pub kernel_id: Option<String>,
}

impl Default for WindowsProfile {
    fn default() -> Self {
        Self {
            source_kind: "remote".to_string(),
            start_url: String::new(),
            start_query: String::new(),
            local_dir: None,
            size_mode: "fixed".to_string(),
            width: 1200,
            height: 820,
            aspect_w: 16,
            aspect_h: 9,
            resizable: true,
            frameless: false,
            always_on_top: false,
            maximized: false,
            debug_enabled: false,
            single_instance: true,
            auto_start: false,
            minimize_on_close: false,
            close_mode: "exit".to_string(),
            zoom: 1.0,
            autoplay: true,
            block_context_menu: true,
            block_text_select: false,
            hide_scrollbars: true,
            block_ctrl_zoom: false,
            block_browser_keys: false,
            external_links_in_browser: false,
            min_font_size: 0,
            ignore_cert_errors: true,
            progress_bar: false,
            nav_bar: false,
            progress_color: "#2E7D32".to_string(),
            progress_height: 1,
            ua_mode: "append".to_string(),
            user_agent: String::new(),
            download_to_subfolder: true,
            min_width: 0,
            min_height: 0,
            remember_window: false,
            local_server: LocalServerConfig::default(),
            splash: SplashConfig::default(),
            permissions: WindowsPermissions::default(),
            output_single: false,
            output_installer: true,
            output_portable: false,
            install_scope: "user".to_string(),
            cert_path: None,
            cert_password: None,
            kernel_id: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BuildRecord {
    pub id: String,
    pub apk_path: String,
    pub size_bytes: u64,
    pub signed: bool,
    pub version_name: String,
    pub version_code: u32,
    pub duration_secs: u64,
    pub created_at: u64,
    pub log_path: String,
    /// android | windows（旧记录为空，按 android 处理）
    pub platform: String,
    /// 构建是否失败（旧记录没有该字段，按成功处理）
    pub failed: bool,
    /// 失败原因（失败记录用来在历史里直接看）
    pub error: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// 产物输出目录（保存时可为空，构建前必须填）
    pub output_dir: Option<String>,
    /// 打包目标，为后续 Windows exe 预留
    pub target: String,
    pub android: AndroidProfile,
    pub windows: WindowsProfile,
    /// macOS 专属字段；窗口、本地服务等其余配置复用 windows
    pub macos: MacosProfile,
    pub created_at: u64,
    pub updated_at: u64,
    pub builds: Vec<BuildRecord>,
}

/// macOS 桌面套壳的专属配置。
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MacosProfile {
    /// CFBundleIdentifier，留空时按包名或应用名生成
    pub bundle_id: String,
    /// codesign 签名身份，留空输出未签名 .app
    pub sign_identity: String,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "未命名项目".to_string(),
            output_dir: None,
            target: "android".to_string(),
            android: AndroidProfile::default(),
            windows: WindowsProfile::default(),
            macos: MacosProfile::default(),
            created_at: 0,
            updated_at: 0,
            builds: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub jdk_path: Option<String>,
    pub sdk_path: Option<String>,
    /// adb.exe 路径（留空则自动探测 Android SDK / PATH）
    pub adb_path: Option<String>,
    pub projects: Vec<Project>,
    /// 已下载/导入的 Windows 内核（WebView2 固定版本运行时）
    pub windows_kernels: Vec<WindowsKernel>,
}

/// Windows 内核：一份 WebView2 固定版本运行时（目录里含 msedgewebview2.exe）
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowsKernel {
    pub id: String,
    pub version: String,
    pub path: String,
    /// auto（自动下载）| local（本地导入）
    pub source: String,
    pub added_at: u64,
    pub size_bytes: u64,
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<Settings, String> {
    let file = settings_file(&app)?;
    if !file.is_file() {
        return Ok(Settings::default());
    }
    let raw = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("配置文件解析失败: {e}"))
}

/// 读取构建日志文本，供历史记录里的「查看日志」在应用内弹窗展示。
/// 仅接受 `.log` 文件并限制大小，避免变成任意文件读取接口。
#[tauri::command]
pub fn read_build_log(path: String) -> Result<String, String> {
    if !path.to_lowercase().ends_with(".log") {
        return Err("只能查看 .log 日志文件".to_string());
    }
    let file = PathBuf::from(&path);
    let meta = std::fs::metadata(&file).map_err(|e| format!("日志文件不存在：{e}"))?;
    if meta.len() > 32 * 1024 * 1024 {
        return Err("日志文件超过 32MB，无法在弹窗中展示".to_string());
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("读取日志失败：{e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Windows 壳运行时把调试流量日志写在系统临时目录，这里返回目录（必要时创建），
/// 供「打开日志目录」按钮直接打开资源管理器。
#[tauri::command]
pub fn traffic_log_dir() -> Result<String, String> {
    let dir = std::env::temp_dir().join("jianling-logs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败：{e}"))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 删除构建记录关联的产物文件。只删白名单扩展名的普通文件，
/// 不存在的路径按已删除处理，避免用户手动清理过之后仍报错。
#[tauri::command]
pub fn delete_build_files(paths: Vec<String>) -> Result<Vec<String>, String> {
    let mut removed = Vec::new();
    for raw in paths {
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }
        let path = PathBuf::from(text);
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "apk" | "exe" | "zip" | "log") {
            return Err(format!("拒绝删除非构建产物：{text}"));
        }
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if !meta.is_file() {
            return Err(format!("拒绝删除非文件：{text}"));
        }
        std::fs::remove_file(&path).map_err(|e| format!("删除失败 {text}：{e}"))?;
        removed.push(text.to_string());
    }
    Ok(removed)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let file = settings_file(&app)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut settings = settings;
    encrypt_settings_secrets(&mut settings)?;
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&file, raw).map_err(|e| e.to_string())
}

/// 落盘前把密码类字段加密（DPAPI）。内存 / IPC 里始终是明文，
/// 只有 settings.json 里存密文，读取时由 serde 的反序列化负责解密。
fn encrypt_settings_secrets(settings: &mut Settings) -> Result<(), String> {
    for project in settings.projects.iter_mut() {
        let android = &mut project.android;
        android.keystore_store_password =
            encrypt_secret(android.keystore_store_password.take())?;
        android.keystore_key_password = encrypt_secret(android.keystore_key_password.take())?;
        project.windows.cert_password = encrypt_secret(project.windows.cert_password.take())?;
    }
    Ok(())
}

fn encrypt_secret(value: Option<String>) -> Result<Option<String>, String> {
    match value.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // 已经是密文就别再套一层（兼容旧版本存下来的数据）
        Some(plain) if crate::secret::looks_encrypted(plain) => Ok(Some(plain.to_string())),
        Some(plain) => crate::secret::protect(plain).map(Some),
        None => Ok(None),
    }
}
