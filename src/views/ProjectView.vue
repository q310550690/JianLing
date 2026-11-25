<script setup lang="ts">
import { computed, inject, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import { ArrowLeft, FolderOpened } from "@element-plus/icons-vue";
import ProxyRuleRow from "../components/ProxyRuleRow.vue";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  buildAndroid,
  buildWindows,
  DEFAULT_ANDROID,
  DEFAULT_LOCAL_SERVER,
  DEFAULT_PERMISSION_PROMPTS,
  DEFAULT_PROXY_RULE,
  DEFAULT_SPLASH,
  DEFAULT_WINDOWS,
  DEFAULT_WINDOWS_PERMISSIONS,
  newProject,
  normalizeBasePath,
  loadSettings,
  proxyHeadersError,
  readBuildLog,
  saveSettings,
} from "../api";
import type {
  BuildRecord,
  BuildResult,
  LocalServerConfig,
  PermissionPrompt,
  Project,
  ProxyRule,
  Settings,
  SplashConfig,
  WindowsBuildResult,
  WindowsProfile,
} from "../api";

const props = defineProps<{ projectId: string }>();
const router = useRouter();

function goBack() {
  router.push({ name: "projects" });
}

/** 设备调试页：不需要先打调试包，正式包开了 WebView 调试也能连 */
function openDebugPage() {
  router.push({ name: "project-debug", params: { id: props.projectId } });
}

const resetScroll = inject<() => void>("resetScroll", () => {});

