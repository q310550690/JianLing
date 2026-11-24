//! Jianling 生成的 Windows 桌面套壳宿主。
//!
//! 运行时读取同目录下的 `shell.json`（缺省字段都有默认值）：
//! ```json
//! {
//!   "title": "应用名",
//!   "url": "https://example.com/",
//!   "localIndex": null,
//!   "width": 1200, "height": 820,
//!   "fullscreen": false, "maximized": false,
//!   "resizable": true, "frameless": false, "alwaysOnTop": false,
//!   "devtools": false,
//!   "singleInstance": true, "autoStart": false, "minimizeOnClose": false,
//!   "zoom": 1.0,
//!   "blockContextMenu": false, "blockTextSelect": false,
//!   "hideScrollbars": true, "blockCtrlZoom": false,
//!   "blockBrowserKeys": false, "externalLinksInBrowser": true,
//!   "minFontSize": 0,
//!   "autoplay": true,
//!   "permissions": { "camera": "ask", "microphone": "ask", "geolocation": "ask",
//!                    "notifications": "ask", "clipboard": "ask",
//!                    "downloads": "ask", "fileReadWrite": "ask" },
//!   "localServer": { "enabled": true, "port": 0, "proxies": [
//!                    { "prefix": "/api", "target": "https://api.example.com",
//!                      "rewriteOrigin": true, "insecure": false } ] }
//! }
//! ```
//! `localIndex` 非空时加载该本地 HTML 文件（相对 exe 目录），否则加载 `url`。
//! 开启 `localServer` 时本地页面改由内置的 127.0.0.1 服务托管，相对路径与转发规则同源。

mod server;
#[cfg(windows)]
mod tray;

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PolicyState {
    Ask,
    Allow,
    Deny,
}

impl Default for PolicyState {
    fn default() -> Self {
        PolicyState::Ask
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PermissionPolicy {
    camera: PolicyState,
    microphone: PolicyState,
    geolocation: PolicyState,
    notifications: PolicyState,
    clipboard: PolicyState,
    downloads: PolicyState,
    file_read_write: PolicyState,
}

/// 启动屏：先展示静态 HTML，服务就绪后再切到真正的页面。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SplashCfg {
    enabled: bool,
    title: String,
    background: String,
    /// 默认模式的背景图（与背景色二选一，图片优先）
    background_image: Option<String>,
    /// 自定义模式的整屏图片
    image: Option<String>,
    logo: Option<String>,
    min_show_ms: u32,
}

impl Default for SplashCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            title: String::new(),
            background: "#FFFFFF".to_string(),
            background_image: None,
            image: None,
            logo: None,
            min_show_ms: 600,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShellConfig {
    title: String,
    url: String,
    #[serde(default)]
    local_index: Option<String>,
    #[serde(default = "default_width")]
    width: f64,
    #[serde(default = "default_height")]
    height: f64,
    #[serde(default = "default_true")]
    resizable: bool,
    #[serde(default)]
    frameless: bool,
    #[serde(default)]
    always_on_top: bool,
    #[serde(default)]
    devtools: bool,
    #[serde(default)]
    fullscreen: bool,
    #[serde(default)]
    maximized: bool,
    /// fixed | ratio | fullscreen
    #[serde(default)]
    size_mode: String,
    #[serde(default = "default_aspect_w")]
    aspect_w: f64,
    #[serde(default = "default_aspect_h")]
    aspect_h: f64,
    #[serde(default = "default_true")]
    single_instance: bool,
    #[serde(default)]
    auto_start: bool,
    #[serde(default)]
    minimize_on_close: bool,
    /// 关闭按钮行为：exit | minimize | tray（空 = 按 minimizeOnClose 推断）
    #[serde(default)]
    close_mode: String,
    #[serde(default = "default_zoom")]
    zoom: f64,
    #[serde(default)]
    block_context_menu: bool,
    #[serde(default)]
    block_text_select: bool,
    #[serde(default)]
    hide_scrollbars: bool,
    #[serde(default)]
    block_ctrl_zoom: bool,
    #[serde(default)]
    block_browser_keys: bool,
    #[serde(default = "default_true")]
    external_links_in_browser: bool,
    /// 最小字号（px），0 表示不限制。
    #[serde(default)]
    min_font_size: u32,
    /// 忽略 https 证书错误（内网自签证书常用）
    #[serde(default = "default_true")]
    ignore_cert_errors: bool,
    /// 页面加载时在顶部显示进度条
    #[serde(default = "default_true")]
    progress_bar: bool,
    /// 顶部操作栏（后退 / 前进 / 刷新）
    #[serde(default)]
    nav_bar: bool,
    #[serde(default = "default_progress_color")]
    progress_color: String,
    #[serde(default = "default_progress_height")]
    progress_height: u32,
    /// UA 策略：keep 保持默认 | append 追加后缀 | override 整串替换
    #[serde(default)]
    ua_mode: String,
    #[serde(default)]
    user_agent: String,
    /// 下载文件统一存到「下载\<应用名>」
    #[serde(default = "default_true")]
    download_to_subfolder: bool,
    /// 最小窗口尺寸（0 表示不限制）
    #[serde(default)]
    min_width: u32,
    #[serde(default)]
    min_height: u32,
    /// 记住窗口位置与大小
    #[serde(default)]
    remember_window: bool,
    #[serde(default = "default_true")]
    autoplay: bool,
    #[serde(default)]
    permissions: PermissionPolicy,
    #[serde(default)]
    local_server: server::Config,
    #[serde(default)]
    splash: SplashCfg,
    #[serde(default)]
    webview2: Option<Webview2Cfg>,
}

/// 自带 WebView2 固定版本运行时：目录相对内容根目录。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Webview2Cfg {
    fixed_dir: Option<String>,
    /// 运行时版本号，用于「UA 追加」拼默认 UA
    version: Option<String>,
}

