import { invoke } from "@tauri-apps/api/core";
import type { EnvReport } from "./types";

export interface PermissionPrompt {
  enabled: boolean;
  title: string;
  message: string;
}

export const DEFAULT_PERMISSION_PROMPTS: Record<string, PermissionPrompt> = {
  camera: {
    enabled: false,
    title: "相机权限",
    message: "用于拍摄照片、扫码或录制视频，仅在相关功能使用时访问摄像头。",
  },
  microphone: {
    enabled: false,
    title: "麦克风权限",
    message: "用于录制语音、语音通话或语音输入，仅在相关功能使用时访问麦克风。",
  },
  storage: {
    enabled: false,
    title: "存储权限",
    message: "用于选择、保存图片与文件，仅在相关功能使用时访问存储空间。",
  },
  location: {
    enabled: false,
    title: "地理位置权限",
    message: "用于提供基于位置的服务、定位导航与附近搜索等功能。",
  },
  notification: {
    enabled: true,
    title: "通知权限",
    message: "用于向你推送消息提醒、业务通知与重要状态变更。",
  },
  bluetooth: {
    enabled: true,
    title: "蓝牙权限",
    message: "用于连接蓝牙设备（耳机、手环等外设）并与其通信。",
  },
  nfc: {
    enabled: true,
    title: "NFC 权限",
    message: "用于读取 NFC 标签，实现碰一碰、卡片识别等功能。",
  },
  vibrate: {
    enabled: true,
    title: "振动权限",
    message: "用于在消息到达或操作完成时提供振动反馈。",
  },
  foreground: {
    enabled: true,
    title: "前台服务权限",
    message: "用于在后台保持通话或录音运行，并在通知栏显示运行状态。",
  },
  call: {
    enabled: true,
    title: "拨打电话权限",
    message: "用于直接发起电话呼叫，省去手动确认的步骤。",
  },
  install: {
    enabled: true,
    title: "应用安装权限",
    message: "用于下载并安装应用更新，保持应用为最新版本。",
  },
};

export interface AndroidProfile {
  appName: string;
  packageName: string;
  versionName: string;
  versionCode: number;
  sourceKind: "remote" | "local";
  startUrl: string;
  localDir: string | null;
  fullscreen: boolean;
  statusBarColor: string;
  /** 状态栏透明、内容从屏幕顶端开始（边到边）；关闭时用 statusBarColor 绘制状态栏 */
  edgeToEdge: boolean;
  /** 注入给页面的状态栏高度 CSS 变量名；默认 --status-bar-height（与 uni-app 同名） */
  statusBarCssVar: string;
  /** 只隐藏导航栏、保留状态栏（底部上滑临时呼出）；全屏启动时无意义 */
  hideNavigationBar: boolean;
  /** auto 跟随系统旋转；landscape / portrait 锁定方向 */
  screenOrientation: "auto" | "landscape" | "portrait";
  ignoreSslErrors: boolean;
  allowCleartext: boolean;
  permissions: string[];
  iconPath: string | null;
  iconBackgroundColor: string;
  keystorePath: string | null;
  keystoreAlias: string | null;
  keystoreStorePassword: string | null;
  keystoreKeyPassword: string | null;
  textZoom: number;
  allowAutoplay: boolean;
  allowZoom: boolean;
  disableLongPress: boolean;
  hideScrollbars: boolean;
  allowThirdPartyCookies: boolean;
  videoFullscreen: boolean;
  debugEnabled: boolean;
  /** 调试包注入的页内调试面板 */
  debugConsole: "vconsole" | "eruda" | "none";
  /** 站外 http(s) 链接改用系统浏览器打开 */
  openExternalInBrowser: boolean;
  cacheMode: "default" | "no_cache" | "cache_first";
  /** append：系统 UA + 应用标记；override：整串替换 */
  uaMode: "append" | "override";
  userAgent: string;
  pullToRefresh: boolean;
  exitOnBackPressed: boolean;
  /** 禁止返回键退出应用：网页退不了时返回键无效，应用不退出 */
  disableBackExit: boolean;
  /** 顶部加载进度条 */
  progressBar: boolean;
  progressColor: string;
  /** 进度条高度（dp） */
  progressHeight: number;
  /** 顶部浏览器操作栏（前进 / 后退 / 刷新） */
  navBar: boolean;
  minSdk: number;
  targetSdk: number;
  compileSdk: number;
  lightStatusBar: boolean;
  keepScreenOn: boolean;
  secureFlag: boolean;
  localServer: LocalServerConfig;
  splash: SplashConfig;
  permissionPrompts: Record<string, PermissionPrompt>;
}