interface FormState {
  appName: string;
  packageName: string;
  versionName: string;
  versionCode: number;
  sourceKind: "remote" | "local";
  startUrl: string;
  startQuery: string;
  localDir: string;
  fullscreen: boolean;
  statusBarColor: string;
  edgeToEdge: boolean;
  statusBarCssVar: string;
  hideNavigationBar: boolean;
  screenOrientation: "auto" | "landscape" | "portrait";
  ignoreSslErrors: boolean;
  allowCleartext: boolean;
  permissions: string[];
  iconPath: string;
  iconBackgroundColor: string;
  keystorePath: string;
  keystoreAlias: string;
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
  openExternalInBrowser: boolean;
  cacheMode: "default" | "no_cache" | "cache_first";
  uaMode: "append" | "override";
  userAgent: string;
  pullToRefresh: boolean;
  exitOnBackPressed: boolean;
  disableBackExit: boolean;
  progressBar: boolean;
  progressColor: string;
  progressHeight: number;
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

function clonePrompts(
  source: Record<string, PermissionPrompt> | undefined,
): Record<string, PermissionPrompt> {
  const out: Record<string, PermissionPrompt> = {};
  for (const [key, value] of Object.entries(DEFAULT_PERMISSION_PROMPTS)) {
    out[key] = { ...value, ...(source?.[key] ?? {}) };
  }
  return out;
}

function emptyForm(): FormState {
  return {
    appName: "",
    packageName: DEFAULT_ANDROID.packageName,
    versionName: DEFAULT_ANDROID.versionName,
    versionCode: DEFAULT_ANDROID.versionCode,
    sourceKind: DEFAULT_ANDROID.sourceKind,
    startUrl: "",
    startQuery: "",
    localDir: "",
    fullscreen: DEFAULT_ANDROID.fullscreen,
    statusBarColor: DEFAULT_ANDROID.statusBarColor,
    edgeToEdge: DEFAULT_ANDROID.edgeToEdge,
    statusBarCssVar: DEFAULT_ANDROID.statusBarCssVar,
    hideNavigationBar: DEFAULT_ANDROID.hideNavigationBar,
    screenOrientation: DEFAULT_ANDROID.screenOrientation,
    ignoreSslErrors: DEFAULT_ANDROID.ignoreSslErrors,
    allowCleartext: DEFAULT_ANDROID.allowCleartext,
    permissions: [...DEFAULT_ANDROID.permissions],
    iconPath: "",
    iconBackgroundColor: DEFAULT_ANDROID.iconBackgroundColor,
    keystorePath: "",
    keystoreAlias: "",
    textZoom: DEFAULT_ANDROID.textZoom,
    allowAutoplay: DEFAULT_ANDROID.allowAutoplay,
    allowZoom: DEFAULT_ANDROID.allowZoom,
    disableLongPress: DEFAULT_ANDROID.disableLongPress,
    hideScrollbars: DEFAULT_ANDROID.hideScrollbars,
    allowThirdPartyCookies: DEFAULT_ANDROID.allowThirdPartyCookies,
    videoFullscreen: DEFAULT_ANDROID.videoFullscreen,
    debugEnabled: DEFAULT_ANDROID.debugEnabled,
    debugConsole: DEFAULT_ANDROID.debugConsole,
    openExternalInBrowser: DEFAULT_ANDROID.openExternalInBrowser,
    cacheMode: DEFAULT_ANDROID.cacheMode,
    uaMode: DEFAULT_ANDROID.uaMode,
    userAgent: DEFAULT_ANDROID.userAgent,
    pullToRefresh: DEFAULT_ANDROID.pullToRefresh,
    exitOnBackPressed: DEFAULT_ANDROID.exitOnBackPressed,
    disableBackExit: DEFAULT_ANDROID.disableBackExit,
    progressBar: DEFAULT_ANDROID.progressBar,
    progressColor: DEFAULT_ANDROID.progressColor,
    progressHeight: DEFAULT_ANDROID.progressHeight,
    navBar: DEFAULT_ANDROID.navBar,
    minSdk: DEFAULT_ANDROID.minSdk,
    targetSdk: DEFAULT_ANDROID.targetSdk,
    compileSdk: DEFAULT_ANDROID.compileSdk,
    lightStatusBar: DEFAULT_ANDROID.lightStatusBar,
    keepScreenOn: DEFAULT_ANDROID.keepScreenOn,
    secureFlag: DEFAULT_ANDROID.secureFlag,
    localServer: {
      ...DEFAULT_LOCAL_SERVER,
      proxies: [{ ...DEFAULT_PROXY_RULE }],
    },
    splash: { ...DEFAULT_SPLASH },
    permissionPrompts: clonePrompts(undefined),
  };
}

const form = ref<FormState>(emptyForm());
const outputDir = ref("");
const ready = ref(false);
const passwords = ref({ store: "", key: "" });
const settings = ref<Settings | null>(null);
const project = ref<Project | null>(null);
const building = ref(false);
const saving = ref(false);
const exporting = ref(false);
const result = ref<BuildResult | null>(null);
const winResult = ref<WindowsBuildResult | null>(null);
/** 最近一次构建的失败原因（成功后清空） */
const buildError = ref("");
const winBuildError = ref("");
const logs = ref<{ stream: string; line: string }[]>([]);
const winLogs = ref<{ stream: string; line: string }[]>([]);
const logBox = ref<HTMLElement | null>(null);
const logVisible = ref(false);
/** 历史记录日志弹窗：展示某个 build.log 的文件内容，首行是日志路径 */
const fileLogVisible = ref(false);
const fileLogPath = ref("");
const fileLogLines = ref<string[]>([]);
const fileLogLoading = ref(false);
let unlistenAndroid: UnlistenFn | null = null;
let unlistenWindows: UnlistenFn | null = null;

type WinFormState = Omit<WindowsProfile, "localDir" | "certPassword"> & {
  localDir: string;
  certPassword: string;
};

const winForm = ref<WinFormState>({
  sourceKind: DEFAULT_WINDOWS.sourceKind,
  startUrl: "",
  startQuery: "",
  localDir: "",
  sizeMode: DEFAULT_WINDOWS.sizeMode,
  width: DEFAULT_WINDOWS.width,
  height: DEFAULT_WINDOWS.height,
  aspectW: DEFAULT_WINDOWS.aspectW,
  aspectH: DEFAULT_WINDOWS.aspectH,
  resizable: DEFAULT_WINDOWS.resizable,
  frameless: DEFAULT_WINDOWS.frameless,
  alwaysOnTop: DEFAULT_WINDOWS.alwaysOnTop,
  maximized: DEFAULT_WINDOWS.maximized,
  debugEnabled: DEFAULT_WINDOWS.debugEnabled,
  singleInstance: DEFAULT_WINDOWS.singleInstance,
  autoStart: DEFAULT_WINDOWS.autoStart,
  minimizeOnClose: DEFAULT_WINDOWS.minimizeOnClose,
  closeMode: DEFAULT_WINDOWS.closeMode,
  zoom: DEFAULT_WINDOWS.zoom,
  autoplay: DEFAULT_WINDOWS.autoplay,
  blockContextMenu: DEFAULT_WINDOWS.blockContextMenu,
  blockTextSelect: DEFAULT_WINDOWS.blockTextSelect,
  hideScrollbars: DEFAULT_WINDOWS.hideScrollbars,
  blockCtrlZoom: DEFAULT_WINDOWS.blockCtrlZoom,
  blockBrowserKeys: DEFAULT_WINDOWS.blockBrowserKeys,
  externalLinksInBrowser: DEFAULT_WINDOWS.externalLinksInBrowser,
  minFontSize: DEFAULT_WINDOWS.minFontSize,
  ignoreCertErrors: DEFAULT_WINDOWS.ignoreCertErrors,
  progressBar: DEFAULT_WINDOWS.progressBar,
  navBar: DEFAULT_WINDOWS.navBar,
  progressColor: DEFAULT_WINDOWS.progressColor,
  progressHeight: DEFAULT_WINDOWS.progressHeight,
  uaMode: DEFAULT_WINDOWS.uaMode,
  userAgent: DEFAULT_WINDOWS.userAgent,
  downloadToSubfolder: DEFAULT_WINDOWS.downloadToSubfolder,
  minWidth: DEFAULT_WINDOWS.minWidth,
  minHeight: DEFAULT_WINDOWS.minHeight,
  rememberWindow: DEFAULT_WINDOWS.rememberWindow,
  localServer: {
    ...DEFAULT_LOCAL_SERVER,
    proxies: [{ ...DEFAULT_PROXY_RULE }],
  },
  splash: { ...DEFAULT_SPLASH },
  permissions: { ...DEFAULT_WINDOWS_PERMISSIONS },
  outputSingle: DEFAULT_WINDOWS.outputSingle,
  outputInstaller: DEFAULT_WINDOWS.outputInstaller,
  outputPortable: DEFAULT_WINDOWS.outputPortable,
  installScope: DEFAULT_WINDOWS.installScope,
  certPath: null,
  certPassword: "",
  kernelId: null,
});

function addProxyRule() {
  winForm.value.localServer.proxies.push({ ...DEFAULT_PROXY_RULE, prefix: "", target: "" });
}

function removeProxyRule(index: number) {
  winForm.value.localServer.proxies.splice(index, 1);
}

function addAndroidProxyRule() {
  form.value.localServer.proxies.push({ ...DEFAULT_PROXY_RULE, prefix: "", target: "" });
}

function removeAndroidProxyRule(index: number) {
  form.value.localServer.proxies.splice(index, 1);
}

const winProxyRules = computed(() => winForm.value.localServer.proxies);
const androidProxyRules = computed(() => form.value.localServer.proxies);

/** 预览用：非全屏时状态栏一定显示，只有全屏启动会连状态栏一起隐藏 */
const statusBarVisible = computed(() => !form.value.fullscreen);
/** 预览用：状态栏底色。边到边时透出下面的网页颜色（斜纹表示透明，不是纯色块） */
const previewStatusStyle = computed(() => {
  if (!statusBarVisible.value) return null;
  if (!form.value.edgeToEdge) return { background: form.value.statusBarColor };
  return { background: "repeating-linear-gradient(45deg, #d6dfea 0 4px, #e7ecf3 4px 8px)" };
});

const activeTab = ref<"android" | "windows">("android");

/** Android 配置分组：把长表单拆成 6 段，避免整页上下翻 */
type AndroidPane = "app" | "ui" | "behavior" | "permission" | "sign" | "build";
const androidTab = ref<AndroidPane>("app");
const androidPaneOptions: { label: string; value: AndroidPane }[] = [
  { label: "应用", value: "app" },
  { label: "外观", value: "ui" },
  { label: "行为", value: "behavior" },
  { label: "权限", value: "permission" },
  { label: "签名", value: "sign" },
  { label: "构建", value: "build" },
];

/** Windows 配置分组，规则与 Android 一致 */
type WinPane = "app" | "ui" | "behavior" | "permission" | "sign" | "build";
const winTab = ref<WinPane>("app");
const winPaneOptions: { label: string; value: WinPane }[] = [
  { label: "应用", value: "app" },
  { label: "外观", value: "ui" },
  { label: "行为", value: "behavior" },
  { label: "权限", value: "permission" },
  { label: "签名", value: "sign" },
  { label: "构建", value: "build" },
];

/** 尺寸方式切换时清掉矛盾的开关：固定比例不提供最大化，全屏不需要调尺寸。 */
watch(
  () => winForm.value.sizeMode,
  (mode) => {
    if (mode === "fullscreen") {
      winForm.value.maximized = false;
      winForm.value.resizable = false;
    } else if (mode === "ratio") {
      winForm.value.maximized = false;
    }
  },
);

const winPermissionRows = [
  { key: "camera", label: "相机", desc: "网页调用摄像头" },
  { key: "microphone", label: "麦克风", desc: "网页调用麦克风" },
  { key: "geolocation", label: "定位", desc: "网页读取当前位置" },
  { key: "notifications", label: "通知", desc: "网页发送桌面通知" },
  { key: "clipboard", label: "剪贴板", desc: "网页读写剪贴板" },
  { key: "downloads", label: "批量下载", desc: "网页连续下载多个文件" },
  { key: "fileReadWrite", label: "文件读写", desc: "网页读写本地目录中的文件" },
] as const;

const winPermissionStates = [
  { value: "ask", label: "系统提示" },
  { value: "allow", label: "自动允许" },
  { value: "deny", label: "自动拒绝" },
];

const zoomOptions = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

const minFontOptions = [0, 12, 14, 16, 18, 20, 24];

const permissionOptions = [
  { value: "camera", label: "相机", desc: "拍照、扫码、视频通话" },
  { value: "microphone", label: "麦克风", desc: "录音、语音通话" },
  { value: "storage", label: "存储", desc: "选文件、存图片、下载" },
  { value: "location", label: "定位", desc: "定位、附近地点" },
  { value: "notification", label: "通知", desc: "系统通知栏消息" },
  { value: "bluetooth", label: "蓝牙", desc: "连接耳机 / 手环 / 外设" },
  { value: "nfc", label: "NFC", desc: "读卡、碰一碰" },
  { value: "vibrate", label: "振动", desc: "震动反馈" },
  { value: "foreground", label: "前台服务", desc: "后台通话 / 录音保活" },
  { value: "call", label: "直接拨号", desc: "点 tel: 链接直接呼出" },
  { value: "install", label: "应用内安装", desc: "下载 APK 自更新" },
];

const MAX_LOG_LINES = 3000;

const selectedPerm = ref("camera");

const activePrompt = computed(() => {
  const map = form.value.permissionPrompts;
  if (!map[selectedPerm.value]) {
    map[selectedPerm.value] = {
      ...(DEFAULT_PERMISSION_PROMPTS[selectedPerm.value] ?? {
        enabled: true,
        title: "权限申请",
        message: "",
        allow: "允许",
        deny: "拒绝",
      }),
    };
  }
  return map[selectedPerm.value];
});

const activePermLabel = computed(
  () => permissionOptions.find((p) => p.value === selectedPerm.value)?.label ?? "",
);

const statusText = computed(() => {
  if (building.value) return "构建中…";
  if (buildError.value) return "构建失败";
  if (result.value) return result.value.signed ? "已签名" : "已生成（调试签名）";
  return "待构建";
});

const sdkReady = computed(() => Boolean(settings.value?.sdkPath));
const builds = computed(() => project.value?.builds ?? []);
const androidBuilds = computed(() =>
  builds.value.filter((item) => (item.platform ?? "android") !== "windows"),
);
const windowsBuilds = computed(() =>
  builds.value.filter((item) => item.platform === "windows"),
);

/** 构建历史分段：调试包 / 正式包 */
const historyKind = ref<"release" | "debug">("release");
const isDebugApk = (item: BuildRecord) => item.apkPath.endsWith("-debug.apk");
const androidReleaseBuilds = computed(() => androidBuilds.value.filter((i) => !isDebugApk(i)));
const androidDebugBuilds = computed(() => androidBuilds.value.filter(isDebugApk));
const visibleAndroidBuilds = computed(() =>
  historyKind.value === "debug" ? androidDebugBuilds.value : androidReleaseBuilds.value,
);
const activeLogs = computed(() =>
  activeTab.value === "android" ? logs.value : winLogs.value,
);

const winSizeText = computed(() => {
  const w = winForm.value;
  if (w.sizeMode === "fullscreen") return "全屏";
  if (w.sizeMode === "ratio") {
    const height = w.aspectW > 0 ? Math.round((w.width * w.aspectH) / w.aspectW) : 0;
    return `${w.aspectW}:${w.aspectH} · ${w.width}×${height}`;
  }
  return `${w.width}×${w.height}`;
});

/** 横竖屏由宽高比的长短边决定，切换时交换两个数，比例本身不变。 */
const winOrientation = computed<"landscape" | "portrait">({
  get: () => (winForm.value.aspectW >= winForm.value.aspectH ? "landscape" : "portrait"),
  set: (value) => {
    const w = winForm.value.aspectW;
    const h = winForm.value.aspectH;
    if (value === "portrait" && w >= h) {
      winForm.value.aspectW = h;
      winForm.value.aspectH = w;
    } else if (value === "landscape" && w < h) {
      winForm.value.aspectW = h;
      winForm.value.aspectH = w;
    }
  },
});

const winStatusText = computed(() => {
  if (exporting.value) return "构建中…";
  if (winBuildError.value) return "构建失败";
  if (winResult.value) return winResult.value.signed ? "已签名" : "已生成（未签名）";
  return "待构建";
});

async function appendBuild(record: BuildRecord) {
  const current = settings.value;
  const base = project.value;
  if (!current || !base) return;
  const updated: Project = {
    ...base,
    builds: [record, ...base.builds].slice(0, 30),
  };
  const next: Settings = {
    ...current,
    projects: current.projects.map((p) => (p.id === updated.id ? updated : p)),
  };
  await saveSettings(next);
  settings.value = next;
  project.value = updated;
}

function formatTime(ts: number) {
  if (!ts) return "—";
  const date = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

async function scrollLogs() {
  await nextTick();
  if (logBox.value) {
    logBox.value.scrollTop = logBox.value.scrollHeight;
  }
}

function pushLog(target: "android" | "windows", entry: { stream: string; line: string }) {
  const list = target === "android" ? logs : winLogs;
  list.value.push(entry);
  if (list.value.length > MAX_LOG_LINES) {
    list.value.splice(0, list.value.length - MAX_LOG_LINES);
  }
  scrollLogs();
}

function fillForm(source: Project) {
  const a = source.android;
  outputDir.value = source.outputDir ?? "";
  form.value = {
    appName: a.appName ?? "",
    packageName: a.packageName || DEFAULT_ANDROID.packageName,
    versionName: a.versionName || DEFAULT_ANDROID.versionName,
    versionCode: a.versionCode || 1,
    sourceKind: a.sourceKind === "local" ? "local" : "remote",
    startUrl: a.startUrl ?? "",
    startQuery: a.startQuery ?? "",
    localDir: a.localDir ?? "",
    fullscreen: a.fullscreen,
    statusBarColor: a.statusBarColor || DEFAULT_ANDROID.statusBarColor,
    edgeToEdge: a.edgeToEdge ?? DEFAULT_ANDROID.edgeToEdge,
    statusBarCssVar: a.statusBarCssVar ?? DEFAULT_ANDROID.statusBarCssVar,
    hideNavigationBar: a.hideNavigationBar ?? DEFAULT_ANDROID.hideNavigationBar,
    screenOrientation: a.screenOrientation ?? DEFAULT_ANDROID.screenOrientation,
    ignoreSslErrors: a.ignoreSslErrors ?? true,
    allowCleartext: a.allowCleartext,
    permissions: [...(a.permissions ?? [])],
    iconPath: a.iconPath ?? "",
    iconBackgroundColor: a.iconBackgroundColor || DEFAULT_ANDROID.iconBackgroundColor,
    keystorePath: a.keystorePath ?? "",
    keystoreAlias: a.keystoreAlias ?? "",
    textZoom: a.textZoom || DEFAULT_ANDROID.textZoom,
    allowAutoplay: a.allowAutoplay,
    allowZoom: a.allowZoom,
    disableLongPress: a.disableLongPress,
    hideScrollbars: a.hideScrollbars,
    allowThirdPartyCookies: a.allowThirdPartyCookies,
    videoFullscreen: a.videoFullscreen,
    debugEnabled: a.debugEnabled,
    debugConsole: a.debugConsole ?? DEFAULT_ANDROID.debugConsole,
    openExternalInBrowser: a.openExternalInBrowser ?? DEFAULT_ANDROID.openExternalInBrowser,
    cacheMode: a.cacheMode ?? DEFAULT_ANDROID.cacheMode,
    uaMode: a.uaMode ?? DEFAULT_ANDROID.uaMode,
    userAgent: a.userAgent ?? "",
    pullToRefresh: a.pullToRefresh ?? DEFAULT_ANDROID.pullToRefresh,
    exitOnBackPressed: a.exitOnBackPressed ?? DEFAULT_ANDROID.exitOnBackPressed,
    disableBackExit: a.disableBackExit ?? DEFAULT_ANDROID.disableBackExit,
    progressBar: a.progressBar ?? DEFAULT_ANDROID.progressBar,
    progressColor: a.progressColor || DEFAULT_ANDROID.progressColor,
    progressHeight: a.progressHeight || DEFAULT_ANDROID.progressHeight,
    navBar: a.navBar ?? DEFAULT_ANDROID.navBar,
    minSdk: a.minSdk || DEFAULT_ANDROID.minSdk,
    targetSdk: a.targetSdk || DEFAULT_ANDROID.targetSdk,
    compileSdk: a.compileSdk || DEFAULT_ANDROID.compileSdk,
    lightStatusBar: a.lightStatusBar ?? DEFAULT_ANDROID.lightStatusBar,
    keepScreenOn: a.keepScreenOn ?? DEFAULT_ANDROID.keepScreenOn,
    secureFlag: a.secureFlag ?? DEFAULT_ANDROID.secureFlag,
    localServer: {
      enabled: a.localServer?.enabled ?? DEFAULT_LOCAL_SERVER.enabled,
      port: a.localServer?.port ?? DEFAULT_LOCAL_SERVER.port,
      basePath: a.localServer?.basePath ?? DEFAULT_LOCAL_SERVER.basePath,
      proxies: (a.localServer?.proxies ?? DEFAULT_LOCAL_SERVER.proxies).map((r) => ({
        ...DEFAULT_PROXY_RULE,
        ...r,
      })),
    },
    splash: {
      ...DEFAULT_SPLASH,
      ...(a.splash ?? {}),
      mode: a.splash?.mode === "custom" ? "custom" : "default",
      backgroundImage: a.splash?.backgroundImage ?? "",
      imagePath: a.splash?.imagePath ?? "",
      logoPath: a.splash?.logoPath ?? "",
    },
    permissionPrompts: clonePrompts(a.permissionPrompts),
  };
  passwords.value = {
    store: a.keystoreStorePassword ?? "",
    key: a.keystoreKeyPassword ?? "",
  };

  const w = source.windows ?? DEFAULT_WINDOWS;
  winForm.value = {
    sourceKind: w.sourceKind === "local" ? "local" : "remote",
    startUrl: w.startUrl ?? "",
    startQuery: w.startQuery ?? "",
    localDir: w.localDir ?? "",
    sizeMode: w.sizeMode ?? "fixed",
    width: w.width || DEFAULT_WINDOWS.width,
    height: w.height || DEFAULT_WINDOWS.height,
    aspectW: w.aspectW || DEFAULT_WINDOWS.aspectW,
    aspectH: w.aspectH || DEFAULT_WINDOWS.aspectH,
    resizable: w.resizable,
    frameless: w.frameless,
    alwaysOnTop: w.alwaysOnTop,
    maximized: w.maximized ?? false,
    debugEnabled: w.debugEnabled,
    singleInstance: w.singleInstance ?? DEFAULT_WINDOWS.singleInstance,
    autoStart: w.autoStart ?? DEFAULT_WINDOWS.autoStart,
    minimizeOnClose: w.minimizeOnClose ?? DEFAULT_WINDOWS.minimizeOnClose,
    closeMode: w.closeMode ?? DEFAULT_WINDOWS.closeMode,
    zoom: w.zoom || DEFAULT_WINDOWS.zoom,
    autoplay: w.autoplay ?? DEFAULT_WINDOWS.autoplay,
    blockContextMenu: w.blockContextMenu ?? false,
    blockTextSelect: w.blockTextSelect ?? false,
    hideScrollbars: w.hideScrollbars ?? DEFAULT_WINDOWS.hideScrollbars,
    blockCtrlZoom: w.blockCtrlZoom ?? false,
    blockBrowserKeys: w.blockBrowserKeys ?? DEFAULT_WINDOWS.blockBrowserKeys,
    externalLinksInBrowser: w.externalLinksInBrowser ?? DEFAULT_WINDOWS.externalLinksInBrowser,
    minFontSize: w.minFontSize ?? DEFAULT_WINDOWS.minFontSize,
    ignoreCertErrors: w.ignoreCertErrors ?? true,
    progressBar: w.progressBar ?? DEFAULT_WINDOWS.progressBar,
    navBar: w.navBar ?? DEFAULT_WINDOWS.navBar,
    progressColor: w.progressColor || DEFAULT_WINDOWS.progressColor,
    progressHeight: w.progressHeight || DEFAULT_WINDOWS.progressHeight,
    uaMode: w.uaMode === "override" ? "override" : "append",
    userAgent: w.userAgent ?? "",
    downloadToSubfolder: w.downloadToSubfolder ?? DEFAULT_WINDOWS.downloadToSubfolder,
    minWidth: w.minWidth ?? 0,
    minHeight: w.minHeight ?? 0,
    rememberWindow: w.rememberWindow ?? false,
    localServer: {
      enabled: w.localServer?.enabled ?? DEFAULT_LOCAL_SERVER.enabled,
      port: w.localServer?.port ?? DEFAULT_LOCAL_SERVER.port,
      basePath: w.localServer?.basePath ?? DEFAULT_LOCAL_SERVER.basePath,
      proxies: (w.localServer?.proxies ?? DEFAULT_LOCAL_SERVER.proxies).map((r) => ({
        ...DEFAULT_PROXY_RULE,
        ...r,
      })),
    },
    splash: {
      ...DEFAULT_SPLASH,
      ...(w.splash ?? {}),
      mode: w.splash?.mode === "custom" ? "custom" : "default",
      backgroundImage: w.splash?.backgroundImage ?? "",
      imagePath: w.splash?.imagePath ?? "",
      logoPath: w.splash?.logoPath ?? "",
    },
    permissions: { ...DEFAULT_WINDOWS_PERMISSIONS, ...(w.permissions ?? {}) },
    outputSingle: w.outputSingle ?? DEFAULT_WINDOWS.outputSingle,
    outputInstaller: w.outputInstaller ?? DEFAULT_WINDOWS.outputInstaller,
    outputPortable: w.outputPortable ?? DEFAULT_WINDOWS.outputPortable,
    installScope: w.installScope ?? DEFAULT_WINDOWS.installScope,
    certPath: w.certPath ?? null,
    certPassword: w.certPassword ?? "",
    kernelId: w.kernelId ?? null,
  };
}

async function load() {
  ready.value = false;
  try {
    const loaded = await loadSettings();
    settings.value = loaded;
    const found = loaded.projects.find((p) => p.id === props.projectId);
    if (!found) {
      ElMessage.error("项目不存在或已被删除");
      goBack();
      return;
    }
    project.value = found;
    fillForm(found);
    ready.value = true;
  } catch (e) {
    ElMessage.error(`读取项目失败：${String(e)}`);
    goBack();
  }
}

async function persist(): Promise<Project> {
  const current = settings.value;
  if (!current) {
    throw new Error("配置尚未加载");
  }
  const outDir = outputDir.value.trim();
  const base = project.value ?? newProject(form.value.appName || "未命名项目");
  const updated: Project = {
    ...base,
    // 项目名跟着「应用名」走：改名后项目管理列表要同步，不能一直留着创建/复制时的旧名
    name: form.value.appName.trim() || base.name || "未命名项目",
    outputDir: outDir || null,
    updatedAt: Date.now(),
    android: {
      appName: form.value.appName.trim(),
      packageName: form.value.packageName.trim(),
      versionName: form.value.versionName.trim(),
      versionCode: form.value.versionCode,
      sourceKind: form.value.sourceKind,
      startUrl: form.value.startUrl.trim(),
      startQuery: form.value.startQuery.trim(),
      localDir: form.value.localDir.trim() || null,
      fullscreen: form.value.fullscreen,
      statusBarColor: form.value.statusBarColor,
      edgeToEdge: form.value.edgeToEdge,
      statusBarCssVar: form.value.statusBarCssVar,
      hideNavigationBar: form.value.hideNavigationBar,
      screenOrientation: form.value.screenOrientation,
      ignoreSslErrors: form.value.ignoreSslErrors,
      allowCleartext: form.value.allowCleartext,
      permissions: [...form.value.permissions],
      iconPath: form.value.iconPath.trim() || null,
      iconBackgroundColor: form.value.iconBackgroundColor,
      keystorePath: form.value.keystorePath.trim() || null,
      keystoreAlias: form.value.keystoreAlias.trim() || null,
      keystoreStorePassword: passwords.value.store.trim() || null,
      keystoreKeyPassword: passwords.value.key.trim() || null,
      textZoom: form.value.textZoom,
      allowAutoplay: form.value.allowAutoplay,
      allowZoom: form.value.allowZoom,
      disableLongPress: form.value.disableLongPress,
      hideScrollbars: form.value.hideScrollbars,
      allowThirdPartyCookies: form.value.allowThirdPartyCookies,
      videoFullscreen: form.value.videoFullscreen,
      debugEnabled: form.value.debugEnabled,
      debugConsole: form.value.debugConsole,
      openExternalInBrowser: form.value.openExternalInBrowser,
      cacheMode: form.value.cacheMode,
      uaMode: form.value.uaMode,
      userAgent: form.value.userAgent.trim(),
      pullToRefresh: form.value.pullToRefresh,
      exitOnBackPressed: form.value.exitOnBackPressed,
      disableBackExit: form.value.disableBackExit,
      progressBar: form.value.progressBar,
      progressColor: form.value.progressColor,
      progressHeight: form.value.progressHeight,
      navBar: form.value.navBar,
      minSdk: form.value.minSdk,
      targetSdk: form.value.targetSdk,
      compileSdk: form.value.compileSdk,
      lightStatusBar: form.value.lightStatusBar,
      keepScreenOn: form.value.keepScreenOn,
      secureFlag: form.value.secureFlag,
      localServer: {
        enabled: form.value.localServer.enabled,
        port: form.value.localServer.port,
        basePath: normalizeBasePath(form.value.localServer.basePath),
        proxies: form.value.localServer.proxies.map((r) => ({ ...r })),
      },
      splash: { ...form.value.splash },
      permissionPrompts: { ...form.value.permissionPrompts },
    },
    windows: {
      sourceKind: winForm.value.sourceKind,
      startUrl: winForm.value.startUrl.trim(),
      startQuery: winForm.value.startQuery.trim(),
      localDir: winForm.value.localDir.trim() || null,
      sizeMode: winForm.value.sizeMode,
      width: winForm.value.width,
      height: winForm.value.height,
      aspectW: winForm.value.aspectW,
      aspectH: winForm.value.aspectH,
      resizable: winForm.value.resizable,
      frameless: winForm.value.frameless,
      alwaysOnTop: winForm.value.alwaysOnTop,
      maximized: winForm.value.maximized,
      debugEnabled: winForm.value.debugEnabled,
      singleInstance: winForm.value.singleInstance,
      autoStart: winForm.value.autoStart,
      minimizeOnClose: winForm.value.closeMode === "minimize",
      closeMode: winForm.value.closeMode,
      zoom: winForm.value.zoom,
      autoplay: winForm.value.autoplay,
      blockContextMenu: winForm.value.blockContextMenu,
      blockTextSelect: winForm.value.blockTextSelect,
      hideScrollbars: winForm.value.hideScrollbars,
      blockCtrlZoom: winForm.value.blockCtrlZoom,
      blockBrowserKeys: winForm.value.blockBrowserKeys,
      externalLinksInBrowser: winForm.value.externalLinksInBrowser,
      minFontSize: winForm.value.minFontSize,
      ignoreCertErrors: winForm.value.ignoreCertErrors,
      progressBar: winForm.value.progressBar,
      navBar: winForm.value.navBar,
      progressColor: winForm.value.progressColor,
      progressHeight: winForm.value.progressHeight,
      uaMode: winForm.value.uaMode,
      userAgent: winForm.value.userAgent.trim(),
      downloadToSubfolder: winForm.value.downloadToSubfolder,
      minWidth: winForm.value.minWidth,
      minHeight: winForm.value.minHeight,
      rememberWindow: winForm.value.rememberWindow,
      localServer: {
        enabled: winForm.value.localServer.enabled,
        port: winForm.value.localServer.port,
        basePath: normalizeBasePath(winForm.value.localServer.basePath),
        proxies: winForm.value.localServer.proxies.map((r) => ({ ...r })),
      },
      splash: { ...winForm.value.splash },
      permissions: { ...winForm.value.permissions },
      outputSingle: winForm.value.outputSingle,
      outputInstaller: winForm.value.outputInstaller,
      outputPortable: winForm.value.outputPortable,
      installScope: winForm.value.installScope,
      certPath: (winForm.value.certPath ?? "").trim() || null,
      certPassword: winForm.value.certPassword.trim() || null,
      kernelId: winForm.value.kernelId ?? null,
    },
  };
  const list = current.projects.some((p) => p.id === updated.id)
    ? current.projects.map((p) => (p.id === updated.id ? updated : p))
    : [...current.projects, updated];
  const next: Settings = { ...current, projects: list };
  await saveSettings(next);
  settings.value = next;
  project.value = updated;
  return updated;
}

async function saveOnly() {
  saving.value = true;
  try {
    await persist();
    ElMessage.success("项目已保存");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    saving.value = false;
  }
}

function togglePerm(value: string) {
  const list = form.value.permissions;
  const index = list.indexOf(value);
  if (index >= 0) {
    list.splice(index, 1);
  } else {
    list.push(value);
  }
}

async function pickLocalDir() {
  const selected = await open({
    multiple: false,
    directory: true,
    title: "选择 H5 目录（根目录需含 index.html）",
  });
  if (typeof selected === "string") {
    form.value.localDir = selected;
  }
}

async function pickOutputDir() {
  const selected = await open({
    multiple: false,
    directory: true,
    title: "选择产物输出目录",
  });
  if (typeof selected === "string") {
    outputDir.value = selected;
  }
}


async function pickLocalZip() {
  const selected = await open({
    multiple: false,
    title: "选择 H5 压缩包（根目录需含 index.html）",
    filters: [{ name: "压缩包", extensions: ["zip"] }],
  });
  if (typeof selected === "string") {
    form.value.localDir = selected;
  }
}

async function pickWinDir() {
  const selected = await open({
    multiple: false,
    directory: true,
    title: "选择 H5 目录（根目录需含 index.html）",
  });
  if (typeof selected === "string") {
    winForm.value.localDir = selected;
  }
}

async function pickWinZip() {
  const selected = await open({
    multiple: false,
    title: "选择 H5 压缩包（根目录需含 index.html）",
    filters: [{ name: "压缩包", extensions: ["zip"] }],
  });
  if (typeof selected === "string") {
    winForm.value.localDir = selected;
  }
}

async function pickWinCert() {
  const selected = await open({
    multiple: false,
    title: "选择代码签名证书",
    filters: [{ name: "签名证书", extensions: ["pfx", "p12"] }],
  });
  if (typeof selected === "string") {
    winForm.value.certPath = selected;
  }
}

function setAspect(w: number, h: number) {
  winForm.value.aspectW = w;
  winForm.value.aspectH = h;
}

async function pickIcon() {
  const selected = await open({
    multiple: false,
    title: "选择应用图标",
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  if (typeof selected === "string") {
    form.value.iconPath = selected;
  }
}

async function pickKeystore() {
  const selected = await open({
    multiple: false,
    title: "选择签名证书",
    filters: [{ name: "证书文件", extensions: ["jks", "keystore", "p12"] }],
  });
  if (typeof selected === "string") {
    form.value.keystorePath = selected;
  }
}

function splashOf(target: "android" | "windows") {
  return target === "android" ? form.value.splash : winForm.value.splash;
}

function setIconColor(value: string | null) {
  form.value.iconBackgroundColor = value || DEFAULT_ANDROID.iconBackgroundColor;
}

function setSplashColor(target: "android" | "windows", value: string | null) {
  splashOf(target).backgroundColor = value || "#FFFFFF";
}

async function pickSplashImage(target: "android" | "windows") {
  const selected = await open({
    multiple: false,
    title: "选择启动屏整屏图片",
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  if (typeof selected === "string") {
    splashOf(target).imagePath = selected;
  }
}

async function pickSplashBackground(target: "android" | "windows") {
  const selected = await open({
    multiple: false,
    title: "选择启动屏背景图",
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  if (typeof selected === "string") {
    splashOf(target).backgroundImage = selected;
  }
}

async function pickSplashLogo(target: "android" | "windows") {
  const selected = await open({
    multiple: false,
    title: "选择启动屏 Logo",
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "svg"] }],
  });
  if (typeof selected === "string") {
    splashOf(target).logoPath = selected;
  }
}

/** 构建前统一校验转发规则的追加 header，返回第一条错误说明（含前缀），空串表示都没问题。 */
function firstProxyHeadersError(rules: ProxyRule[]): string {
  for (const rule of rules) {
    const err = proxyHeadersError(rule.headers);
    if (err) {
      return `转发规则「${rule.prefix || "未填前缀"}」的追加 header：${err}`;
    }
  }
  return "";
}

async function startBuild(debugBuild = false) {
  if (building.value) return;
  if (!outputDir.value.trim()) {
    ElMessage.error("请先选择产物输出目录再构建");
    return;
  }
  const headersError = firstProxyHeadersError(form.value.localServer.proxies);
  if (headersError) {
    ElMessage.error(headersError);
    return;
  }
  building.value = true;
  result.value = null;
  logs.value = [];
  buildError.value = "";
  // 构建参数与结果都在「构建」分组里，切过去顺手把历史分段对齐
  androidTab.value = "build";
  historyKind.value = debugBuild ? "debug" : "release";
  const startedAt = Date.now();
  try {
    const saved = await persist();
    const current = settings.value;
    result.value = await buildAndroid({
      ...saved.android,
      projectId: props.projectId,
      keystoreStorePassword: passwords.value.store.trim() || null,
      keystoreKeyPassword: passwords.value.key.trim() || null,
      sdkPath: current?.sdkPath ?? null,
      jdkPath: current?.jdkPath ?? null,
      outputDir: saved.outputDir ?? null,
      debugBuild,
    });
    const built = result.value;
    if (built) {
      await appendBuild({
        id: `b${Date.now().toString(36)}`,
        buildId: built.buildId,
        apkPath: built.apkPath,
        sizeBytes: built.sizeBytes,
        signed: built.signed,
        versionName: saved.android.versionName,
        versionCode: saved.android.versionCode,
        durationSecs: built.durationSecs,
        createdAt: Date.now(),
        logPath: built.logPath,
        platform: "android",
        failed: false,
        error: "",
      });
    }
    ElMessage.success("构建完成");
    if (debugBuild) {
      // 调试包的下一步就是装到设备上看日志，直接进调试页
      router.push({ name: "project-debug", params: { id: props.projectId } });
    }
  } catch (e) {
    const message = String(e);
    buildError.value = message;
    ElMessage.error(message);
    await appendBuild({
      id: `b${Date.now().toString(36)}`,
      apkPath: "",
      sizeBytes: 0,
      signed: false,
      versionName: form.value.versionName,
      versionCode: form.value.versionCode,
      durationSecs: Math.max(1, Math.round((Date.now() - startedAt) / 1000)),
      createdAt: Date.now(),
      logPath: logPathFromError(message),
      platform: "android",
      failed: true,
      error: message,
    });
  } finally {
    building.value = false;
  }
}

function clearLogs() {
  if (activeTab.value === "android") {
    logs.value = [];
  } else {
    winLogs.value = [];
  }
}

async function startWindowsBuild() {
  if (exporting.value) return;
  if (!outputDir.value.trim()) {
    ElMessage.error("请先选择产物输出目录再构建");
    return;
  }
  const headersError = firstProxyHeadersError(winForm.value.localServer.proxies);
  if (headersError) {
    ElMessage.error(headersError);
    return;
  }
  exporting.value = true;
  winResult.value = null;
  winLogs.value = [];
  winBuildError.value = "";
  // 构建参数与结果在「构建」分组里，切过去顺手对齐
  activeTab.value = "windows";
  winTab.value = "build";
  const startedAt = Date.now();
  try {
    const saved = await persist();
    const built = await buildWindows({
      projectId: props.projectId,
      appName: saved.android.appName,
      versionName: saved.android.versionName,
      outputDir: saved.outputDir ?? null,
      sourceKind: winForm.value.sourceKind,
      startUrl: winForm.value.startUrl.trim(),
      startQuery: winForm.value.startQuery.trim(),
      localDir: winForm.value.localDir.trim() || null,
      iconPath: saved.android.iconPath ?? null,
      sizeMode: winForm.value.sizeMode,
      width: winForm.value.width,
      height: winForm.value.height,
      aspectW: winForm.value.aspectW,
      aspectH: winForm.value.aspectH,
      resizable: winForm.value.resizable,
      frameless: winForm.value.frameless,
      alwaysOnTop: winForm.value.alwaysOnTop,
      maximized: winForm.value.maximized,
      debugEnabled: winForm.value.debugEnabled,
      singleInstance: winForm.value.singleInstance,
      autoStart: winForm.value.autoStart,
      minimizeOnClose: winForm.value.closeMode === "minimize",
      closeMode: winForm.value.closeMode,
      zoom: winForm.value.zoom,
      autoplay: winForm.value.autoplay,
      blockContextMenu: winForm.value.blockContextMenu,
      blockTextSelect: winForm.value.blockTextSelect,
      hideScrollbars: winForm.value.hideScrollbars,
      blockCtrlZoom: winForm.value.blockCtrlZoom,
      blockBrowserKeys: winForm.value.blockBrowserKeys,
      externalLinksInBrowser: winForm.value.externalLinksInBrowser,
      minFontSize: winForm.value.minFontSize,
      ignoreCertErrors: winForm.value.ignoreCertErrors,
      progressBar: winForm.value.progressBar,
      navBar: winForm.value.navBar,
      progressColor: winForm.value.progressColor,
      progressHeight: winForm.value.progressHeight,
      uaMode: winForm.value.uaMode,
      userAgent: winForm.value.userAgent.trim(),
      downloadToSubfolder: winForm.value.downloadToSubfolder,
      minWidth: winForm.value.minWidth,
      minHeight: winForm.value.minHeight,
      rememberWindow: winForm.value.rememberWindow,
      localServer: {
        enabled: winForm.value.localServer.enabled,
        port: winForm.value.localServer.port,
        basePath: normalizeBasePath(winForm.value.localServer.basePath),
        proxies: winForm.value.localServer.proxies.map((r) => ({ ...r })),
      },
      splash: { ...winForm.value.splash },
      permissions: { ...winForm.value.permissions },
      outputSingle: winForm.value.outputSingle,
      outputInstaller: winForm.value.outputInstaller,
      outputPortable: winForm.value.outputPortable,
      installScope: winForm.value.installScope,
      certPath: (winForm.value.certPath ?? "").trim() || null,
      certPassword: winForm.value.certPassword.trim() || null,
      kernelPath:
        (settings.value?.windowsKernels ?? []).find((k) => k.id === winForm.value.kernelId)?.path ??
        null,
    });
    winResult.value = built;
    await appendBuild({
      id: `b${Date.now().toString(36)}`,
      buildId: built.buildId,
      apkPath: built.exePath,
      sizeBytes: built.sizeBytes,
      signed: built.signed,
      versionName: saved.android.versionName,
      versionCode: saved.android.versionCode,
      durationSecs: built.durationSecs,
      createdAt: Date.now(),
      logPath: built.logPath,
      platform: "windows",
      failed: false,
      error: "",
    });
    const deliver = [
      built.singleExe ? "单文件 exe" : null,
      built.installer ? "安装包" : null,
      built.portableZip ? "绿色版 zip" : null,
    ].filter(Boolean);
    ElMessage.success(
      deliver.length ? `已导出：${deliver.join(" / ")}` : `Windows 版已导出：${built.exePath}`,
    );
    await reveal(built.dir);
  } catch (e) {
    const message = String(e);
    winBuildError.value = message;
    ElMessage.error(message);
    await appendBuild({
      id: `b${Date.now().toString(36)}`,
      apkPath: "",
      sizeBytes: 0,
      signed: false,
      versionName: form.value.versionName,
      versionCode: form.value.versionCode,
      durationSecs: Math.max(1, Math.round((Date.now() - startedAt) / 1000)),
      createdAt: Date.now(),
      logPath: logPathFromError(message),
      platform: "windows",
      failed: true,
      error: message,
    });
  } finally {
    exporting.value = false;
  }
}

function mb(bytes: number) {
  return (bytes / 1024 / 1024).toFixed(2);
}

async function reveal(path: string) {
  try {
    await revealItemInDir(path);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

function logPathFromError(message: string): string {
  const matched = message.match(/([A-Za-z]:\\[^\s"'）)]*\.log)/);
  return matched ? matched[1] : "";
}

/** 构建记录里的「查看日志」：在应用内弹窗展示日志内容，首行显示日志路径 */
async function openBuildLog(path: string) {
  if (!path) {
    ElMessage.warning("这条记录没有日志文件");
    return;
  }
  fileLogPath.value = path;
  fileLogLines.value = [];
  fileLogVisible.value = true;
  fileLogLoading.value = true;
  try {
    const text = await readBuildLog(path);
    fileLogLines.value = text.split(/\r?\n/);
  } catch (e) {
    fileLogLines.value = [`读取日志失败：${String(e)}`];
  } finally {
    fileLogLoading.value = false;
  }
}

watch(
  () => props.projectId,
  () => {
    logs.value = [];
    winLogs.value = [];
    result.value = null;
    winResult.value = null;
    logVisible.value = false;
    passwords.value = { store: "", key: "" };
    load();
  },
);

onMounted(async () => {
  unlistenAndroid = await listen<{ stream: string; line: string }>("build-log", (event) => {
    pushLog("android", event.payload);
  });
  unlistenWindows = await listen<{ stream: string; line: string }>(
    "build-log-windows",
    (event) => {
      pushLog("windows", event.payload);
    },
  );
  await load();
  resetScroll();
  setTimeout(resetScroll, 120);
});

onUnmounted(() => {
  unlistenAndroid?.();
  unlistenWindows?.();
});
</script>

<template>
  <div v-if="ready" class="page page-frame">
    <header class="page-head">
      <div class="head-left">
        <button class="back-btn" type="button" title="返回项目管理" @click="goBack">
          <el-icon :size="15"><ArrowLeft /></el-icon>
        </button>
        <div>
          <h1>{{ form.appName || "未命名应用" }}</h1>
          <div class="head-meta">
            <span
              class="status-dot"
              :class="{
                live: building || exporting,
                failed:
                  !building &&
                  !exporting &&
                  (activeTab === 'android' ? !!buildError : !!winBuildError),
              }"
            ></span>
            <span>{{ activeTab === "android" ? statusText : winStatusText }}</span>
            <span class="dot-sep">·</span>
            <span class="mono">
              {{
                activeTab === "android"
                  ? form.packageName || "未设置包名"
                  : "Windows " + winSizeText
              }}
            </span>
            <span v-if="activeTab === 'android' && !sdkReady" class="warn-inline">
              Android SDK 未配置
            </span>
          </div>
        </div>
      </div>
      <div class="head-actions">
        <el-button :loading="saving" @click="saveOnly">保存</el-button>
        <el-button
          v-if="activeTab === 'android'"
          :disabled="!form.packageName"
          @click="openDebugPage"
        >
          调试
        </el-button>
        <el-button
          v-if="activeTab === 'android' ? building : exporting"
          @click="logVisible = true"
        >
          构建日志
        </el-button>
        <el-button v-if="activeTab === 'android'" :loading="building" @click="startBuild(true)">
          {{ building ? "构建中…" : "构建调试包" }}
        </el-button>
        <el-button
          v-if="activeTab === 'android'"
          type="primary"
          :loading="building"
          @click="startBuild(false)"
        >
          {{ building ? "构建中…" : "构建 Android" }}
        </el-button>
        <el-button v-else type="primary" :loading="exporting" @click="startWindowsBuild">
          {{ exporting ? "构建中…" : "构建 Windows" }}
        </el-button>
      </div>
    </header>

    <el-scrollbar class="page-body">
    <section class="card">
      <div class="card-head">
        <span class="card-title">基础信息</span>
        <span class="card-hint">应用名、图标与版本由各平台共用</span>
      </div>
      <div class="fields">
        <label class="field">
          <span class="field-label">应用名</span>
          <el-input v-model="form.appName" placeholder="显示在桌面图标下方" />
        </label>
        <label class="field">
          <span class="field-label">版本名</span>
          <el-input v-model="form.versionName" placeholder="1.0.0" />
        </label>
        <label class="field">
          <span class="field-label">版本号</span>
          <el-input-number
            v-model="form.versionCode"
            :min="1"
            :max="1000000"
            controls-position="right"
            class="full"
          />
        </label>
    <div class="field">
      <span class="field-label">图标背景色</span>
      <div class="input-row">
        <el-input
          v-model="form.iconBackgroundColor"
          placeholder="#0B2A5E"
          class="mono-input"
        />
        <el-color-picker
          v-model="form.iconBackgroundColor"
          @change="(v: any) => setIconColor(v)"
        />
      </div>
    </div>
    <div class="field span-2of3">
          <span class="field-label">应用图标</span>
          <div class="input-row">
            <el-input
              v-model="form.iconPath"
              placeholder="建议 1024×1024 PNG，留空用默认图标"
              clearable
            />
            <el-button @click="pickIcon">选择</el-button>
          </div>
        </div>
        <div class="field span-2">
          <span class="field-label">产物输出目录（构建前必填）</span>
          <div class="input-row">
            <el-input
              v-model="outputDir"
              placeholder="选择 APK / exe 的保存目录"
              clearable
            />
            <el-button @click="pickOutputDir">选择</el-button>
          </div>
          <span class="field-note">
            Android 直接放 APK；Windows 按勾选的形态（安装包 / 单文件 / 绿色版）直接写进这个目录。
            只保存项目时可以不填，点构建才会检查。
          </span>
        </div>
      </div>
    </section>

    <section class="card platform-card">
      <div class="platform-head">
        <div class="seg">
          <button
            type="button"
            class="seg-item"
            :class="{ on: activeTab === 'android' }"
            @click="activeTab = 'android'"
          >
            <span class="seg-dot"></span>
            Android
          </button>
          <button
            type="button"
            class="seg-item"
            :class="{ on: activeTab === 'windows' }"
            @click="activeTab = 'windows'"
          >
            <span class="seg-dot"></span>
            Windows
          </button>
        </div>
        <span class="platform-hint">
          {{ activeTab === "android" ? "Android 专用配置" : "Windows 专用配置" }}
        </span>
      </div>

      <div v-show="activeTab === 'android'" class="pane-tabs-wrap">
        <el-segmented v-model="androidTab" block class="pane-seg" :options="androidPaneOptions" />
      </div>

      <div v-show="activeTab === 'android'" class="platform-body">
    <section v-show="androidTab === 'app'" class="card">
      <div class="card-head">
        <span class="card-title">应用信息</span>
      </div>
      <div class="fields">
        <label class="field">
          <span class="field-label">包名</span>
          <el-input v-model="form.packageName" placeholder="com.example.app" class="mono-input" />
        </label>
        <div class="field span-2">
          <span class="field-label">内容来源</span>
          <el-radio-group v-model="form.sourceKind">
            <el-radio-button value="remote">远程网址</el-radio-button>
            <el-radio-button value="local">本地静态包</el-radio-button>
          </el-radio-group>
        </div>
        <div v-if="form.sourceKind === 'remote'" class="field span-2">
          <span class="field-label">H5 首页地址</span>
          <el-input v-model="form.startUrl" placeholder="https://example.com/" />
        </div>
        <div v-else class="field span-2">
          <span class="field-label">本地 H5 路径</span>
          <div class="input-row">
            <el-input
              v-model="form.localDir"
              placeholder="含 index.html 的目录，或 .zip 压缩包"
              clearable
            />
            <el-button @click="pickLocalDir">选目录</el-button>
            <el-button @click="pickLocalZip">选 zip</el-button>
          </div>
          <span class="field-note">
            打包时整个目录会放进 APK 的 assets/www，由内置本地服务以 http://127.0.0.1 托管。
          </span>
        </div>
        <div class="field span-2">
          <span class="field-label">启动参数（可选）</span>
          <el-input
            v-model="form.startQuery"
            placeholder="如 key=value（会追加到启动地址）"
            clearable
          />
          <span class="field-note">
            按 URL 参数区分版本/环境的场景用：会拼成 …/index.html?key=value，
            页面重新加载或重启应用后仍带上。
          </span>
        </div>
      </div>
    </section>

    <section v-show="androidTab === 'app'" v-if="form.sourceKind === 'local'" class="card">
      <div class="card-head">
        <span class="card-title">本地服务</span>
        <span class="card-hint">页面与接口同源，等价 vite proxy</span>
      </div>
      <div class="local-server-top">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">启用本地服务</div>
            <div class="toggle-desc">
              以 http://127.0.0.1 托管 APK 内的页面，/api 等相对路径可直接转发到后端
            </div>
          </div>
          <el-switch v-model="form.localServer.enabled" />
        </div>
        <label v-if="form.localServer.enabled" class="field">
          <span class="field-label">Base 地址</span>
          <el-input v-model="form.localServer.basePath" placeholder="/" clearable />
          <span class="field-note">
            页面部署在域名子目录时填 /h5/（此时访问 http://www.a.com/h5/），根目录保持 /。
            填了前缀后，形如 /h5/assets/app.js 的请求会映射到包内的 assets/app.js。
          </span>
        </label>
      </div>
      <div v-if="form.localServer.enabled" class="proxy-block">
        <div class="proxy-head">
          <span class="field-label">转发规则</span>
          <el-button size="small" @click="addAndroidProxyRule">添加规则</el-button>
        </div>
        <ProxyRuleRow
          v-for="(rule, index) in androidProxyRules"
          :key="index"
          :rule="rule"
          :index="index"
          @remove="removeAndroidProxyRule(index)"
        />
        <p class="card-note">
          前缀匹配（最长优先），命中后保留原路径转发到目标地址，http(s) 与 ws(s) 都支持。
          规则会写进 APK，改了要重新打包；后端若校验 Origin，保持「改写 Origin」开启即可。
        </p>
        <p class="card-note">
          前端接入：壳会在页面脚本执行前注入 <code>window.JianLing</code>，用
          <code>typeof window.JianLing !== 'undefined'</code> 判定是否在壳内；命中后接口走相对路径
          <code>/api</code>、ws 走同源 <code>ws://127.0.0.1:端口/ws</code>，由上面的规则转发到真实地址。
        </p>
      </div>
    </section>

    <section v-show="androidTab === 'ui'" class="card">
      <div class="card-head">
        <span class="card-title">启动屏</span>
        <span class="card-hint">服务就绪前的第一屏</span>
      </div>
      <div class="toggles toggles-one">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">启用启动屏</div>
            <div class="toggle-desc">先显示静态页面，到点后再切到真正的网页</div>
          </div>
          <el-switch v-model="form.splash.enabled" />
        </div>
      </div>
      <div v-if="form.splash.enabled" class="fields spaced">
        <div class="field span-2">
          <span class="field-label">启动屏样式</span>
          <el-radio-group v-model="form.splash.mode">
            <el-radio-button value="default">默认</el-radio-button>
            <el-radio-button value="custom">自定义</el-radio-button>
          </el-radio-group>
          <span class="field-note">
            默认：背景 + 名称 + Logo；自定义：只用一张整屏图片。
          </span>
        </div>
      </div>
      <div v-if="form.splash.enabled && form.splash.mode !== 'custom'" class="fields spaced">
        <label class="field">
          <span class="field-label">显示名称</span>
          <el-input v-model="form.splash.title" placeholder="留空则用应用名" clearable />
        </label>
        <div class="field">
          <span class="field-label">背景色</span>
          <div class="input-row">
            <el-input
              v-model="form.splash.backgroundColor"
              placeholder="#FFFFFF"
              class="mono-input"
            />
            <el-color-picker
              v-model="form.splash.backgroundColor"
              @change="(v: any) => setSplashColor('android', v)"
            />
          </div>
        </div>
        <div class="field span-2">
          <span class="field-label">背景图（可选，优先于背景色）</span>
          <div class="input-row">
            <el-input
              v-model="form.splash.backgroundImage"
              placeholder="选了就用这张图铺满背景"
              clearable
            />
            <el-button @click="pickSplashBackground('android')">选择</el-button>
          </div>
        </div>
        <div class="field span-2">
          <span class="field-label">Logo（可选）</span>
          <div class="input-row">
            <el-input v-model="form.splash.logoPath" placeholder="留空则用应用图标" clearable />
            <el-button @click="pickSplashLogo('android')">选择</el-button>
          </div>
        </div>
      </div>
      <div v-if="form.splash.enabled && form.splash.mode === 'custom'" class="fields spaced">
        <div class="field span-2">
          <span class="field-label">整屏图片</span>
          <div class="input-row">
            <el-input
              v-model="form.splash.imagePath"
              placeholder="整屏铺满这张图，推荐与屏幕同比例"
              clearable
            />
            <el-button @click="pickSplashImage('android')">选择</el-button>
          </div>
        </div>
      </div>
      <p v-if="form.splash.enabled" class="card-note">
        默认样式：背景图优先于背景色；名称留空用应用名、Logo 留空用应用图标，水平居中、垂直位于屏幕 1/3 处。
        自定义样式：只显示这一张整屏图片。图片会内联进页面，不依赖本地服务。
      </p>
    </section>

    <section v-show="androidTab === 'behavior'" class="card">
      <div class="card-head">
        <span class="card-title">壳行为</span>
        <span class="card-hint">系统栏与页面行为</span>
      </div>
      <div class="toggles">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">明文 HTTP</div>
            <div class="toggle-desc">允许 http 与混合内容</div>
          </div>
          <el-switch v-model="form.allowCleartext" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">忽略证书错误</div>
            <div class="toggle-desc">
              默认开启：放行自签名 / 不受信任的 https 证书，内网自签才需要；公开站点请关闭
            </div>
          </div>
          <el-switch v-model="form.ignoreSslErrors" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">返回键双击退出</div>
            <div class="toggle-desc">无法后退时提示「再按一次退出」</div>
          </div>
          <el-switch v-model="form.exitOnBackPressed" :disabled="form.disableBackExit" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁止返回键退出应用</div>
            <div class="toggle-desc">
              开启后返回键只做网页后退，退不动时按返回键不会退出应用（上面的双击退出失效）
            </div>
          </div>
          <el-switch v-model="form.disableBackExit" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">屏幕常亮</div>
            <div class="toggle-desc">使用期间不自动息屏</div>
          </div>
          <el-switch v-model="form.keepScreenOn" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁止截屏</div>
            <div class="toggle-desc risk">开启后系统截图/录屏为黑屏</div>
          </div>
          <el-switch v-model="form.secureFlag" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">允许自动播放</div>
            <div class="toggle-desc">解除「必须静音才能自动播放」限制</div>
          </div>
          <el-switch v-model="form.allowAutoplay" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">允许手势缩放</div>
            <div class="toggle-desc">双指放大页面</div>
          </div>
          <el-switch v-model="form.allowZoom" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁用长按</div>
            <div class="toggle-desc">屏蔽长按选字与图片菜单</div>
          </div>
          <el-switch v-model="form.disableLongPress" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">隐藏滚动条</div>
            <div class="toggle-desc">页面不显示滚动条</div>
          </div>
          <el-switch v-model="form.hideScrollbars" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">外链用系统浏览器</div>
            <div class="toggle-desc">非本站域名的 http(s) 链接交给系统浏览器</div>
          </div>
          <el-switch v-model="form.openExternalInBrowser" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">下拉刷新</div>
            <div class="toggle-desc">页面顶部下拉重新加载</div>
          </div>
          <el-switch v-model="form.pullToRefresh" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">允许第三方 Cookie</div>
            <div class="toggle-desc">跨域 iframe 内保留登录态</div>
          </div>
          <el-switch v-model="form.allowThirdPartyCookies" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">视频全屏</div>
            <div class="toggle-desc">H5 video 可全屏播放</div>
          </div>
          <el-switch v-model="form.videoFullscreen" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">开启调试</div>
            <div class="toggle-desc risk">允许调试器连接，正式包建议关闭</div>
          </div>
          <el-switch v-model="form.debugEnabled" />
        </div>
        <div v-if="form.debugEnabled" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">调试包页内面板</div>
            <div class="toggle-desc">
              构建调试包时注入；Eruda 自带 Elements 面板可看/改 DOM
            </div>
          </div>
          <el-select v-model="form.debugConsole" class="debug-console-select">
            <el-option value="vconsole" label="vConsole" />
            <el-option value="eruda" label="Eruda" />
            <el-option value="none" label="不注入" />
          </el-select>
        </div>
      </div>
      <div class="fields spaced">
        <label class="field span-2">
          <span class="field-label">缓存模式</span>
          <el-select v-model="form.cacheMode">
            <el-option value="default" label="默认（按响应头）" />
            <el-option value="no_cache" label="不使用缓存" />
            <el-option value="cache_first" label="优先使用缓存" />
          </el-select>
        </label>
        <div class="field">
          <span class="field-label">User-Agent</span>
          <el-radio-group v-model="form.uaMode">
            <el-radio-button value="append">追加</el-radio-button>
            <el-radio-button value="override">整串替换</el-radio-button>
          </el-radio-group>
        </div>
        <label class="field span-2of3">
          <span class="field-label">
            {{ form.uaMode === "override" ? "自定义 UA" : "UA 附加信息（可留空）" }}
          </span>
          <el-input
            v-model="form.userAgent"
            :placeholder="form.uaMode === 'override' ? 'Mozilla/5.0 (Linux; Android 14) ...' : '加在系统 UA 末尾的标记'"
            clearable
          />
        </label>
      </div>
    </section>

    <section v-show="androidTab === 'ui'" class="card">
      <div class="card-head">
        <span class="card-title">页面</span>
        <span class="card-hint">屏幕方向、字号、进度条与状态栏</span>
      </div>
      <div class="fields spaced">
        <div class="field span-2">
          <span class="field-label">屏幕方向</span>
          <el-radio-group v-model="form.screenOrientation">
            <el-radio-button value="auto">自动横竖屏</el-radio-button>
            <el-radio-button value="landscape">强制横屏</el-radio-button>
            <el-radio-button value="portrait">强制竖屏</el-radio-button>
          </el-radio-group>
          <span class="field-note">
            自动跟随系统旋转设置；强制则锁定方向，设备旋转不生效。
          </span>
        </div>
      </div>
      <div class="fields spaced">
        <div class="field span-2">
          <span class="field-label">文字缩放 {{ form.textZoom }}%</span>
          <el-slider v-model="form.textZoom" :min="50" :max="200" :step="5" />
          <span class="field-note">100% 跟随系统设置，调大可解决「字太小看不清」。</span>
        </div>
      </div>
      <div class="toggles spaced">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">加载进度条</div>
            <div class="toggle-desc">{{ form.progressBar ? "页面加载时在顶部显示" : "已关闭" }}</div>
          </div>
          <el-switch v-model="form.progressBar" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">顶部操作栏</div>
            <div class="toggle-desc">{{ form.navBar ? "显示前进 / 后退 / 刷新" : "已关闭" }}</div>
          </div>
          <el-switch v-model="form.navBar" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">全屏启动</div>
            <div class="toggle-desc">隐藏状态栏与导航栏</div>
          </div>
          <el-switch v-model="form.fullscreen" />
        </div>
        <div v-if="!form.fullscreen" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">隐藏导航栏</div>
            <div class="toggle-desc">保留状态栏，底部上滑临时呼出</div>
          </div>
          <el-switch v-model="form.hideNavigationBar" />
        </div>
        <div v-if="!form.fullscreen" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">状态栏透明（边到边）</div>
            <div class="toggle-desc">
              开启后状态栏透明，内容从屏幕最顶端开始；关闭后用「状态栏颜色」画一条实色状态栏
            </div>
          </div>
          <el-switch v-model="form.edgeToEdge" />
        </div>
        <div v-if="!form.fullscreen" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">状态栏深色图标</div>
            <div class="toggle-desc">
              页面顶部是浅色时开启；边到边时压的是网页顶部颜色，深色页面要关闭
            </div>
          </div>
          <el-switch v-model="form.lightStatusBar" />
        </div>
      </div>
      <div v-if="!form.fullscreen && !form.edgeToEdge" class="fields spaced">
        <div class="field span-2">
          <span class="field-label">状态栏颜色</span>
          <div class="input-row">
            <el-input v-model="form.statusBarColor" placeholder="#1F2430" class="mono-input" />
            <el-color-picker v-model="form.statusBarColor" />
          </div>
          <span class="field-note">非边到边时用来绘制状态栏底色</span>
        </div>
      </div>
      <div v-if="form.progressBar" class="fields spaced">
        <label class="field">
          <span class="field-label">进度条颜色</span>
          <div class="input-row">
            <el-input v-model="form.progressColor" placeholder="#409EFF" class="mono-input" />
            <el-color-picker v-model="form.progressColor" />
          </div>
        </label>
        <label class="field">
          <span class="field-label">高度（dp）</span>
          <el-input-number
            v-model="form.progressHeight"
            :min="1"
            :max="12"
            :step="1"
            controls-position="right"
            class="full"
          />
        </label>
      </div>
      <div class="fields spaced">
        <div v-if="!form.fullscreen && form.edgeToEdge" class="field span-2">
          <span class="field-label">状态栏高度变量名</span>
          <el-input v-model="form.statusBarCssVar" placeholder="--status-bar-height" />
          <span class="field-note">
            壳把当前状态栏高度写进这个 CSS 变量，页面用 <code>var(变量名)</code> 自己留白。
            默认 <code>--status-bar-height</code> 与 uni-app 同名（会覆盖它自己的值）；换其它框架时改成该框架的变量名，留空则回到默认。
          </span>
        </div>
        <div class="field span-2">
          <span class="field-label">预览</span>
          <div class="top-preview">
            <div v-if="statusBarVisible" class="top-preview-status" :style="previewStatusStyle">
              <span :style="{ color: form.lightStatusBar ? '#1f2329' : '#ffffff' }">9:41</span>
              <span :style="{ color: form.lightStatusBar ? '#1f2329' : '#ffffff' }">▮▮ 5G ▮</span>
            </div>
            <div v-if="form.navBar" class="preview-nav">
              <span class="preview-nav-btn">
                <svg viewBox="0 0 24 24" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M15 5L9 12L15 19" /></svg>
              </span>
              <span class="preview-nav-btn">
                <svg viewBox="0 0 24 24" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 5L15 12L9 19" /></svg>
              </span>
              <span class="preview-nav-btn">
                <svg viewBox="0 0 24 24" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.67981 13L3.67981 11.3333C3.67981 6.73096 7.4402 3 12.0789 3C15.1178 3 17.7799 4.60136 19.2545 7M2 11.3333L3.67981 13L5.35962 11.3333" /><path d="M20.3139 11V12.6667C20.3139 17.269 16.5391 21 11.8827 21C8.83213 21 6.15995 19.3986 4.67969 17M22.0001 12.6667L20.3139 11L18.6277 12.6667" /></svg>
              </span>
            </div>
            <span
              v-if="form.progressBar"
              class="top-preview-progress"
              :style="{ background: form.progressColor, height: form.progressHeight + 'px' }"
            ></span>
            <div class="top-preview-page">网页内容</div>
          </div>
          <span v-if="!form.fullscreen && form.edgeToEdge" class="field-note">
            壳会把状态栏高度写进页面 CSS 变量
            <code>{{ form.statusBarCssVar || '--status-bar-height' }}</code>（单位 dp，如 24px），页面用
            <code>var({{ form.statusBarCssVar || '--status-bar-height' }})</code>
            给自己留白：边到边时是真实高度；关闭边到边或开启全屏启动时不注入。底部导航栏 / 键盘高度由壳补回。
          </span>
        </div>
      </div>
    </section>

    <section v-show="androidTab === 'permission'" class="card">
      <div class="card-head">
        <span class="card-title">套壳权限</span>
        <span class="card-hint">左侧勾选要申请的权限；右侧开关控制申请时是否先在顶部显示用途说明</span>
      </div>

      <div class="perm-layout">
        <div class="perm-side">
          <div class="perm-side-list">
            <button
              v-for="p in permissionOptions"
              :key="p.value"
              type="button"
              class="perm-item"
              :class="{ active: selectedPerm === p.value }"
              @click="selectedPerm = p.value"
            >
              <span
                class="perm-check"
                :class="{ on: form.permissions.includes(p.value) }"
                @click.stop="togglePerm(p.value)"
              ></span>
              <span class="perm-side-text">
                <span class="perm-name">{{ p.label }}</span>
                <span class="perm-desc">{{ p.desc }}</span>
              </span>
            </button>
          </div>
        </div>

        <div class="perm-detail">
          <div class="detail-head">
            <div>
              <div class="detail-title">{{ activePermLabel }} 用途说明</div>
              <div class="detail-sub">
                {{
                  !form.permissions.includes(selectedPerm)
                    ? "该权限未勾选，不会写入安装包，申请时也不会出现说明"
                    : activePrompt.enabled
                      ? "开启：申请该权限时，先在页面顶部显示这段说明"
                      : "关闭：申请该权限时不显示说明，直接弹系统授权框"
                }}
              </div>
            </div>
            <el-switch v-model="activePrompt.enabled" />
          </div>

          <div class="fields">
            <label class="field span-2">
              <span class="field-label">说明标题</span>
              <el-input v-model="activePrompt.title" :disabled="!activePrompt.enabled" />
            </label>
            <label class="field span-2">
              <span class="field-label">说明文案</span>
              <el-input
                v-model="activePrompt.message"
                type="textarea"
                :rows="4"
                resize="none"
                :disabled="!activePrompt.enabled"
                placeholder="例如：用于拍摄照片、扫码或录制视频，仅在相关功能使用时访问摄像头。"
              />
            </label>
          </div>

          <p class="detail-note">
            申请系统权限时，在页面顶部显示这段用途说明；是否授权由系统弹窗决定（不含自定义允许 / 拒绝按钮）。
          </p>
        </div>
      </div>
    </section>

    <section v-show="androidTab === 'sign'" class="card">
      <div class="card-head">
        <span class="card-title">签名</span>
      </div>
      <div class="fields">
        <div class="field span-2">
          <span class="field-label">签名证书</span>
          <div class="input-row">
            <el-input
              v-model="form.keystorePath"
              placeholder="选择 .jks 证书，留空自动生成调试签名"
              clearable
            />
            <el-button @click="pickKeystore">选择</el-button>
          </div>
        </div>
      </div>

      <div class="sign-grid spaced">
        <label class="field">
          <span class="field-label">证书别名</span>
          <el-input v-model="form.keystoreAlias" placeholder="key0" />
        </label>
        <label class="field">
          <span class="field-label">证书密码</span>
          <el-input v-model="passwords.store" type="password" show-password />
        </label>
        <label class="field">
          <span class="field-label">密钥密码</span>
          <el-input v-model="passwords.key" type="password" show-password />
        </label>
      </div>

      <p class="card-note">
        证书别名与密码随项目保存，密码用 Windows DPAPI 加密，仅当前用户可解密；换机器或换系统用户需重新填写。
      </p>
    </section>

    <section v-show="androidTab === 'build'" class="card">
      <div class="card-head">
        <span class="card-title">构建版本</span>
        <span class="card-hint">决定可安装的最低系统与上架要求</span>
      </div>
      <div class="fields">
        <label class="field">
          <span class="field-label">minSdk</span>
          <el-input-number
            v-model="form.minSdk"
            :min="21"
            :max="34"
            :step="1"
            controls-position="right"
            class="full"
          />
          <span class="field-note">允许安装的最低 Android 版本</span>
        </label>
        <label class="field">
          <span class="field-label">targetSdk</span>
          <el-input-number
            v-model="form.targetSdk"
            :min="21"
            :max="36"
            :step="1"
            controls-position="right"
            class="full"
          />
          <span class="field-note">建议与 compileSdk 一致</span>
        </label>
        <label class="field">
          <span class="field-label">compileSdk</span>
          <el-input-number
            v-model="form.compileSdk"
            :min="30"
            :max="36"
            :step="1"
            controls-position="right"
            class="full"
          />
          <span class="field-note">编译用 SDK，需本机已安装</span>
        </label>
      </div>
    </section>

    <section v-show="androidTab === 'build'" class="card" :class="{ 'card-failed': !!buildError }">
      <div class="card-head">
        <span class="card-title">
          构建 APK
          <span v-if="buildError" class="failed-tag">构建失败</span>
          <span v-else-if="building" class="running-tag">构建中…</span>
        </span>
        <div class="build-actions">
          <el-button v-if="building || logs.length" @click="logVisible = true">
            构建日志
            <span v-if="logs.length" class="log-count">{{ logs.length }}</span>
          </el-button>
        </div>
      </div>

      <div v-if="buildError" class="build-error">
        <div class="build-error-head">
          <span class="build-error-title">构建失败</span>
          <el-button size="small" @click="logVisible = true">查看日志</el-button>
        </div>
        <div class="build-error-msg mono">{{ buildError }}</div>
      </div>

      <div v-if="result" class="result">
        <div class="stats">
          <div class="stat">
            <span class="k">大小</span>
            <span class="v mono">{{ mb(result.sizeBytes) }} MB</span>
          </div>
          <div class="stat">
            <span class="k">签名</span>
            <span class="v" :class="result.signed ? 'ok' : 'warn'">
              {{ result.signed ? "正式签名" : "调试签名" }}
            </span>
          </div>
          <div class="stat">
            <span class="k">耗时</span>
            <span class="v mono">{{ result.durationSecs }}s</span>
          </div>
          <div class="stat artifact">
            <span class="k">产物</span>
            <div class="artifact-row">
              <span class="artifact-path mono" :title="result.apkPath">{{ result.apkPath }}</span>
              <el-button size="small" :icon="FolderOpened" @click="reveal(result.apkPath)">
                打开
              </el-button>
            </div>
          </div>
        </div>
      </div>

      <p v-else-if="!building" class="build-hint">
        点右上角「构建 Android」生成 APK，过程日志在「构建日志」里查看。
      </p>
      <p v-else class="build-hint live">正在构建，日志实时输出到「构建日志」。</p>
    </section>

    <section v-show="androidTab === 'build'" class="card">
      <div class="card-head">
        <span class="card-title">构建历史</span>
        <el-segmented
          v-model="historyKind"
          class="hist-seg"
          :options="[
            { label: `正式包 ${androidReleaseBuilds.length}`, value: 'release' },
            { label: `调试包 ${androidDebugBuilds.length}`, value: 'debug' },
          ]"
        />
      </div>

      <div v-if="!visibleAndroidBuilds.length" class="hist-empty">
        {{
          historyKind === "debug"
            ? "还没有调试包，点右上角「构建调试包」生成。"
            : "还没有正式包，点右上角「构建 Android」生成。"
        }}
      </div>

      <div v-else class="hist-list">
        <div v-for="item in visibleAndroidBuilds" :key="item.id" class="hist-row" :class="{ 'hist-row-failed': item.failed }">
          <span class="hist-time mono">{{ formatTime(item.createdAt) }}</span>
          <span class="hist-ver mono">v{{ item.versionName }} ({{ item.versionCode }})</span>
          <span class="hist-size mono">{{ item.failed ? "—" : mb(item.sizeBytes) + " MB" }}</span>
          <span class="hist-flag" :class="item.failed ? 'bad' : item.signed ? 'ok' : 'warn'">
            {{ item.failed ? "构建失败" : item.signed ? "正式签名" : "非正式签名" }}
          </span>
          <span v-if="item.failed" class="hist-path mono err" :title="item.error">{{ item.error }}</span>
          <span v-else class="hist-path mono" :title="item.apkPath">{{ item.apkPath }}</span>
          <el-button v-if="!item.failed" size="small" @click="reveal(item.apkPath)">打开</el-button>
          <el-button
            v-if="!item.failed && item.apkPath.endsWith('-debug.apk')"
            size="small"
            type="primary"
            @click="router.push({ name: 'project-debug', params: { id: projectId } })"
          >
            调试
          </el-button>
          <el-button size="small" :disabled="!item.logPath" @click="openBuildLog(item.logPath)">
            查看日志
          </el-button>
        </div>
      </div>
    </section>
      </div>

      <div v-show="activeTab === 'windows'" class="pane-tabs-wrap">
        <el-segmented v-model="winTab" block class="pane-seg" :options="winPaneOptions" />
      </div>

      <div v-show="activeTab === 'windows'" class="platform-body">
    <section v-show="winTab === 'app'" class="card">
      <div class="card-head">
        <span class="card-title">应用信息</span>
      </div>
      <div class="fields">
        <div class="field span-2">
          <span class="field-label">内容来源</span>
          <el-radio-group v-model="winForm.sourceKind">
            <el-radio-button value="remote">远程网址</el-radio-button>
            <el-radio-button value="local">本地静态包</el-radio-button>
          </el-radio-group>
        </div>
        <div v-if="winForm.sourceKind === 'remote'" class="field span-2">
          <span class="field-label">H5 首页地址</span>
          <el-input v-model="winForm.startUrl" placeholder="https://example.com/" />
        </div>
        <div v-else class="field span-2">
          <span class="field-label">本地 H5 路径</span>
          <div class="input-row">
            <el-input
              v-model="winForm.localDir"
              placeholder="含 index.html 的目录，或 .zip 压缩包"
              clearable
            />
            <el-button @click="pickWinDir">选目录</el-button>
            <el-button @click="pickWinZip">选 zip</el-button>
          </div>
          <span class="field-note">
            内容会复制到导出目录的 www/ 下，exe 启动时直接加载本地文件，不依赖服务器。
          </span>
        </div>
        <div class="field span-2">
          <span class="field-label">启动参数（可选）</span>
          <el-input
            v-model="winForm.startQuery"
            placeholder="如 key=value（会追加到启动地址）"
            clearable
          />
          <span class="field-note">
            按 URL 参数区分版本/环境的场景用：会拼成 …/index.html?key=value，
            页面重新加载或重启应用后仍带上。
          </span>
        </div>
      </div>
    </section>

    <section v-show="winTab === 'app'" v-if="winForm.sourceKind === 'local'" class="card">
      <div class="card-head">
        <span class="card-title">本地服务</span>
        <span class="card-hint">页面与接口同源，等价 vite proxy</span>
      </div>
      <div class="local-server-top">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">启用本地服务</div>
            <div class="toggle-desc">
              用 http://127.0.0.1 托管本地包，页面里的 /api 等相对路径可直接转发到后端
            </div>
          </div>
          <el-switch v-model="winForm.localServer.enabled" />
        </div>
        <label v-if="winForm.localServer.enabled" class="field">
          <span class="field-label">Base 地址</span>
          <el-input v-model="winForm.localServer.basePath" placeholder="/" clearable />
          <span class="field-note">
            页面部署在域名子目录时填 /h5/（此时访问 http://www.a.com/h5/），根目录保持 /。
            填了前缀后，形如 /h5/assets/app.js 的请求会映射到包内的 assets/app.js。
          </span>
        </label>
      </div>
      <div v-if="winForm.localServer.enabled" class="proxy-block">
        <div class="proxy-head">
          <span class="field-label">转发规则</span>
          <el-button size="small" @click="addProxyRule">添加规则</el-button>
        </div>
        <ProxyRuleRow
          v-for="(rule, index) in winProxyRules"
          :key="index"
          :rule="rule"
          :index="index"
          @remove="removeProxyRule(index)"
        />
        <p class="card-note">
          前缀匹配（最长优先），命中后保留原路径转发到目标地址，http(s) 与 ws(s) 都支持。相对路径能直接调通，后端不需要配 CORS；
          若后端会校验 Origin（例如 nginx 只放行自己的域名），保持「改写 Origin」开启即可。
        </p>
        <p class="card-note">
          前端接入：壳会在页面脚本执行前注入 <code>window.JianLing</code>，用
          <code>typeof window.JianLing !== 'undefined'</code> 判定是否在壳内；命中后接口走相对路径
          <code>/api</code>、ws 走同源 <code>ws://127.0.0.1:端口/ws</code>，由上面的规则转发到真实地址。
        </p>
      </div>
    </section>


    <section v-show="winTab === 'ui'" class="card">
      <div class="card-head">
        <span class="card-title">启动屏</span>
        <span class="card-hint">服务就绪前的第一屏</span>
      </div>
      <div class="toggles toggles-one">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">启用启动屏</div>
            <div class="toggle-desc">先显示静态页面，到点后再切到真正的网页</div>
          </div>
          <el-switch v-model="winForm.splash.enabled" />
        </div>
      </div>
      <div v-if="winForm.splash.enabled" class="fields spaced">
        <div class="field span-2">
          <span class="field-label">启动屏样式</span>
          <el-radio-group v-model="winForm.splash.mode">
            <el-radio-button value="default">默认</el-radio-button>
            <el-radio-button value="custom">自定义</el-radio-button>
          </el-radio-group>
          <span class="field-note">
            默认：背景 + 名称 + Logo；自定义：只用一张整屏图片。
          </span>
        </div>
      </div>
      <div
        v-if="winForm.splash.enabled && winForm.splash.mode !== 'custom'"
        class="fields spaced"
      >
        <label class="field">
          <span class="field-label">显示名称</span>
          <el-input v-model="winForm.splash.title" placeholder="留空则用应用名" clearable />
        </label>
        <div class="field">
          <span class="field-label">背景色</span>
          <div class="input-row">
            <el-input
              v-model="winForm.splash.backgroundColor"
              placeholder="#FFFFFF"
              class="mono-input"
            />
            <el-color-picker
              v-model="winForm.splash.backgroundColor"
              @change="(v: any) => setSplashColor('windows', v)"
            />
          </div>
        </div>
        <div class="field span-2">
          <span class="field-label">背景图（可选，优先于背景色）</span>
          <div class="input-row">
            <el-input
              v-model="winForm.splash.backgroundImage"
              placeholder="选了就用这张图铺满背景"
              clearable
            />
            <el-button @click="pickSplashBackground('windows')">选择</el-button>
          </div>
        </div>
        <div class="field span-2">
          <span class="field-label">Logo（可选）</span>
          <div class="input-row">
            <el-input v-model="winForm.splash.logoPath" placeholder="留空则用应用图标" clearable />
            <el-button @click="pickSplashLogo('windows')">选择</el-button>
          </div>
        </div>
      </div>
      <div
        v-if="winForm.splash.enabled && winForm.splash.mode === 'custom'"
        class="fields spaced"
      >
        <div class="field span-2">
          <span class="field-label">整屏图片</span>
          <div class="input-row">
            <el-input
              v-model="winForm.splash.imagePath"
              placeholder="整屏铺满这张图，推荐与窗口同比例"
              clearable
            />
            <el-button @click="pickSplashImage('windows')">选择</el-button>
          </div>
        </div>
      </div>
      <p v-if="winForm.splash.enabled" class="card-note">
        默认样式：背景图优先于背景色；名称留空用应用名、Logo 留空用应用图标，水平居中、垂直位于窗口 1/3 处。
        自定义样式：只显示这一张整屏图片。
      </p>
    </section>

    <section v-show="winTab === 'app'" class="card">
      <div class="card-head">
        <span class="card-title">运行时内核</span>
        <span class="card-hint">WebView2 固定版本运行时</span>
      </div>
      <div class="fields">
        <label class="field span-2">
          <span class="field-label">内核</span>
          <el-select v-model="winForm.kernelId" placeholder="跟随系统" clearable>
            <el-option :value="null" label="跟随系统（不内置内核）" />
            <el-option
              v-for="item in settings?.windowsKernels ?? []"
              :key="item.id"
              :value="item.id"
              :label="`WebView2 ${item.version}（${mb(item.sizeBytes)} MB）`"
            />
          </el-select>
          <span class="field-note">
            内置内核后产物会多出一个 WebView2 目录（解压后约 680 MB，单文件/zip 都会明显变大）；不选则使用系统已安装的 WebView2。
            内核在设置页下载或导入。
          </span>
        </label>
      </div>
    </section>

    <section v-show="winTab === 'ui'" class="card">
      <div class="card-head">
        <span class="card-title">窗口</span>
        <span class="card-hint">{{ winSizeText }}</span>
      </div>
      <div class="fields">
        <div class="field span-2">
          <span class="field-label">尺寸方式</span>
          <el-radio-group v-model="winForm.sizeMode">
            <el-radio-button value="fixed">指定分辨率</el-radio-button>
            <el-radio-button value="ratio">固定比例</el-radio-button>
            <el-radio-button value="fullscreen">全屏</el-radio-button>
          </el-radio-group>
        </div>

        <template v-if="winForm.sizeMode === 'fixed'">
          <label class="field">
            <span class="field-label">宽度</span>
            <el-input-number
              v-model="winForm.width"
              :min="320"
              :max="7680"
              :step="20"
              controls-position="right"
              class="full"
            />
          </label>
          <label class="field">
            <span class="field-label">高度</span>
            <el-input-number
              v-model="winForm.height"
              :min="240"
              :max="4320"
              :step="20"
              controls-position="right"
              class="full"
            />
          </label>
        </template>

        <template v-else-if="winForm.sizeMode === 'ratio'">
          <div class="field span-2">
            <span class="field-label">宽高比</span>
            <div class="ratio-row">
              <el-radio-group v-model="winOrientation">
                <el-radio-button value="landscape">横屏</el-radio-button>
                <el-radio-button value="portrait">竖屏</el-radio-button>
              </el-radio-group>
              <el-input-number
                v-model="winForm.aspectW"
                :min="1"
                :max="100"
                controls-position="right"
                class="ratio-input"
              />
              <span class="ratio-colon">:</span>
              <el-input-number
                v-model="winForm.aspectH"
                :min="1"
                :max="100"
                controls-position="right"
                class="ratio-input"
              />
              <el-button class="ratio-preset" @click="setAspect(16, 9)">16:9</el-button>
              <el-button class="ratio-preset" @click="setAspect(4, 3)">4:3</el-button>
              <el-button class="ratio-preset" @click="setAspect(21, 9)">21:9</el-button>
            </div>
          </div>
          <label class="field span-2">
            <span class="field-label">宽度（高度按比例自动计算）</span>
            <el-input-number
              v-model="winForm.width"
              :min="320"
              :max="7680"
              :step="20"
              controls-position="right"
              class="full"
            />
            <span class="field-note">按当前设置：{{ winSizeText }}</span>
          </label>
        </template>

        <div v-else class="field span-2">
          <span class="field-note">
            启动后直接以无边框全屏显示，铺满当前显示器，适合大屏展示或一体机。
          </span>
        </div>
      </div>
      <div class="fields spaced">
        <label class="field">
          <span class="field-label">最小窗口宽</span>
          <el-input-number
            v-model="winForm.minWidth"
            :min="0"
            :max="7680"
            :step="10"
            controls-position="right"
            class="full"
          />
          <span class="field-note">0 表示不限制；防止拖得太小把页面挤坏</span>
        </label>
        <label class="field">
          <span class="field-label">最小窗口高</span>
          <el-input-number
            v-model="winForm.minHeight"
            :min="0"
            :max="4320"
            :step="10"
            controls-position="right"
            class="full"
          />
          <span class="field-note">0 表示不限制</span>
        </label>
      </div>
      <div class="toggles spaced">
        <div v-if="winForm.sizeMode !== 'fullscreen'" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">允许调整窗口大小</div>
            <div class="toggle-desc">
              {{
                winForm.sizeMode === "ratio"
                  ? "拖动时按宽高比等比缩放，标题栏不提供最大化"
                  : "关闭后窗口尺寸固定"
              }}
            </div>
          </div>
          <el-switch v-model="winForm.resizable" />
        </div>
        <div v-if="winForm.sizeMode !== 'fullscreen'" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">无边框窗口</div>
            <div class="toggle-desc">隐藏系统标题栏，适合自定义 UI</div>
          </div>
          <el-switch v-model="winForm.frameless" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">窗口置顶</div>
            <div class="toggle-desc">始终显示在其他窗口之上</div>
          </div>
          <el-switch v-model="winForm.alwaysOnTop" />
        </div>
        <div v-if="winForm.sizeMode === 'fixed'" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">启动最大化</div>
            <div class="toggle-desc">启动后直接最大化显示</div>
          </div>
          <el-switch v-model="winForm.maximized" />
        </div>
      </div>
    </section>

    <section v-show="winTab === 'ui'" class="card">
      <div class="card-head">
        <span class="card-title">页面</span>
        <span class="card-hint">缩放、字号、进度条与操作栏</span>
      </div>
      <div class="fields spaced">
        <div class="field">
          <span class="field-label">页面缩放</span>
          <el-select v-model="winForm.zoom" class="full">
            <el-option
              v-for="z in zoomOptions"
              :key="z"
              :label="`${Math.round(z * 100)}%`"
              :value="z"
            />
          </el-select>
        </div>
        <div class="field">
          <span class="field-label">最小字号</span>
          <el-select v-model="winForm.minFontSize" class="full">
            <el-option
              v-for="f in minFontOptions"
              :key="f"
              :label="f === 0 ? '不限制' : `${f}px`"
              :value="f"
            />
          </el-select>
          <span class="field-note">
            页面里小于该字号的文字会被放大到该值，适合桌面端显示手机版页面。
          </span>
        </div>
      </div>
      <div class="toggles toggles-half spaced">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">加载进度条</div>
            <div class="toggle-desc">{{ winForm.progressBar ? "页面加载时在顶部显示" : "已关闭" }}</div>
          </div>
          <el-switch v-model="winForm.progressBar" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">顶部操作栏</div>
            <div class="toggle-desc">{{ winForm.navBar ? "显示前进 / 后退 / 刷新" : "已关闭" }}</div>
          </div>
          <el-switch v-model="winForm.navBar" />
        </div>
      </div>
      <div v-if="winForm.progressBar" class="fields spaced">
        <label class="field">
          <span class="field-label">进度条颜色</span>
          <div class="input-row">
            <el-input v-model="winForm.progressColor" placeholder="#409EFF" class="mono-input" />
            <el-color-picker v-model="winForm.progressColor" />
          </div>
        </label>
        <label class="field">
          <span class="field-label">高度（px）</span>
          <el-input-number
            v-model="winForm.progressHeight"
            :min="1"
            :max="12"
            :step="1"
            controls-position="right"
            class="full"
          />
        </label>
      </div>
      <div class="fields spaced">
        <div class="field span-2">
          <span class="field-label">预览</span>
          <div class="top-preview">
            <div v-if="winForm.navBar" class="preview-nav">
              <span class="preview-nav-btn">
                <svg viewBox="0 0 24 24" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M15 5L9 12L15 19" /></svg>
              </span>
              <span class="preview-nav-btn">
                <svg viewBox="0 0 24 24" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 5L15 12L9 19" /></svg>
              </span>
              <span class="preview-nav-btn">
                <svg viewBox="0 0 24 24" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.67981 13L3.67981 11.3333C3.67981 6.73096 7.4402 3 12.0789 3C15.1178 3 17.7799 4.60136 19.2545 7M2 11.3333L3.67981 13L5.35962 11.3333" /><path d="M20.3139 11V12.6667C20.3139 17.269 16.5391 21 11.8827 21C8.83213 21 6.15995 19.3986 4.67969 17M22.0001 12.6667L20.3139 11L18.6277 12.6667" /></svg>
              </span>
            </div>
            <span
              v-if="winForm.progressBar"
              class="top-preview-progress"
              :style="{ background: winForm.progressColor, height: winForm.progressHeight + 'px' }"
            ></span>
            <div class="top-preview-page">网页内容</div>
          </div>
        </div>
      </div>
    </section>

    <section v-show="winTab === 'behavior'" class="card">
      <div class="card-head">
        <span class="card-title">壳行为</span>
        <span class="card-hint">窗口生命周期与页面行为</span>
      </div>
      <div class="toggles spaced">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">单实例运行</div>
            <div class="toggle-desc">重复启动不再开新窗口，而是激活已有窗口</div>
          </div>
          <el-switch v-model="winForm.singleInstance" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">开机自启</div>
            <div class="toggle-desc">随 Windows 启动自动运行（写入当前用户启动项）</div>
          </div>
          <el-switch v-model="winForm.autoStart" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">关闭按钮行为</div>
            <div class="toggle-desc">
              {{
                winForm.closeMode === "tray"
                  ? "收进系统托盘，左键图标还原，右键可退出"
                  : winForm.closeMode === "minimize"
                    ? "收进任务栏，点任务栏图标还原"
                    : "直接退出程序"
              }}
            </div>
          </div>
          <el-select v-model="winForm.closeMode" class="perm-select">
            <el-option label="退出程序" value="exit" />
            <el-option label="最小化到任务栏" value="minimize" />
            <el-option label="最小化到托盘" value="tray" />
          </el-select>
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">记住窗口位置和大小</div>
            <div class="toggle-desc">下次启动回到上次的位置与尺寸（全屏 / 启动最大化时忽略）</div>
          </div>
          <el-switch v-model="winForm.rememberWindow" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">下载统一存到应用子目录</div>
            <div class="toggle-desc">文件落到「下载\应用名」，不再和其他下载混在一起</div>
          </div>
          <el-switch v-model="winForm.downloadToSubfolder" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">开启调试</div>
            <div class="toggle-desc risk">可用 DevTools 调试页面</div>
          </div>
          <el-switch v-model="winForm.debugEnabled" />
        </div>
      </div>
      <div class="toggles spaced">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">允许自动播放</div>
            <div class="toggle-desc">音视频无需用户点击即可播放</div>
          </div>
          <el-switch v-model="winForm.autoplay" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">忽略证书错误</div>
            <div class="toggle-desc">
              默认开启：放行自签名 / 不受信任的 https 证书，内网自签才需要；公开站点请关闭
            </div>
          </div>
          <el-switch v-model="winForm.ignoreCertErrors" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁用右键菜单</div>
            <div class="toggle-desc">屏蔽系统右键菜单（含「另存为」「打印」）</div>
          </div>
          <el-switch v-model="winForm.blockContextMenu" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁止文字选中</div>
            <div class="toggle-desc">像原生应用一样无法选中文字</div>
          </div>
          <el-switch v-model="winForm.blockTextSelect" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">隐藏滚动条</div>
            <div class="toggle-desc">页面滚动正常，但滚动条不显示</div>
          </div>
          <el-switch v-model="winForm.hideScrollbars" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁用 Ctrl 缩放</div>
            <div class="toggle-desc">屏蔽 Ctrl + 滚轮 / Ctrl ± 缩放页面</div>
          </div>
          <el-switch v-model="winForm.blockCtrlZoom" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">禁用浏览器快捷键</div>
            <div class="toggle-desc">屏蔽 F5 刷新、Ctrl+P 打印、Ctrl+F 查找等（开启调试时自动放行）</div>
          </div>
          <el-switch v-model="winForm.blockBrowserKeys" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">外链用系统浏览器</div>
            <div class="toggle-desc">window.open 与「在新窗口打开」的链接交给默认浏览器，不在套壳内开窗</div>
          </div>
          <el-switch v-model="winForm.externalLinksInBrowser" />
        </div>
      </div>
      <div class="fields spaced">
        <div class="field">
          <span class="field-label">User-Agent</span>
          <el-radio-group v-model="winForm.uaMode">
            <el-radio-button value="append">追加</el-radio-button>
            <el-radio-button value="override">整串替换</el-radio-button>
          </el-radio-group>
        </div>
        <label class="field span-2of3">
          <span class="field-label">
            {{ winForm.uaMode === "override" ? "自定义 UA" : "UA 附加信息（可留空）" }}
          </span>
          <el-input
            v-model="winForm.userAgent"
            :placeholder="
              winForm.uaMode === 'override'
                ? 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) ...'
                : '加在系统 UA 末尾的标记'
            "
            clearable
          />
        </label>
      </div>
    </section>

    <section v-show="winTab === 'permission'" class="card">
      <div class="card-head">
        <span class="card-title">套壳权限</span>
        <span class="card-hint">网页请求时的默认应答</span>
      </div>
      <div class="win-perms">
        <div v-for="row in winPermissionRows" :key="row.key" class="win-perm">
          <div class="toggle-text">
            <div class="toggle-name">{{ row.label }}</div>
            <div class="toggle-desc">{{ row.desc }}</div>
          </div>
          <el-select v-model="winForm.permissions[row.key]" class="perm-select">
            <el-option
              v-for="s in winPermissionStates"
              :key="s.value"
              :label="s.label"
              :value="s.value"
            />
          </el-select>
        </div>
      </div>
      <p class="card-note">
        「系统提示」交给 WebView2 内核弹窗询问用户；「自动允许 / 自动拒绝」直接放行或拦截，不再打扰用户。
      </p>
    </section>

    <section v-show="winTab === 'sign'" class="card">
      <div class="card-head">
        <span class="card-title">代码签名</span>
        <span class="card-hint">可选，留空导出未签名程序</span>
      </div>
      <div class="fields">
        <div class="field span-2">
          <span class="field-label">签名证书（.pfx / .p12）</span>
          <div class="input-row">
            <el-input
              v-model="winForm.certPath"
              placeholder="选择 Authenticode 代码签名证书，留空不签名"
              clearable
            />
            <el-button @click="pickWinCert">选择</el-button>
          </div>
        </div>
        <label class="field">
          <span class="field-label">证书密码</span>
          <el-input v-model="winForm.certPassword" type="password" show-password />
        </label>
      </div>
      <p class="card-note">
        Windows 的签名不是 .jks，而是 Authenticode 证书（.pfx），需要用系统里的 signtool
        签名（Windows SDK 自带）。不签名也能运行，但会触发 SmartScreen 警告。
      </p>
    </section>

    <section v-show="winTab === 'build'" class="card">
      <div class="card-head">
        <span class="card-title">产物</span>
        <span class="card-hint">勾选这次要打出的形态，可多选</span>
      </div>
      <div class="toggles">
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">安装包（.exe）</div>
            <div class="toggle-desc">
              {{
                winForm.installScope === "machine"
                  ? "装到 Program Files，所有用户可用；安装时请求管理员权限"
                  : "装到 %LOCALAPPDATA%\\Programs，带桌面 / 开始菜单快捷方式与卸载项"
              }}
            </div>
          </div>
          <el-checkbox v-model="winForm.outputInstaller" />
        </div>
        <div v-if="winForm.outputInstaller" class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">安装范围</div>
            <div class="toggle-desc">覆盖安装前会自动结束正在运行的实例</div>
          </div>
          <el-select v-model="winForm.installScope" class="perm-select">
            <el-option label="当前用户" value="user" />
            <el-option label="所有用户" value="machine" />
          </el-select>
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">单文件 exe</div>
            <div class="toggle-desc">页面资源内嵌在这一个文件里，拷走就能运行</div>
          </div>
          <el-checkbox v-model="winForm.outputSingle" />
        </div>
        <div class="toggle">
          <div class="toggle-text">
            <div class="toggle-name">绿色版 zip</div>
            <div class="toggle-desc">解压即用：exe + www/ + shell.json</div>
          </div>
          <el-checkbox v-model="winForm.outputPortable" />
        </div>
      </div>
      <p class="card-note">
        产物会写进项目的「产物输出目录」，中间目录用完即清理。
        {{ winForm.outputInstaller || winForm.outputSingle || winForm.outputPortable ? "" : "当前一个都没勾选，构建会被拦下。" }}
      </p>
    </section>

    <section v-show="winTab === 'build'" class="card">
      <div class="card-head">
        <span class="card-title">
          构建
          <span v-if="winBuildError" class="failed-tag">构建失败</span>
          <span v-else-if="exporting" class="running-tag">构建中…</span>
        </span>
        <el-button v-if="exporting || winLogs.length" @click="logVisible = true">
          构建日志
          <span v-if="winLogs.length" class="log-count">{{ winLogs.length }}</span>
        </el-button>
      </div>

      <div v-if="winBuildError" class="build-error">
        <div class="build-error-head">
          <span class="build-error-title">构建失败</span>
          <el-button size="small" @click="logVisible = true">查看日志</el-button>
        </div>
        <div class="build-error-msg mono">{{ winBuildError }}</div>
      </div>

      <div v-if="winResult" class="result">
        <div class="stats">
          <div class="stat">
            <span class="k">大小</span>
            <span class="v mono">{{ mb(winResult.sizeBytes) }} MB</span>
          </div>
          <div class="stat">
            <span class="k">签名</span>
            <span class="v" :class="winResult.signed ? 'ok' : 'warn'">
              {{ winResult.signed ? "已签名" : "未签名" }}
            </span>
          </div>
          <div class="stat">
            <span class="k">耗时</span>
            <span class="v mono">{{ winResult.durationSecs }}s</span>
          </div>
          <div class="stat artifact">
            <span class="k">产物</span>
            <div class="artifact-row">
              <span class="artifact-path mono" :title="winResult.exePath">
                {{ winResult.exePath }}
              </span>
              <el-button size="small" :icon="FolderOpened" @click="reveal(winResult.dir)">
                打开
              </el-button>
            </div>
          </div>
        </div>
      </div>

      <p v-else-if="!exporting" class="build-hint">
        点右上角「构建 Windows」按上面勾选的形态产出，过程日志在「构建日志」里查看。
      </p>
      <p v-else class="build-hint live">正在构建，日志实时输出到「构建日志」。</p>
    </section>

    <section v-show="winTab === 'build'" class="card">
      <div class="card-head">
        <span class="card-title">构建历史</span>
        <span class="card-hint">{{ windowsBuilds.length }} 条记录</span>
      </div>

      <div v-if="!windowsBuilds.length" class="hist-empty">
        还没有 Windows 构建记录，点右上角「构建 Windows」生成第一个版本。
      </div>

      <div v-else class="hist-list">
        <div v-for="item in windowsBuilds" :key="item.id" class="hist-row" :class="{ 'hist-row-failed': item.failed }">
          <span class="hist-time mono">{{ formatTime(item.createdAt) }}</span>
          <span class="hist-ver mono">v{{ item.versionName }} ({{ item.versionCode }})</span>
          <span class="hist-size mono">{{ item.failed ? "—" : mb(item.sizeBytes) + " MB" }}</span>
          <span class="hist-flag" :class="item.failed ? 'bad' : item.signed ? 'ok' : 'warn'">
            {{ item.failed ? "构建失败" : item.signed ? "已签名" : "未签名" }}
          </span>
          <span v-if="item.failed" class="hist-path mono err" :title="item.error">{{ item.error }}</span>
          <span v-else class="hist-path mono" :title="item.apkPath">{{ item.apkPath }}</span>
          <el-button v-if="!item.failed" size="small" @click="reveal(item.apkPath)">打开</el-button>
          <el-button size="small" :disabled="!item.logPath" @click="openBuildLog(item.logPath)">
            查看日志
          </el-button>
        </div>
      </div>
    </section>
      </div>
    </section>
    </el-scrollbar>

    <el-dialog
      v-model="logVisible"
      title="构建日志"
      width="min(900px, 92vw)"
      align-center
      @opened="scrollLogs"
    >
      <div class="log">
        <div ref="logBox" class="log-body">
          <div v-if="!activeLogs.length" class="log-empty">等待构建…</div>
          <div
            v-for="(item, index) in activeLogs"
            :key="index"
            class="log-line"
            :class="item.stream"
          >
            {{ item.line }}
          </div>
        </div>
      </div>
      <template #footer>
        <el-button :disabled="!activeLogs.length" @click="clearLogs">清空</el-button>
        <el-button type="primary" @click="logVisible = false">关闭</el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="fileLogVisible"
      title="构建日志"
      width="min(900px, 92vw)"
      align-center
    >
      <div class="log">
        <div class="log-path">{{ fileLogPath }}</div>
        <div class="log-body">
          <div v-if="fileLogLoading" class="log-empty">读取中…</div>
          <div v-else-if="!fileLogLines.length" class="log-empty">日志文件为空</div>
          <div v-for="(line, index) in fileLogLines" :key="index" class="log-line">
            {{ line }}
          </div>
        </div>
      </div>
      <template #footer>
        <el-button type="primary" @click="fileLogVisible = false">关闭</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.head-left {
  display: flex;
  align-items: flex-start;
  gap: 11px;
}

.back-btn {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  margin-top: 3px;
  border: 1px solid var(--border-default);
  border-radius: 9px;
  background: var(--surface-input);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background-color 0.16s var(--ease), color 0.16s var(--ease),
    border-color 0.16s var(--ease);
}

.back-btn:hover {
  background: #ffffff;
  border-color: var(--border-strong);
  color: var(--text-primary);
}

h1 {
  margin: 0;
  font-size: 22px;
  font-weight: 650;
  letter-spacing: -0.02em;
  color: var(--text-primary);
}

.head-meta {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-top: 7px;
  font-size: 12.5px;
  color: var(--text-muted);
}

.status-dot {
  width: 6px;
  height: 6px;
  flex: 0 0 6px;
  border-radius: 50%;
  background: #b6bec9;
}

.status-dot.live {
  background: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.status-dot.failed {
  background: var(--danger);
  box-shadow: 0 0 0 3px var(--danger-soft);
}

.dot-sep {
  color: var(--text-faint);
}

.warn-inline {
  margin-left: 4px;
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--warning-soft);
  color: var(--warning);
  font-size: 11.5px;
}

.head-actions {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
}

.card {
  margin-bottom: 14px;
  padding: 16px 18px 18px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  background: var(--surface);
  box-shadow: var(--shadow-card);
}

.card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 14px;
}

.card-title {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-primary);
}

.card-hint {
  font-size: 11.5px;
  color: var(--text-faint);
}

.build-actions {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
}

.platform-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.seg {
  display: inline-flex;
  padding: 3px;
  border: 1px solid var(--border-subtle);
  border-radius: 10px;
  background: var(--surface-sunken);
}

.seg-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 15px;
  border: none;
  border-radius: 8px;
  background: transparent;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-muted);
  cursor: pointer;
  transition: background-color 0.16s var(--ease), color 0.16s var(--ease),
    box-shadow 0.16s var(--ease);
}

.seg-item:hover {
  color: var(--text-primary);
}

.seg-item.on {
  background: #ffffff;
  color: var(--text-primary);
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.08), 0 2px 6px -2px rgba(15, 23, 42, 0.14);
}

.seg-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  opacity: 0.3;
  transition: background-color 0.16s var(--ease), opacity 0.16s var(--ease);
}

.seg-item.on .seg-dot {
  background: var(--accent);
  opacity: 1;
}

.platform-hint {
  font-size: 11.5px;
  color: var(--text-faint);
}

/* 平台切换器的内层分组：去掉独立卡片的外壳，只保留标题与分隔线 */
.pane-tabs-wrap {
  position: sticky;
  top: 0;
  z-index: 3;
  padding: 14px 0;
  background: var(--surface-solid);
  margin-bottom: -6px;
}

/* 分段器：等分整行，点击区域更大 */
.pane-seg {
  width: 100%;
  --el-border-radius-base: 9px;
  --el-segmented-item-selected-bg-color: var(--accent);
  --el-segmented-item-selected-color: #fff;
}

.pane-seg :deep(.el-segmented__group) {
  display: flex;
  width: 100%;
}

.pane-seg :deep(.el-segmented__item) {
  flex: 1;
  min-height: 34px;
  padding: 0 10px;
  font-size: 13px;
}

.hist-seg {
  --el-segmented-item-selected-bg-color: var(--surface-solid);
  --el-segmented-item-selected-color: var(--text-primary);
  --el-border-radius-base: 8px;
}

.hist-seg :deep(.el-segmented__item) {
  padding: 0 16px;
  font-size: 12.5px;
  line-height: 30px;
  min-height: 30px;
}

.platform-body {
  padding-top: 14px;
}

.platform-body > .card {
  margin: 0 0 18px;
  padding: 0;
  border: 0;
  border-radius: 0;
  background: transparent;
  box-shadow: none;
}

.platform-body > .card:last-of-type {
  margin-bottom: 0;
}

/* 一行最多三个字段：单个占满整行，两个各占一半，三个等分，不足时自动撑满剩余空间 */
.fields {
  display: flex;
  flex-wrap: wrap;
  gap: 13px 16px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.fields > .field {
  flex: 1 1 calc(33.333% - 11px);
}

.fields > .field.span-2 {
  flex-basis: 100%;
}

.field-note code {
  padding: 0 3px;
  border-radius: 3px;
  background: var(--surface-input);
  font-family: var(--font-mono);
  font-size: 11px;
}

.fields > .field.span-2of3 {
  flex: 2 1 calc(66.666% - 5px);
}

.input-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.input-row :deep(.el-input) {
  flex: 1 1 auto;
  width: auto;
  min-width: 0;
}

.input-row :deep(.el-input__wrapper) {
  width: 100%;
}

.input-row :deep(.el-button) {
  flex: 0 0 auto;
}

.field-note {
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--text-faint);
}

.fields.spaced,
.toggles.spaced,
.sign-grid.spaced {
  margin-top: 13px;
}

.sign-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 13px 16px;
}