fn default_width() -> f64 {
    1200.0
}

fn default_height() -> f64 {
    820.0
}

fn default_true() -> bool {
    true
}

fn default_progress_color() -> String {
    "#2E7D32".to_string()
}

fn default_progress_height() -> u32 {
    2
}

fn default_zoom() -> f64 {
    1.0
}

fn default_aspect_w() -> f64 {
    16.0
}

fn default_aspect_h() -> f64 {
    9.0
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            title: "Jianling".into(),
            url: "about:blank".into(),
            local_index: None,
            width: default_width(),
            height: default_height(),
            resizable: true,
            frameless: false,
            always_on_top: false,
            devtools: false,
            fullscreen: false,
            maximized: false,
            size_mode: String::new(),
            aspect_w: default_aspect_w(),
            aspect_h: default_aspect_h(),
            single_instance: true,
            auto_start: false,
            minimize_on_close: false,
            close_mode: String::new(),
            zoom: default_zoom(),
            block_context_menu: false,
            block_text_select: false,
            hide_scrollbars: false,
            block_ctrl_zoom: false,
            block_browser_keys: false,
            external_links_in_browser: true,
            min_font_size: 0,
            ignore_cert_errors: true,
            progress_bar: true,
            nav_bar: false,
            progress_color: default_progress_color(),
            progress_height: default_progress_height(),
            ua_mode: String::new(),
            user_agent: String::new(),
            download_to_subfolder: true,
            min_width: 0,
            min_height: 0,
            remember_window: false,
            autoplay: true,
            permissions: PermissionPolicy::default(),
            local_server: server::Config::default(),
            splash: SplashCfg::default(),
            webview2: None,
        }
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn load_config(dir: &std::path::Path) -> ShellConfig {
    let path = dir.join("shell.json");
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => ShellConfig::default(),
    }
}

/// 单文件版：exe 尾部内嵌 zip + 16 字节尾部（标记 + 长度）。
fn embedded_payload() -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let exe = std::env::current_exe().ok()?;
    let mut file = fs::File::open(&exe).ok()?;
    let total = file.metadata().ok()?.len();
    if total < 16 {
        return None;
    }
    file.seek(SeekFrom::End(-16)).ok()?;
    let mut tail = [0u8; 16];
    file.read_exact(&mut tail).ok()?;
    if &tail[0..8] != b"APSHPAY1" {
        return None;
    }
    let len = u64::from_le_bytes(tail[8..16].try_into().ok()?) as usize;
    if len == 0 || len as u64 > total - 16 {
        return None;
    }
    file.seek(SeekFrom::Start(total - 16 - len as u64)).ok()?;
    let mut data = vec![0u8; len];
    file.read_exact(&mut data).ok()?;
    Some(data)
}

/// 把内嵌内容解到缓存目录（按内容哈希分目录，命中就直接复用）。
fn extract_payload(data: &[u8]) -> Option<std::path::PathBuf> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let dir = std::env::temp_dir()
        .join("jianling")
        .join(format!("{hash:016x}"));
    let marker = dir.join(".ready");
    if marker.is_file() {
        return Some(dir);
    }
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data)).ok()?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).ok()?;
        let rel = entry.enclosed_name()?;
        let target = dir.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&target).ok()?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).ok()?;
        }
        let mut out = fs::File::create(&target).ok()?;
        std::io::copy(&mut entry, &mut out).ok()?;
    }
    fs::write(&marker, b"ok").ok()?;
    Some(dir)
}

/// 运行期异常写入 exe 同目录的 shell-error.log，方便排查。
fn log_error(message: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(exe_dir().join("shell-error.log"))
    {
        let _ = writeln!(file, "{message}");
    }
}

/// 把 window.open / target=_blank 的链接通过 IPC 交给宿主，由系统默认浏览器打开。
const EXTERNAL_LINKS_SCRIPT: &str = "\
(function(){\
if(window.__jianlingExtLinks){return;}\
window.__jianlingExtLinks=true;\
function post(u){try{window.ipc.postMessage('jianling-open:'+u);}catch(e){}}\
window.open=function(u){if(u){post(String(u));}return null;};\
document.addEventListener('click',function(e){\
var el=e.target;\
while(el&&el.tagName!=='A'){el=el.parentElement;}\
if(!el){return;}\
var href=el.getAttribute('href')||'';\
if(el.target==='_blank'||(el.getAttribute('rel')||'').split(/\\s+/).indexOf('external')>=0){\
e.preventDefault();\
var abs=el.href||href;\
if(abs){post(abs);}\
}\
},true);\
})();\n";