export interface BuildRecord {
  id: string;
  /** 构建水印 ID（旧记录没有） */
  buildId?: string;
  apkPath: string;
  sizeBytes: number;
  signed: boolean;
  versionName: string;
  versionCode: number;
  durationSecs: number;
  createdAt: number;
  logPath: string;
  platform: "android" | "windows";
  /** 构建是否失败；旧记录没有该字段，缺省视为成功 */
  failed: boolean;
  /** 失败原因 */
  error: string;
}

export interface SplashConfig {
  enabled: boolean;
  /** default：背景色/背景图 + 名称 + Logo；custom：整屏图片 */
  mode: "default" | "custom";
  title: string;
  backgroundColor: string;
  /** 默认模式的背景图源路径（优先于背景色） */
  backgroundImage: string;
  /** 整屏图片源路径，空字符串表示不使用 */
  imagePath: string;
  /** logo 图片源路径，留空用应用图标 */
  logoPath: string;
  minShowMs: number;
}

export const DEFAULT_SPLASH: SplashConfig = {
  enabled: false,
  mode: "default",
  title: "",
  backgroundColor: "#FFFFFF",
  backgroundImage: "",
  imagePath: "",
  logoPath: "",
  minShowMs: 600,
};
export type WindowsPermissionState = "ask" | "allow" | "deny";

export interface ProxyRule {
  prefix: string;
  target: string;
  rewriteOrigin: boolean;
  insecure: boolean;
  /** 可选：把上游域名固定解析到该 IP（域名只在本机 hosts 里能解析时用） */
  resolveIp: string;
  /** 可选：追加到上游请求的 header，JSON 对象文本，例如 {"X-Token":"abc"} */
  headers: string;
}

export interface LocalServerConfig {
  enabled: boolean;
  port: number;
  /** 部署目录：站点挂在域名子目录时填 /h5/，根目录用 / */
  basePath: string;
  proxies: ProxyRule[];
}

export const DEFAULT_PROXY_RULE: ProxyRule = {
  prefix: "/api",
  target: "",
  rewriteOrigin: true,
  insecure: false,
  resolveIp: "",
  headers: "",
};

export const DEFAULT_LOCAL_SERVER: LocalServerConfig = {
  enabled: false,
  port: 0,
  basePath: "/",
  proxies: [{ ...DEFAULT_PROXY_RULE }],
};

/**
 * 校验转发规则的「追加 header」：空值合法；否则必须是 JSON 对象，值只能是字符串、数字或布尔。
 * 返回错误说明，空串表示通过。
 */
export function proxyHeadersError(text: string | null | undefined): string {
  const raw = (text ?? "").trim();
  if (!raw) return "";
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    return `JSON 格式错误：${(err as Error).message}`;
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return '需要 JSON 对象，例如 {"X-Token": "abc"}';
  }
  for (const [name, value] of Object.entries(parsed as Record<string, unknown>)) {
    if (!name.trim()) return "header 名不能为空";
    if (value === null || typeof value === "object") {
      return `header「${name}」的值必须是字符串、数字或布尔`;
    }
  }
  return "";
}

/** 统一成「以 / 开头并以 / 结尾」的部署目录，空值即根目录 /。 */
export function normalizeBasePath(raw: string | null | undefined): string {
  const text = (raw ?? "").trim().replace(/^\/+|\/+$/g, "");
  return text ? `/${text}/` : "/";
}
export interface WindowsPermissions {
  camera: WindowsPermissionState;
  microphone: WindowsPermissionState;
  geolocation: WindowsPermissionState;
  notifications: WindowsPermissionState;
  clipboard: WindowsPermissionState;
  downloads: WindowsPermissionState;
  fileReadWrite: WindowsPermissionState;
}

export const DEFAULT_WINDOWS_PERMISSIONS: WindowsPermissions = {
  camera: "ask",
  microphone: "ask",
  geolocation: "ask",
  notifications: "ask",
  clipboard: "ask",
  downloads: "ask",
  fileReadWrite: "ask",
};