.field-label {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-secondary);
}

.full {
  width: 100%;
}

.mono-input :deep(.el-input__inner) {
  font-family: var(--font-mono);
  font-size: 12.5px;
}

/* 固定 4 列：条目不足一行时卡片宽度不变，避免少一项就变宽 */
.toggles {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

/* 侧栏占掉宽度后，窄窗口退成两列 / 一列，避免文字挤成竖条 */
@media (max-width: 1000px) {
  .toggles {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 720px) {
  .toggles {
    grid-template-columns: minmax(0, 1fr);
  }
}

/* 只有两个开关时对半分，避免空出三分之一 */
.toggles-half {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

/* 只有一个开关时占满整行 */
.toggles-one {
  grid-template-columns: minmax(0, 1fr);
}

@media (max-width: 720px) {
  .toggles-half {
    grid-template-columns: minmax(0, 1fr);
  }
}

/* 开关占满整行，Base 地址单独一行 */
.local-server-top {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
}

.toggle-name {
  font-size: 13px;
  color: var(--text-primary);
}

.toggle-desc {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.toggle-desc.risk {
  color: var(--warning);
}

.debug-console-select {
  width: 130px;
  flex: none;
}

.win-perms {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 10px;
}

.win-perm {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
}

.perm-select {
  flex: none;
  width: 118px;
}

.proxy-block {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.proxy-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

/* 高度由右侧弹窗配置决定，左侧列表填充同高并在内部滚动 */
.perm-layout {
  display: grid;
  grid-template-columns: 238px minmax(0, 1fr);
  gap: 16px;
  align-items: stretch;
}

.perm-side {
  position: relative;
  min-height: 0;
  padding: 8px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
}

.perm-side-list {
  position: absolute;
  inset: 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  padding-right: 4px;
}

.perm-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 8px 10px;
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition: background-color 0.16s var(--ease), border-color 0.16s var(--ease);
}

.perm-item:hover {
  background: var(--surface-sunken);
}

.perm-item.active {
  border-color: rgba(21, 128, 61, 0.35);
  background: var(--accent-soft);
}

.perm-check {
  width: 15px;
  height: 15px;
  flex: 0 0 15px;
  border: 1px solid var(--border-strong);
  border-radius: 4px;
  background: #ffffff;
  transition: background-color 0.16s var(--ease), border-color 0.16s var(--ease);
}

.perm-check.on {
  border-color: var(--accent);
  background: var(--accent);
  box-shadow: inset 0 0 0 2px #ffffff;
}

.perm-side-text {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.perm-name {
  font-size: 13px;
  color: var(--text-primary);
}

.perm-desc {
  font-size: 11px;
  line-height: 1.35;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.perm-detail {
  padding: 14px 16px 16px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
}

.detail-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 14px;
}

.detail-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.detail-sub {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.detail-note {
  margin: 12px 0 0;
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--text-faint);
}

.ratio-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.ratio-input {
  width: 92px;
}

.ratio-colon {
  color: var(--text-muted);
}

.ratio-preset {
  min-width: 58px;
}

.card-note {
  margin: 12px 0 0;
  font-size: 11.5px;
  color: var(--text-faint);
}

.result {
  margin-bottom: 12px;
}

.stats {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr 2fr;
  gap: 9px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-sunken);
}

.stat .k {
  font-size: 11px;
  letter-spacing: 0.05em;
  color: var(--text-muted);
}

.stat .v {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.stat .v.ok {
  color: var(--accent);
}

.stat .v.warn {
  color: var(--warning);
}

.artifact-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.artifact-path {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log {
  overflow: hidden;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-sunken);
}

.log-body {
  height: min(58vh, 560px);
  min-height: 220px;
  overflow: auto;
  padding: 12px 14px;
}

/* 历史日志弹窗首行：日志文件路径 */
.log-path {
  padding: 8px 14px;
  border-bottom: 1px solid var(--border-subtle);
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--text-muted);
  word-break: break-all;
}

.build-hint {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-muted);
}

.build-hint.live {
  color: var(--accent);
}

.log-count {
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--surface-sunken);
  font-size: 11px;
  color: var(--text-muted);
}

.log-empty {
  font-size: 12px;
  color: var(--text-faint);
}

.log-line {
  font-family: var(--font-mono);
  font-size: 11.5px;
  line-height: 1.7;
  color: #52606f;
  white-space: pre-wrap;
  word-break: break-all;
}

.log-line.err {
  color: var(--danger);
}

.log-line.info {
  color: var(--accent);
}

.hist-empty {
  padding: 4px 0 2px;
  font-size: 12.5px;
  color: var(--text-faint);
}

.hist-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.hist-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 7px 10px;
  margin: 0 -10px;
  border-radius: 8px;
  font-size: 12px;
  transition: background-color 0.16s var(--ease);
}

.hist-row:hover {
  background: var(--surface-sunken);
}

.hist-time {
  flex: 0 0 126px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.hist-ver {
  flex: 0 0 118px;
  color: var(--text-secondary);
}

.hist-size {
  flex: 0 0 70px;
  color: var(--text-secondary);
}

.hist-flag {
  flex: 0 0 auto;
  min-width: 64px;
  font-size: 11.5px;
  white-space: nowrap;
}

.hist-flag.ok {
  color: var(--accent);
}

.hist-flag.warn {
  color: var(--warning);
}

.hist-flag.bad {
  color: var(--danger);
  font-weight: 600;
}

.hist-row-failed .hist-time,
.hist-row-failed .hist-ver {
  color: var(--danger);
}

.hist-path.err {
  color: var(--danger);
}

/* 构建失败：卡片描边 + 标题红字 + 醒目的失败块 */
.card-failed {
  border-color: var(--danger);
  box-shadow: 0 0 0 1px var(--danger-soft);
}

.failed-tag {
  margin-left: 8px;
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--danger);
  color: #fff;
  font-size: 11.5px;
  font-weight: 600;
  vertical-align: middle;
}

.running-tag {
  margin-left: 8px;
  font-size: 11.5px;
  color: var(--text-secondary);
  vertical-align: middle;
}

.build-error {
  margin: 4px 0 12px;
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--danger-soft);
  border: 1px solid var(--danger);
}

.build-error-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.build-error-title {
  color: var(--danger);
  font-weight: 600;
  font-size: 13px;
}

.build-error-msg {
  margin-top: 6px;
  font-size: 11.5px;
  color: var(--danger);
  word-break: break-all;
  white-space: pre-wrap;
  max-height: 120px;
  overflow: auto;
}

.hist-path {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview-nav {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 3px 6px;
  background: #f5f5f7;
  border-bottom: 1px solid var(--border-subtle);
}

.preview-nav-btn {
  display: inline-grid;
  place-items: center;
  width: 14px;
  height: 14px;
  color: #555;
}

/* 顶部预览：状态栏 + 可选操作栏 + 进度条 + 网页区，跟着开关变化 */
.top-preview {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-subtle);
  border-radius: 8px;
  overflow: hidden;
  background: #fff;
}

.top-preview-status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 20px;
  padding: 0 8px;
  font-size: 10px;
  line-height: 1;
}

.top-preview-progress {
  display: block;
  width: 100%;
}

.top-preview-page {
  padding: 10px 8px;
  font-size: 11px;
  color: #7c8797;
  background: #e7ecf3;
}
</style>