/// 顶部操作栏：后退 / 前进 / 刷新。纯页面内实现（history / location 即可），
/// 挂载后给 documentElement 加上等高的 padding，避免遮住页面内容。
const NAV_BAR_SCRIPT: &str = r#"(function(){
  var H = 34;
  function build(){
    if (!document.documentElement) { return false; }
    if (document.querySelector('[data-jianling-nav]')) { return true; }
    var bar = document.createElement('div');
    bar.setAttribute('data-jianling-nav','');
    bar.style.cssText = 'position:fixed;top:0;left:0;right:0;height:' + H + 'px;display:flex;'
      + 'align-items:center;gap:6px;padding:0 8px;background:#f5f5f7;border-bottom:1px solid #e0e0e3;'
      + 'z-index:2147483646;font:13px/1 system-ui,sans-serif;color:#333;user-select:none';
    function icon(paths){
      return '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"'
        + ' fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"'
        + ' stroke-linejoin="round">' + paths + '</svg>';
    }
    function btn(paths, title, action){
      var b = document.createElement('button');
      b.type = 'button';
      b.title = title;
      b.style.cssText = 'width:26px;height:22px;display:inline-flex;align-items:center;'
        + 'justify-content:center;border:0;background:transparent;color:#333;cursor:pointer;'
        + 'padding:0;border-radius:6px';
      b.innerHTML = icon(paths);
      b.addEventListener('click', function(e){ e.preventDefault(); action(); }, true);
      return b;
    }
    var ICON_BACK = '<path d="M15 5L9 12L15 19"/>';
    var ICON_FORWARD = '<path d="M9 5L15 12L9 19"/>';
    var ICON_REFRESH = '<path d="M3.67981 13L3.67981 11.3333C3.67981 6.73096 7.4402 3 12.0789 3'
      + 'C15.1178 3 17.7799 4.60136 19.2545 7M2 11.3333L3.67981 13L5.35962 11.3333"/>'
      + '<path d="M20.3139 11V12.6667C20.3139 17.269 16.5391 21 11.8827 21'
      + 'C8.83213 21 6.15995 19.3986 4.67969 17M22.0001 12.6667L20.3139 11L18.6277 12.6667"/>';
    bar.appendChild(btn(ICON_BACK, '后退', function(){ history.back(); }));
    bar.appendChild(btn(ICON_FORWARD, '前进', function(){ history.forward(); }));
    bar.appendChild(btn(ICON_REFRESH, '刷新', function(){ location.reload(); }));
    document.documentElement.appendChild(bar);
    document.documentElement.style.paddingTop = H + 'px';
    return true;
  }
  var tries = 0;
  (function wait(){
    if (build()) { return; }
    if (tries++ < 400) { setTimeout(wait, 10); }
  })();
})();
"#;

/// 顶部操作栏占用的高度，进度条要落在它下沿。
fn nav_bar_height(config: &ShellConfig) -> u32 {
    if config.nav_bar {
        34
    } else {
        0
    }
}

/// 把浏览器行为开关翻译成一段在页面创建时立即执行的脚本。
fn build_init_script(config: &ShellConfig) -> Option<String> {
    // 立即执行的监听：document / window 在文档创建阶段已经存在。
    let mut immediate = String::new();
    if config.external_links_in_browser {
        immediate.push_str(EXTERNAL_LINKS_SCRIPT);
    }
    if config.block_context_menu {
        immediate.push_str(
            "document.addEventListener('contextmenu',function(e){e.preventDefault();},true);\n",
        );
    }
    if config.block_ctrl_zoom {
        immediate.push_str(
            "window.addEventListener('wheel',function(e){if(e.ctrlKey){e.preventDefault();}},{passive:false});\n\
             window.addEventListener('keydown',function(e){if(e.ctrlKey&&(e.key==='+'||e.key==='-'||e.key==='='||e.key==='0')){e.preventDefault();}},true);\n",
        );
    }

    // 样式必须在 documentElement / head 就绪后注入，否则 document_start 阶段会拿不到节点。
    let mut css = String::new();
    if config.block_text_select {
        css.push_str("*{-webkit-user-select:none!important;user-select:none!important}");
    }
    if config.hide_scrollbars {
        css.push_str("::-webkit-scrollbar{display:none!important;width:0!important;height:0!important}html,body{scrollbar-width:none!important}");
    }

    let mut script = immediate;
    if config.nav_bar {
        script.push_str(NAV_BAR_SCRIPT);
    }
    if config.progress_bar {
        script.push_str(&progress_script(config));
    }
    if !css.is_empty() {
        script.push_str(&format!(
            "(function(){{var css={};var done=false;function apply(){{if(done){{return true;}}var de=document.documentElement;if(!de){{return false;}}try{{var s=document.createElement('style');s.setAttribute('data-jianling','');s.textContent=css;(document.head||de).appendChild(s);done=true;return true;}}catch(e){{return false;}}}}if(!apply()){{document.addEventListener('DOMContentLoaded',apply,{{once:true}});}}}})();\n",
            serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into())
        ));
    }

    if script.trim().is_empty() {
        None
    } else {
        Some(script)
    }
}

/// 进度条颜色只接受 #rgb / #rrggbb，其余回退默认，避免拼进脚本时破坏语法。
fn normalize_color(raw: &str) -> String {
    let hex = raw.trim().trim_start_matches('#');
    if !hex.is_empty() && hex.len() <= 8 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("#{hex}")
    } else {
        default_progress_color()
    }
}