export interface WindowsProfile {
  sourceKind: "remote" | "local";
  startUrl: string;
  localDir: string | null;
  sizeMode: "fixed" | "ratio" | "fullscreen";
  width: number;
  height: number;
  aspectW: number;
  aspectH: number;
  resizable: boolean;
  frameless: boolean;
  alwaysOnTop: boolean;
  maximized: boolean;
  debugEnabled: boolean;
  singleInstance: boolean;
  autoStart: boolean;
  minimizeOnClose: boolean;
  /** 关闭按钮行为：exit 退出 / minimize 收进任务栏 / tray 收进托盘 */
  closeMode: "exit" | "minimize" | "tray";
  zoom: number;
  autoplay: boolean;
  blockContextMenu: boolean;
  blockTextSelect: boolean;
  hideScrollbars: boolean;
  blockCtrlZoom: boolean;
  blockBrowserKeys: boolean;
  externalLinksInBrowser: boolean;
  minFontSize: number;
  /** 忽略 https 证书错误（内网自签证书常用） */
  ignoreCertErrors: boolean;
  /** 页面加载时在顶部显示进度条 */
  progressBar: boolean;
  /** 顶部操作栏（后退 / 前进 / 刷新） */
  navBar: boolean;
  progressColor: string;
  /** 进度条高度（px） */
  progressHeight: number;
  /** UA 策略：append 追加后缀 / override 整串替换（后缀为空时不动 UA） */
  uaMode: "append" | "override";
  userAgent: string;
  /** 下载文件统一存到「下载\<应用名>」子目录 */
  downloadToSubfolder: boolean;
  /** 最小窗口尺寸（0 = 不限制） */
  minWidth: number;
  minHeight: number;
  /** 记住窗口位置与大小 */
  rememberWindow: boolean;
  localServer: LocalServerConfig;
  splash: SplashConfig;
  permissions: WindowsPermissions;
  /** 产物形态：单文件 exe / 安装包 / 绿色版 zip */
  outputSingle: boolean;
  outputInstaller: boolean;
  outputPortable: boolean;
  /** 安装包安装范围：user（当前用户）/ machine（全机器，需管理员） */
  installScope: "user" | "machine";
  certPath: string | null;
  certPassword: string | null;
  /** 自带内核（设置页内核列表的 id），null = 跟随系统 */
  kernelId: string | null;
}

export const DEFAULT_WINDOWS: WindowsProfile = {
  sourceKind: "remote",
  startUrl: "",
  localDir: null,
  sizeMode: "fixed",
  width: 1200,
  height: 820,
  aspectW: 16,
  aspectH: 9,
  resizable: true,
  frameless: false,
  alwaysOnTop: false,
  maximized: false,
  debugEnabled: false,
  singleInstance: true,
  autoStart: false,
  minimizeOnClose: false,
  closeMode: "exit",
  zoom: 1,
  autoplay: true,
  blockContextMenu: false,
  blockTextSelect: false,
  hideScrollbars: true,
  blockCtrlZoom: false,
  blockBrowserKeys: false,
  externalLinksInBrowser: false,
  minFontSize: 0,
  ignoreCertErrors: true,
  progressBar: false,
  navBar: false,
  progressColor: "#2E7D32",
  progressHeight: 1,
  uaMode: "append",
  userAgent: "",
  downloadToSubfolder: true,
  minWidth: 0,
  minHeight: 0,
  rememberWindow: false,
  localServer: DEFAULT_LOCAL_SERVER,
  splash: { ...DEFAULT_SPLASH },
  permissions: DEFAULT_WINDOWS_PERMISSIONS,
  outputSingle: false,
  outputInstaller: true,
  outputPortable: false,
  installScope: "user",
  certPath: null,
  certPassword: null,
  kernelId: null,
};

export interface Project {
  id: string;
  name: string;
  /** 产物输出目录（每个项目必填） */
  outputDir: string | null;
  target: string;
  android: AndroidProfile;
  windows: WindowsProfile;
  createdAt: number;
  updatedAt: number;
  builds: BuildRecord[];
}

export interface Settings {
  jdkPath: string | null;
  sdkPath: string | null;
  /** adb.exe 路径（留空自动探测） */
  adbPath: string | null;
  projects: Project[];
  /** 已下载/导入的 Windows 内核（WebView2 固定版本运行时） */
  windowsKernels: WindowsKernel[];
}

export interface WindowsKernel {
  id: string;
  version: string;
  /** 运行时目录（含 msedgewebview2.exe） */
  path: string;
  source: "auto" | "local" | string;
  addedAt: number;
  sizeBytes: number;
}

export interface KernelVersion {
  version: string;
  url: string;
}

export interface KernelProgress {
  stage: "download" | "extract" | "done" | string;
  received: number;
  total: number;
}

export const DEFAULT_ANDROID: AndroidProfile = {
  appName: "",
  packageName: "com.jianling.app",
  versionName: "0.0.1",
  versionCode: 1,
  sourceKind: "remote",
  startUrl: "",
  localDir: null,
  fullscreen: false,
  statusBarColor: "#1F2430",
  edgeToEdge: true,
  statusBarCssVar: "--status-bar-height",
  hideNavigationBar: false,
  screenOrientation: "auto",
  ignoreSslErrors: false,
  allowCleartext: false,
  permissions: ["camera", "microphone", "storage", "location"],
  iconPath: null,
  iconBackgroundColor: "#ffffff",
  keystorePath: null,
  keystoreAlias: null,
  keystoreStorePassword: null,
  keystoreKeyPassword: null,
  textZoom: 100,
  allowAutoplay: true,
  allowZoom: false,
  disableLongPress: false,
  hideScrollbars: true,
  allowThirdPartyCookies: true,
  videoFullscreen: false,
  debugEnabled: false,
  debugConsole: "eruda",
  openExternalInBrowser: false,
  cacheMode: "default",
  uaMode: "append",
  userAgent: "",
  pullToRefresh: false,
  exitOnBackPressed: true,
  disableBackExit: false,
  progressBar: false,
  progressColor: "#2E7D32",
  progressHeight: 1,
  navBar: false,
  minSdk: 24,
  targetSdk: 34,
  compileSdk: 34,
  lightStatusBar: false,
  keepScreenOn: false,
  secureFlag: false,
  localServer: DEFAULT_LOCAL_SERVER,
  splash: { ...DEFAULT_SPLASH },
  permissionPrompts: DEFAULT_PERMISSION_PROMPTS,
};

export interface AndroidBuildRequest extends AndroidProfile {
  /** 项目 id（写进构建水印，便于反查） */
  projectId: string;
  sdkPath: string | null;
  jdkPath: string | null;
  outputDir: string | null;
  /** 构建调试包：包名加 .debug 且注入页内调试面板 */
  debugBuild: boolean;
}

export interface BuildResult {
  /** 本次构建的水印 ID */
  buildId: string;
  apkPath: string;
  sizeBytes: number;
  signed: boolean;
  logPath: string;
  workDir: string;
  durationSecs: number;
}

export function fetchWindowsKernelVersions() {
  return invoke<KernelVersion[]>("fetch_windows_kernel_versions");
}

export function downloadWindowsKernel(version: string, url: string) {
  return invoke<WindowsKernel>("download_windows_kernel", { version, url });
}

export function importWindowsKernel(dir: string) {
  return invoke<WindowsKernel>("import_windows_kernel", { dir });
}

export function deleteWindowsKernel(id: string, path: string) {
  return invoke<void>("delete_windows_kernel", { id, path });
}

export function checkEnvironment(settings: Partial<Settings> = {}) {
  return invoke<EnvReport>("check_environment", {
    jdkPath: settings.jdkPath ?? null,
    sdkPath: settings.sdkPath ?? null,
  });
}

export function loadSettings() {
  return invoke<Settings>("load_settings");
}

export function saveSettings(settings: Settings) {
  return invoke<void>("save_settings", { settings });
}

/** 读取构建日志文件内容（供历史记录在弹窗中查看） */
export function readBuildLog(path: string) {
  return invoke<string>("read_build_log", { path });
}

export interface AdbDevice {
  serial: string;
  /** device | offline | unauthorized */
  state: string;
  model: string;
  product: string;
}

/** 列出已连接设备 */
export function adbDevices() {
  return invoke<AdbDevice[]>("adb_devices");
}

/** 设备当前 System WebView 版本 */
export function adbWebviewVersion(serial: string) {
  return invoke<string>("adb_webview_version", { serial });
}

/** 设备最近 logcat（尾部若干行，keyword 可选过滤） */
export function adbLogcatTail(serial: string, lines = 300, filter = "") {
  return invoke<string>("adb_logcat_tail", { serial, lines, filter });
}

export function adbLogcatClear(serial: string) {
  return invoke<void>("adb_logcat_clear", { serial });
}

/** 安装（覆盖）APK 到设备 */
export function adbInstallApk(serial: string, path: string) {
  return invoke<string>("adb_install_apk", { serial, path });
}

/** 启动设备上的应用 */
export function adbLaunchApp(serial: string, packageName: string) {
  return invoke<void>("adb_launch_app", { serial, package: packageName });
}

/** 复制文本到系统剪贴板 */
export function copyToClipboard(text: string) {
  return invoke<void>("copy_to_clipboard", { text });
}

/** 屏幕常亮（避免锁屏后 CDP 被节流） */
export function adbKeepScreen(serial: string, on: boolean) {
  return invoke<void>("adb_keep_screen", { serial, on });
}

/** 从候选包名里挑出设备上已安装的（判断正式包 / 调试包） */
export function adbInstalledPackages(serial: string, packages: string[]) {
  return invoke<string[]>("adb_installed_packages", { serial, packages });
}