/// 顶部加载进度条：每个新文档创建时挂一条，load 后收敛到 100% 再淡出。
/// WebView2 没有逐资源的进度回调，这里用「估算推进 + load 收尾」模拟。
fn progress_script(config: &ShellConfig) -> String {
    let height = config.progress_height.clamp(1, 12);
    let color = normalize_color(&config.progress_color);
    let template = r#"(function(){
  var h = __H__, color = "__COLOR__";
  var bar = null, timer = null, pct = 6, finished = false;
  function mount(){
    if (bar) { return true; }
    if (!document.documentElement) { return false; }
    bar = document.createElement('div');
    bar.setAttribute('data-jianling-progress','');
    bar.style.cssText = 'position:fixed;top:' + __TOP__ + 'px;left:0;height:' + h + 'px;width:0;z-index:2147483647;'
      + 'background:' + color + ';box-shadow:0 0 6px ' + color + ';pointer-events:none;'
      + 'transition:width .18s ease-out,opacity .28s ease-out';
    document.documentElement.appendChild(bar);
    bar.style.width = pct + '%';
    timer = setInterval(function(){
      pct = Math.min(90, pct + (90 - pct) * 0.08);
      if (bar) { bar.style.width = pct + '%'; }
    }, 120);
    return true;
  }
  function finish(){
    if (finished) { return; }
    finished = true;
    if (!mount()) { return; }
    clearInterval(timer);
    bar.style.width = '100%';
    setTimeout(function(){ if (bar) { bar.style.opacity = '0'; } }, 160);
    setTimeout(function(){
      if (bar && bar.parentNode) { bar.parentNode.removeChild(bar); }
      bar = null;
    }, 520);
  }
  var tries = 0;
  (function wait(){
    if (!mount()) {
      if (tries++ < 400) { setTimeout(wait, 10); }
      return;
    }
    if (document.readyState === 'complete') { setTimeout(finish, 60); return; }
    window.addEventListener('load', finish, { once: true });
    setTimeout(finish, 8000);
  })();
})();
"#;
    template
        .replace("__H__", &height.to_string())
        .replace("__TOP__", &nav_bar_height(config).to_string())
        .replace("__COLOR__", &color)
}

fn browser_args(config: &ShellConfig) -> String {
    let mut args: Vec<String> = Vec::new();
    if config.autoplay {
        args.push("--autoplay-policy=no-user-gesture-required".into());
    }
    if config.min_font_size > 0 {
        // Blink 内核级最小字号，覆盖页面自身写死的过小字号。
        args.push(format!(
            "--blink-settings=minimumFontSize={}",
            config.min_font_size.clamp(1, 96)
        ));
    }
    if config.ignore_cert_errors {
        // 内网自签证书：浏览器能给「继续访问」，WebView2 不会，只能显式放开
        args.push("--ignore-certificate-errors".into());
    }
    args.join(" ")
}

#[cfg(windows)]
fn install_permission_handler(
    controller: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
    policy: PermissionPolicy,
) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ,
        COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE, COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS,
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        COREWEBVIEW2_PERMISSION_STATE_DEFAULT, COREWEBVIEW2_PERMISSION_STATE_DENY,
    };
    use webview2_com::PermissionRequestedEventHandler;

    unsafe {
        let core = match controller.CoreWebView2() {
            Ok(core) => core,
            Err(e) => {
                log_error(&format!("获取 CoreWebView2 失败: {e}"));
                return;
            }
        };
        let handler = PermissionRequestedEventHandler::create(Box::new(move |_sender, args| {
            if let Some(args) = args {
                let mut kind = Default::default();
                let _ = args.PermissionKind(&mut kind);
                let state = if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA {
                    policy.camera
                } else if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                    policy.microphone
                } else if kind == COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION {
                    policy.geolocation
                } else if kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS {
                    policy.notifications
                } else if kind == COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ {
                    policy.clipboard
                } else if kind == COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS {
                    policy.downloads
                } else if kind == COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE {
                    policy.file_read_write
                } else {
                    PolicyState::Ask
                };
                let target = match state {
                    PolicyState::Allow => COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                    PolicyState::Deny => COREWEBVIEW2_PERMISSION_STATE_DENY,
                    PolicyState::Ask => COREWEBVIEW2_PERMISSION_STATE_DEFAULT,
                };
                let _ = args.SetState(target);
            }
            Ok(())
        }));
        let mut token = 0i64;
        if let Err(e) = core.add_PermissionRequested(&handler, &mut token) {
            log_error(&format!("注册权限处理器失败: {e}"));
        }
    }
}

/// 直接调用少量 Win32 接口，避免为单实例 / 前台激活额外拉依赖。
#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    pub type Handle = *mut c_void;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateMutexW(attrs: *mut c_void, initial_owner: i32, name: *const u16) -> Handle;
        pub fn GetLastError() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn FindWindowW(class: *const u16, title: *const u16) -> Handle;
        pub fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
        pub fn SetForegroundWindow(hwnd: Handle) -> i32;
    }

    pub const ERROR_ALREADY_EXISTS: u32 = 183;
    pub const SW_RESTORE: i32 = 9;
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 单实例：同名互斥体已存在时把已有窗口拉到前台，并让本次启动退出。
#[cfg(windows)]
fn claim_single_instance(title: &str) -> bool {
    let name = wide(&format!("JianlingShellHost_{title}"));
    unsafe {
        let handle = win32::CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr());
        if handle.is_null() {
            log_error("创建单实例互斥体失败，跳过单实例限制");
            return true;
        }
        if win32::GetLastError() != win32::ERROR_ALREADY_EXISTS {
            // 句柄故意不关闭，随进程退出由系统回收
            return true;
        }
        let target = wide(title);
        let hwnd = win32::FindWindowW(std::ptr::null(), target.as_ptr());
        if !hwnd.is_null() {
            win32::ShowWindow(hwnd, win32::SW_RESTORE);
            win32::SetForegroundWindow(hwnd);
        }
        false
    }
}

/// 开机自启：写 / 删 HKCU 的 Run 键。关闭开关时会主动清理残留项。
#[cfg(windows)]
fn sync_autostart(enable: bool, app_name: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            log_error(&format!("读取自身路径失败，跳过开机自启：{e}"));
            return;
        }
    };
    let cleaned: String = app_name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        .collect();
    let value_name = if cleaned.trim().is_empty() {
        "Jianling".to_string()
    } else {
        cleaned.trim().to_string()
    };

    let mut command = std::process::Command::new("reg.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    if enable {
        let quoted = format!("\"{}\"", exe.to_string_lossy());
        command.args([
            "add", RUN_KEY, "/v", &value_name, "/t", "REG_SZ", "/d", &quoted, "/f",
        ]);
    } else {
        command.args(["delete", RUN_KEY, "/v", &value_name, "/f"]);
    }
    if let Err(e) = command.output() {
        log_error(&format!("同步开机自启失败：{e}"));
    }
}

/// 只放行 http(s) / mailto / tel，避免页面诱导打开本地可执行文件。
fn open_external(url: &str) {
    let lower = url.to_ascii_lowercase();
    let allowed = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:");
    if !allowed {
        log_error(&format!("拒绝打开非网页链接：{url}"));
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = std::process::Command::new("cmd");
        command
            .creation_flags(CREATE_NO_WINDOW)
            .args(["/c", "start", "", url]);
        if let Err(e) = command.spawn() {
            log_error(&format!("打开系统浏览器失败：{e}"));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
    }
}

/// 本地静态包：按配置启动内置服务，返回实际端口；未开启或失败时返回 None（退回 file://）。
fn start_local_server(config: &ShellConfig, index_path: &std::path::Path) -> Option<u16> {
    if !config.local_server.enabled {
        return None;
    }
    let root = index_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(exe_dir);
    let index = index_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "index.html".to_string());
    match server::start(&config.local_server, root, index, log_error) {
        Ok(port) => Some(port),
        Err(e) => {
            log_error(&format!("本地服务启动失败，退回 file:// 加载：{e}"));
            None
        }
    }
}

/// 简易 base64（为一张启动图不引入额外依赖）。
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// 把 exe 目录里的图片读成 data URL（启动屏内联，避免相对路径问题）。
fn image_data_url(name: &str) -> Option<String> {
    let path = exe_dir().join(name);
    let bytes = fs::read(&path).ok()?;
    let mime = match path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    };
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 背景是否偏暗，用于自动挑选文字颜色。
fn color_is_dark(color: &str) -> bool {
    let hex = color.trim().trim_start_matches('#');
    if hex.len() < 6 {
        return false;
    }
    let parse = |range: std::ops::Range<usize>| u32::from_str_radix(&hex[range], 16).unwrap_or(255);
    let (r, g, b) = (parse(0..2), parse(2..4), parse(4..6));
    (r * 299 + g * 587 + b * 114) / 1000 < 140
}

/// 启动屏：整屏图片优先；否则「背景图 + logo + 名称」，背景图优先于背景色。
fn build_splash_html(config: &ShellConfig) -> String {
    let cfg = &config.splash;
    let background = if cfg.background.trim().is_empty() {
        "#FFFFFF"
    } else {
        cfg.background.trim()
    };
    let title = if cfg.title.trim().is_empty() {
        config.title.as_str()
    } else {
        cfg.title.trim()
    };

    let full_image = cfg.image.as_deref().and_then(image_data_url);
    let background_image = if full_image.is_some() {
        None
    } else {
        cfg.background_image.as_deref().and_then(image_data_url)
    };
    let foreground = if background_image.is_some() || color_is_dark(background) {
        "#FFFFFF"
    } else if full_image.is_some() {
        "#FFFFFF"
    } else {
        "#1F2430"
    };

    let content = if let Some(src) = full_image.as_ref() {
        format!("<img class=\"full\" src=\"{src}\" alt=\"\">")
    } else {
        let logo = cfg
            .logo
            .as_deref()
            .and_then(image_data_url)
            .map(|src| format!("<img class=\"logo\" src=\"{src}\" alt=\"\">"))
            .unwrap_or_default();
        format!(
            "<div class=\"center\">{logo}<div class=\"name\">{}</div></div>",
            html_escape(title)
        )
    };

    let body_css = match background_image {
        Some(src) => format!("background:#FFFFFF url('{src}') center/cover no-repeat"),
        None => format!("background:{background}"),
    };

    let css = r#"
html,body{margin:0;padding:0;height:100%;overflow:hidden}
img.full{position:fixed;inset:0;width:100%;height:100%;object-fit:cover;display:block}
.center{position:absolute;left:50%;top:33.3333%;transform:translate(-50%,-50%);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:20px;max-width:72%}
.center img.logo{max-width:120px;max-height:120px;display:block}
.name{font:600 22px/1.45 -apple-system,'Segoe UI','Microsoft YaHei',sans-serif;text-align:center;word-break:break-word}
"#;
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>body{{{body_css}}} .name{{color:{foreground}}}{css}</style></head><body>{content}</body></html>"
    )
}