export interface DevtoolsInfo {
  port: number;
  targetId: string;
  title: string;
  url: string;
  /** 粘到浏览器地址栏即可打开的完整 DevTools 地址 */
  devtoolsUrl: string;
}

/** 建立 adb forward 并返回可直接打开的 DevTools 地址 */
export function devtoolsConnect(serial: string, packageName: string) {
  return invoke<DevtoolsInfo>("devtools_connect", { serial, package: packageName });
}

export function devtoolsDisconnect(serial: string) {
  return invoke<void>("devtools_disconnect", { serial });
}

export function buildAndroid(request: AndroidBuildRequest) {
  return invoke<BuildResult>("build_android", { request });
}

export interface InstallProgress {
  kind: string;
  phase: string;
  percent: number;
  message: string;
}

export interface WindowsBuildResult {
  /** 本次构建的水印 ID */
  buildId: string;
  dir: string;
  exePath: string;
  sizeBytes: number;
  signed: boolean;
  durationSecs: number;
  logPath: string;
  /** 单文件 exe（内嵌页面资源） */
  singleExe: string | null;
  /** NSIS 安装包 */
  installer: string | null;
  /** 绿色版 zip */
  portableZip: string | null;
}

export interface WindowsBuildRequest {
  /** 项目 id（写进构建水印，便于反查） */
  projectId: string;
  appName: string;
  versionName: string;
  outputDir: string | null;
  sourceKind: string;
  startUrl: string;
  localDir: string | null;
  /** 应用图标（启动屏 logo 缺省时使用） */
  iconPath: string | null;
  sizeMode: string;
  width: number;
  height: number;
  aspectW: number;
  aspectH: number;
  resizable: boolean;
  frameless: boolean;
  alwaysOnTop: boolean;
  maximized: boolean;
  debugEnabled: boolean;
  singleInstance: boolean;
  autoStart: boolean;
  minimizeOnClose: boolean;
  /** 关闭按钮行为：exit 退出 / minimize 收进任务栏 / tray 收进托盘 */
  closeMode: "exit" | "minimize" | "tray";
  zoom: number;
  autoplay: boolean;
  blockContextMenu: boolean;
  blockTextSelect: boolean;
  hideScrollbars: boolean;
  blockCtrlZoom: boolean;
  blockBrowserKeys: boolean;
  externalLinksInBrowser: boolean;
  minFontSize: number;
  ignoreCertErrors: boolean;
  progressBar: boolean;
  navBar: boolean;
  progressColor: string;
  progressHeight: number;
  uaMode: "append" | "override";
  userAgent: string;
  downloadToSubfolder: boolean;
  minWidth: number;
  minHeight: number;
  rememberWindow: boolean;
  localServer: LocalServerConfig;
  splash: SplashConfig;
  permissions: WindowsPermissions;
  outputSingle: boolean;
  outputInstaller: boolean;
  outputPortable: boolean;
  /** 安装包安装范围：user（当前用户）/ machine（全机器，需管理员） */
  installScope: "user" | "machine";
  certPath: string | null;
  certPassword: string | null;
  /** 自带内核目录，null = 跟随系统 */
  kernelPath: string | null;
}

/** 导出 Windows 桌面套壳：exe + www + shell.json 的免安装目录。 */
export function buildWindows(request: WindowsBuildRequest) {
  return invoke<WindowsBuildResult>("build_windows", { request });
}

/** 下载并安装 JDK 21；targetDir 为空时装到应用数据目录。 */
export function installJdk(targetDir: string, mirror: boolean) {
  return invoke<string>("install_jdk_command", { targetDir, mirror });
}

/** 下载 cmdline-tools 并安装 platform-tools / android-34 / build-tools 34.0.0。 */
export function installSdk(targetDir: string, jdkHome: string, mirror: boolean) {
  return invoke<string>("install_sdk_command", { targetDir, jdkHome, mirror });
}

export function newProject(name?: string): Project {
  const now = Date.now();
  const id = `p${now.toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  const prompts: Record<string, PermissionPrompt> = {};
  for (const [key, value] of Object.entries(DEFAULT_PERMISSION_PROMPTS)) {
    prompts[key] = { ...value };
  }
  const projectName = name?.trim() || "新建项目";
  return {
    id,
    name: projectName,
    outputDir: null,
    target: "android",
    android: { ...DEFAULT_ANDROID, appName: projectName, permissionPrompts: prompts },
    windows: {
      ...DEFAULT_WINDOWS,
      localServer: {
        ...DEFAULT_LOCAL_SERVER,
        proxies: [{ ...DEFAULT_PROXY_RULE }],
      },
    },
    createdAt: now,
    updatedAt: now,
    builds: [],
  };
}