/// 关闭浏览器快捷键（F5 / Ctrl+R / Ctrl+P / Ctrl+F 等）。开启调试时保持可用。
#[cfg(windows)]
fn apply_browser_keys(
    controller: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
    config: &ShellConfig,
) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows::core::Interface;

    unsafe {
        let core = match controller.CoreWebView2() {
            Ok(core) => core,
            Err(e) => {
                log_error(&format!("获取 CoreWebView2 失败: {e}"));
                return;
            }
        };
        let settings = match core.Settings() {
            Ok(settings) => settings,
            Err(e) => {
                log_error(&format!("获取 WebView 设置失败: {e}"));
                return;
            }
        };
        let enabled = !config.block_browser_keys || config.devtools;
        match settings.cast::<ICoreWebView2Settings3>() {
            Ok(settings3) => {
                if let Err(e) = settings3.SetAreBrowserAcceleratorKeysEnabled(enabled) {
                    log_error(&format!("设置浏览器快捷键开关失败: {e}"));
                }
            }
            Err(e) => log_error(&format!("CoreWebView2 不支持快捷键开关: {e}")),
        }
    }
}

/// 内核默认 UA：用来拼「追加后缀」模式。内核版本优先取自带内核目录名，否则扫系统运行时。
#[cfg(windows)]
fn default_user_agent(config: &ShellConfig) -> Option<String> {
    let version = config
        .webview2
        .as_ref()
        .and_then(|w| w.version.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(system_runtime_version)?;
    let major = version.split('.').next()?.to_string();
    let arch = if cfg!(target_arch = "aarch64") {
        "ARM64"
    } else {
        "x64"
    };
    Some(format!(
        "Mozilla/5.0 (Windows NT 10.0; {arch}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36 Edg/{version}"
    ))
}

#[cfg(not(windows))]
fn default_user_agent(_config: &ShellConfig) -> Option<String> {
    None
}

/// 系统安装的 WebView2 运行时版本（取版本号最大的那个目录）
#[cfg(windows)]
fn system_runtime_version() -> Option<String> {
    let root = PathBuf::from(std::env::var_os("ProgramFiles(x86)")?)
        .join("Microsoft")
        .join("EdgeWebView")
        .join("Application");
    let mut best: Option<(Vec<u32>, String)> = None;
    for entry in fs::read_dir(&root).ok()?.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let parts: Vec<u32> = name.split('.').filter_map(|p| p.parse().ok()).collect();
        if parts.len() < 4 {
            continue;
        }
        if best.as_ref().map(|(v, _)| parts > *v).unwrap_or(true) {
            best = Some((parts, name));
        }
    }
    best.map(|(_, name)| name)
}

/// 固定比例窗口（如 9:16）：目标尺寸放不进屏幕时等比缩小，避免被系统裁掉高度后比例失真。
fn fit_ratio_to_screen(window: &tao::window::Window, width: f64, height: f64) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let Some(monitor) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        return;
    }
    let screen = monitor.size();
    // 预留标题栏与任务栏余量
    let max_w = screen.width as f64 / scale - 32.0;
    let max_h = screen.height as f64 / scale - 96.0;
    if max_w <= 0.0 || max_h <= 0.0 {
        return;
    }
    let ratio = width / height;
    let mut w = width;
    let mut h = height;
    if h > max_h {
        h = max_h;
        w = h * ratio;
    }
    if w > max_w {
        w = max_w;
        h = w / ratio;
    }
    if (w - width).abs() < 0.5 && (h - height).abs() < 0.5 {
        return;
    }
    window.set_inner_size(tao::dpi::LogicalSize::new(w, h));
    let outer = window.outer_size();
    let origin = monitor.position();
    let x = origin.x + (screen.width as i32 - outer.width as i32) / 2;
    let y = origin.y + (screen.height as i32 - outer.height as i32) / 2;
    window.set_outer_position(tao::dpi::PhysicalPosition::new(x, y));
}

/// 窗口位置/尺寸记忆文件（放在 exe 同级，绿色版拷走也能带着）
fn window_state_path() -> PathBuf {
    exe_dir().join("window.json")
}

fn load_window_state() -> Option<(i32, i32, u32, u32)> {
    let raw = fs::read_to_string(window_state_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let x = value.get("x")?.as_i64()? as i32;
    let y = value.get("y")?.as_i64()? as i32;
    let w = value.get("w")?.as_u64()? as u32;
    let h = value.get("h")?.as_u64()? as u32;
    if w < 200 || h < 150 {
        return None;
    }
    // 记录到畸形坐标时宁可回到默认居中
    if x < -20000 || y < -20000 || x > 20000 || y > 20000 {
        return None;
    }
    Some((x, y, w, h))
}

fn save_window_state(window: &tao::window::Window) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let size = window.inner_size();
    let value = serde_json::json!({
        "x": position.x,
        "y": position.y,
        "w": size.width,
        "h": size.height
    });
    if let Err(e) = fs::write(window_state_path(), value.to_string()) {
        log_error(&format!("保存窗口位置失败: {e}"));
    }
}

/// 系统「下载」已知文件夹（可能被用户改到别的盘），取不到时退回 %USERPROFILE%\Downloads。
#[cfg(windows)]
fn system_download_dir() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    unsafe {
        let raw = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let value = raw.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(raw.0 as *const _));
        value.filter(|path| !path.as_os_str().is_empty())
    }
}

#[cfg(not(windows))]
fn system_download_dir() -> Option<PathBuf> {
    None
}

/// 下载目录：开启「统一存到应用子目录」时落到「下载\<应用名>」，否则就用系统下载目录本身。
/// 必须显式给出路径：WebView2 的 DownloadStarting 一旦标记为已处理，不指定路径就不会落盘。
fn download_target(title: &str, to_subfolder: bool) -> Option<PathBuf> {
    let root = system_download_dir()
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join("Downloads"))
        })
        .unwrap_or_else(|| exe_dir().join("downloads"));
    let name = title.trim();
    let dir = if to_subfolder && !name.is_empty() {
        root.join(name)
    } else {
        root
    };
    if let Err(e) = fs::create_dir_all(&dir) {
        log_error(&format!("创建下载目录失败: {e}"));
        return None;
    }
    Some(dir)
}

fn main() {
    // 单文件版：页面资源内嵌在 exe 尾部，先解到缓存目录再当普通目录用
    let content_base = embedded_payload()
        .and_then(|data| extract_payload(&data))
        .unwrap_or_else(exe_dir);
    let config = load_config(&content_base);

    // 自带内核：把 WebView2 固定版本运行时目录告诉内核加载器（必须在创建 WebView 之前）
    if let Some(reldir) = config
        .webview2
        .as_ref()
        .and_then(|w| w.fixed_dir.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let dir = content_base.join(reldir);
        if dir.join("msedgewebview2.exe").is_file() {
            std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &dir);
        } else {
            log_error(&format!(
                "内置内核缺失，回退系统 WebView2: {}",
                dir.display()
            ));
        }
    }

    #[cfg(windows)]
    {
        if config.single_instance && !claim_single_instance(&config.title) {
            return;
        }
        sync_autostart(config.auto_start, &config.title);
    }

    let event_loop = tao::event_loop::EventLoopBuilder::new().build();

    // 固定比例：标题栏不显示最大化按钮，拖动时按比例联动
    let ratio_lock = if config.size_mode == "ratio" && !config.fullscreen {
        Some(config.aspect_w.max(1.0) / config.aspect_h.max(1.0))
    } else {
        None
    };

    // 记住窗口位置：恢复上次的物理像素位置与尺寸（全屏 / 启动最大化时忽略）
    let restored = if config.remember_window && !config.fullscreen && !config.maximized {
        load_window_state()
    } else {
        None
    };

    let mut window_builder = tao::window::WindowBuilder::new()
        .with_title(&config.title)
        .with_inner_size(tao::dpi::LogicalSize::new(config.width, config.height))
        .with_resizable(config.resizable)
        .with_maximizable(ratio_lock.is_none())
        .with_decorations(!config.frameless)
        .with_always_on_top(config.always_on_top)
        .with_maximized(config.maximized)
        .with_fullscreen(if config.fullscreen {
            Some(tao::window::Fullscreen::Borderless(None))
        } else {
            None
        });
    if let Some((x, y, w, h)) = restored {
        window_builder = window_builder
            .with_inner_size(tao::dpi::PhysicalSize::new(w, h))
            .with_position(tao::dpi::PhysicalPosition::new(x, y));
    }

    let window = match window_builder.build(&event_loop) {
        Ok(window) => window,
        Err(e) => {
            eprintln!("无法创建窗口: {e}");
            return;
        }
    };

    if !config.fullscreen && !config.maximized && restored.is_none() {
        fit_ratio_to_screen(&window, config.width, config.height);
    }

    if config.min_width > 0 || config.min_height > 0 {
        window.set_min_inner_size(Some(tao::dpi::LogicalSize::new(
            if config.min_width > 0 {
                config.min_width as f64
            } else {
                200.0
            },
            if config.min_height > 0 {
                config.min_height as f64
            } else {
                150.0
            },
        )));
    }

    // 关闭按钮行为：exit 退出 / minimize 收进任务栏 / tray 收进托盘（空值按旧字段推断）
    let close_mode = match config.close_mode.trim() {
        "exit" | "minimize" | "tray" => config.close_mode.trim().to_string(),
        _ => {
            if config.minimize_on_close {
                "minimize".to_string()
            } else {
                "exit".to_string()
            }
        }
    };

    #[cfg(windows)]
    let tray = if close_mode == "tray" {
        use tao::platform::windows::WindowExtWindows;
        match tray::install(&config.title, window.hwnd()) {
            Some(tray) => Some(tray),
            None => {
                log_error("托盘图标创建失败，关闭行为退回最小化");
                None
            }
        }
    } else {
        None
    };

    let target = match &config.local_index {
        Some(relative) => {
            let path = content_base.join(relative);
            match start_local_server(&config, &path) {
                Some(port) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "index.html".to_string());
                    format!(
                        "http://127.0.0.1:{port}{}{name}",
                        server::normalize_base(&config.local_server.base_path)
                    )
                }
                None => format!("file:///{}", path.to_string_lossy().replace('\\', "/")),
            }
        }
        None => config.url.clone(),
    };

    let splash_html = if config.splash.enabled {
        Some(build_splash_html(&config))
    } else {
        None
    };

    let mut builder = wry::WebViewBuilder::new();
    builder = match &splash_html {
        Some(html) => builder.with_html(html),
        None => builder.with_url(&target),
    };
    // 自定义 UA 必须在创建 WebView 时设定：运行起来后再改不影响页面
    let ua_suffix = config.user_agent.trim();
    if !ua_suffix.is_empty() {
        match config.ua_mode.trim() {
            "override" => builder = builder.with_user_agent(ua_suffix),
            "append" => match default_user_agent(&config) {
                Some(base) => builder = builder.with_user_agent(format!("{base} {ua_suffix}")),
                None => log_error("无法确定内核默认 UA，已跳过 UA 追加"),
            },
            _ => {}
        }
    }
    let mut builder = builder
        .with_devtools(config.devtools)
        .with_hotkeys_zoom(!config.block_ctrl_zoom)
        .with_background_color((255, 255, 255, 255));

    let args = browser_args(&config);
    if !args.is_empty() {
        #[cfg(windows)]
        {
            use wry::WebViewBuilderExtWindows;
            builder = builder.with_additional_browser_args(&args);
        }
    }
    if config.external_links_in_browser {
        builder = builder.with_ipc_handler(|request| {
            let body = request.body();
            if let Some(url) = body.strip_prefix("jianling-open:") {
                open_external(url.trim());
            }
        });
    }
    // 下载：勾了「存到应用子目录」落到「下载\<应用名>」，否则就落系统下载目录
    let download_dir = download_target(&config.title, config.download_to_subfolder);
    builder = builder.with_download_started_handler(move |_url, path| {
        if let Some(dir) = &download_dir {
            let file_name = path
                .file_name()
                .map(|name| name.to_os_string())
                .unwrap_or_else(|| std::ffi::OsString::from("download"));
            *path = dir.join(file_name);
        }
        true
    });
    // window.open / target=_blank：不在套壳内开空白窗，交给默认浏览器
    let open_new_window_external = config.external_links_in_browser;
    builder = builder.with_new_window_req_handler(move |url, _features| {
        if open_new_window_external && (url.starts_with("http://") || url.starts_with("https://")) {
            open_external(&url);
        }
        wry::NewWindowResponse::Deny
    });
    if let Some(script) = build_init_script(&config) {
        builder = builder.with_initialization_script(&script);
    }

    let policy = config.permissions.clone();
    let webview = match builder.build(&window) {
        Ok(webview) => webview,
        Err(e) => {
            eprintln!("无法创建 WebView: {e}");
            return;
        }
    };

    #[cfg(windows)]
    {
        use wry::WebViewExtWindows;
        let controller = webview.controller();
        let zoom = config.zoom.clamp(0.5, 3.0);
        if (zoom - 1.0).abs() > 1e-6 {
            let result = unsafe { controller.SetZoomFactor(zoom) };
            if let Err(e) = result {
                log_error(&format!("设置页面缩放失败: {e}"));
            }
        }
        install_permission_handler(&controller, policy.clone());
        apply_browser_keys(&controller, &config);
    }
    #[cfg(not(windows))]
    {
        let _ = &policy;
    }

    let remember_window = config.remember_window;
    let close_mode_for_loop = close_mode.clone();
    // 托盘对象要活到进程结束
    #[cfg(windows)]
    let tray_guard = tray;
    // 启动屏最短展示时长；到点后切到真正的页面
    let mut splash_deadline = splash_html.as_ref().map(|_| {
        std::time::Instant::now()
            + std::time::Duration::from_millis(config.splash.min_show_ms.min(10000) as u64)
    });
    let target_for_splash = target.clone();
    let mut last_size = window.inner_size();

    event_loop.run(move |event, _, control_flow| {
        if let Some(deadline) = splash_deadline {
            if std::time::Instant::now() >= deadline {
                if let Err(e) = webview.load_url(&target_for_splash) {
                    log_error(&format!("加载主页面失败：{e}"));
                }
                splash_deadline = None;
            }
        }
        *control_flow = match splash_deadline {
            Some(deadline) => tao::event_loop::ControlFlow::WaitUntil(deadline),
            None => tao::event_loop::ControlFlow::Wait,
        };
        if let Some(rw) = ratio_lock {
            if let tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::Resized(_),
                ..
            } = &event
            {
                let inner = window.inner_size();
                let prev = last_size;
                last_size = inner;
                let dw = (inner.width as i64 - prev.width as i64).abs();
                let dh = (inner.height as i64 - prev.height as i64).abs();
                if dh > dw {
                    let w = (inner.height as f64 * rw).round() as u32;
                    if w > 0 && inner.width.abs_diff(w) > 1 {
                        window.set_inner_size(tao::dpi::PhysicalSize::new(w, inner.height));
                    }
                } else {
                    let h = (inner.width as f64 / rw).round() as u32;
                    if h > 0 && inner.height.abs_diff(h) > 1 {
                        window.set_inner_size(tao::dpi::PhysicalSize::new(inner.width, h));
                    }
                }
            }
        }
        if let tao::event::Event::WindowEvent {
            event: tao::event::WindowEvent::CloseRequested,
            ..
        } = event
        {
            if remember_window && !window.is_maximized() {
                save_window_state(&window);
            }
            #[cfg(windows)]
            let force_exit = tray::exit_requested();
            #[cfg(not(windows))]
            let force_exit = false;
            if force_exit {
                *control_flow = tao::event_loop::ControlFlow::Exit;
            } else {
                match close_mode_for_loop.as_str() {
                    "tray" => window.set_visible(false),
                    "minimize" => window.set_minimized(true),
                    _ => *control_flow = tao::event_loop::ControlFlow::Exit,
                }
            }
        }
        let _ = &webview;
        #[cfg(windows)]
        let _ = &tray_guard;
    });
}
