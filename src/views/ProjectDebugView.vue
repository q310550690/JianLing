<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import { ArrowLeft } from "@element-plus/icons-vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  adbDevices,
  adbInstallApk,
  adbInstalledPackages,
  adbKeepScreen,
  adbLaunchApp,
  adbLogcatClear,
  adbLogcatTail,
  adbPullLogs,
  adbPushWww,
  adbWebviewVersion,
  copyToClipboard,
  devtoolsConnect,
  devtoolsDisconnect,
  isDebugApkPath,
  loadSettings,
  type AdbDevice,
  type BuildRecord,
  type DevtoolsInfo,
  type Settings,
} from "../api";

const props = defineProps<{ projectId: string }>();
const router = useRouter();
const route = useRoute();

/** 入口指定的目标包（构建调试包 / 历史「调试」按钮会带 target=debug） */
const forcedTarget = computed<"release" | "debug" | "">(() => {
  const target = route.query.target;
  return target === "debug" || target === "release" ? target : "";
});
let targetApplied = false;

const settings = ref<Settings | null>(null);
const devices = ref<AdbDevice[]>([]);
const selected = ref("");
const loadingDevices = ref(false);
const devicesError = ref("");
const webviewVersion = ref("");
const versionError = ref("");

const installing = ref(false);
const launching = ref(false);
const installMessage = ref("");
const pushing = ref(false);

const devtools = ref<DevtoolsInfo | null>(null);
const devtoolsBusy = ref(false);
const keepScreen = ref(false);

const logText = ref("");
const logFilter = ref("");
const logLoading = ref(false);
const autoRefresh = ref(true);
const logBox = ref<HTMLElement | null>(null);
/** 是否停在底部（仅控制「回到最新」按钮的显隐） */
const stickToBottom = ref(true);
let timer: number | null = null;

const project = computed(
  () => settings.value?.projects.find((p) => p.id === props.projectId) ?? null,
);
const android = computed(() => project.value?.android ?? null);
const appName = computed(() => android.value?.appName ?? "");
const basePackage = computed(() => android.value?.packageName ?? "");
const debugPackage = computed(() =>
  basePackage.value ? `${basePackage.value}.debug` : "",
);

const distDir = computed(() => android.value?.localDir ?? "");
/** 只有本地静态资源（含本地服务）才有 dist 可推；远程地址模式没有意义 */
const canPush = computed(
  () => android.value?.sourceKind === "local" && !!distDir.value,
);

const debugApk = computed<BuildRecord | null>(() => {
  const list = (project.value?.builds ?? []).filter(
    (b) => b.platform === "android" && !b.failed && isDebugApkPath(b.apkPath),
  );
  if (!list.length) return null;
  return list.reduce((a, b) => (b.createdAt > a.createdAt ? b : a));
});

/** 目标包：正式包（只开了调试开关）还是调试包（名字带 .debug） */
const target = ref<"release" | "debug">("release");
/** 设备上已安装的候选包名 */
const installed = ref<string[]>([]);
const targetPackage = computed(() =>
  target.value === "debug" ? debugPackage.value : basePackage.value,
);
const targetInstalled = computed(
  () => !!targetPackage.value && installed.value.includes(targetPackage.value),
);

async function detectInstalled() {
  installed.value = [];
  if (!selected.value || !basePackage.value) return;
  try {
    installed.value = await adbInstalledPackages(selected.value, [
      basePackage.value,
      debugPackage.value,
    ]);
  } catch {
    // 探测失败不影响其它步骤，交给按钮状态提示
    installed.value = [];
  }
  // 入口指定了目标包（构建完调试包跳过来）就优先用它，不再按已装情况猜测
  if (!targetApplied) {
    targetApplied = true;
    if (forcedTarget.value) {
      target.value = forcedTarget.value;
      return;
    }
  }
  // 只装了一个就选它；两个都装时保持用户选择
  const hasRelease = installed.value.includes(basePackage.value);
  const hasDebug = installed.value.includes(debugPackage.value);
  if (hasDebug && !hasRelease) target.value = "debug";
  else if (hasRelease && !hasDebug) target.value = "release";
}

const current = computed(
  () => devices.value.find((d) => d.serial === selected.value) ?? null,
);
const chromeMajor = computed(() => {
  const major = webviewVersion.value.split(".")[0];
  return major && /^\d+$/.test(major) ? major : "";
});

async function loadConfig() {
  try {
    settings.value = await loadSettings();
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function refreshDevices() {
  loadingDevices.value = true;
  devicesError.value = "";
  try {
    devices.value = await adbDevices();
    if (!devices.value.length) {
      selected.value = "";
      webviewVersion.value = "";
      return;
    }
    if (!devices.value.some((d) => d.serial === selected.value)) {
      selected.value = devices.value[0].serial;
    }
    await probeVersion();
    await detectInstalled();
  } catch (e) {
    devicesError.value = String(e);
    devices.value = [];
  } finally {
    loadingDevices.value = false;
  }
}

async function probeVersion() {
  webviewVersion.value = "";
  versionError.value = "";
  if (!selected.value) return;
  try {
    webviewVersion.value = await adbWebviewVersion(selected.value);
  } catch (e) {
    versionError.value = String(e);
  }
}

function pickDevice(serial: string) {
  if (selected.value === serial) return;
  selected.value = serial;
  devtools.value = null;
  void probeVersion();
  void detectInstalled();
}

async function install() {
  if (!debugApk.value || !selected.value) return;
  installing.value = true;
  installMessage.value = "";
  try {
    installMessage.value = await adbInstallApk(selected.value, debugApk.value.apkPath);
    ElMessage.success("安装完成");
    target.value = "debug";
    await detectInstalled();
  } catch (e) {
    installMessage.value = String(e);
    ElMessage.error("安装失败");
  } finally {
    installing.value = false;
  }
}

async function launch() {
  if (!selected.value || !targetPackage.value) return;
  launching.value = true;
  try {
    await adbLaunchApp(selected.value, targetPackage.value);
    ElMessage.success("已启动");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    launching.value = false;
  }
}

/** 把前端 dist 推到设备（files/www），省去重新打包 APK；应用重启后生效 */
async function pushDist() {
  if (!selected.value || !targetPackage.value || !distDir.value) return;
  if (!targetInstalled.value) {
    ElMessage.warning(`设备上没有安装 ${targetPackage.value}，先安装再推送`);
    return;
  }
  pushing.value = true;
  try {
    await adbPushWww(selected.value, targetPackage.value, distDir.value);
    await adbLaunchApp(selected.value, targetPackage.value);
    ElMessage.success(`已推送到 ${targetPackage.value} 并重启`);
    await pullLogs();
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    pushing.value = false;
  }
}

async function connectDevtools() {
  if (!selected.value || !targetPackage.value) return;
  devtoolsBusy.value = true;
  try {
    devtools.value = await devtoolsConnect(selected.value, targetPackage.value);
    await copyToClipboard(devtools.value.devtoolsUrl);
    ElMessage.success("地址已复制，粘到浏览器地址栏回车即可");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    devtoolsBusy.value = false;
  }
}

async function copyDevtoolsUrl() {
  if (!devtools.value) return;
  try {
    await copyToClipboard(devtools.value.devtoolsUrl);
    ElMessage.success("地址已复制");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

/** 指定面板直达：panel=console / network，省去默认的 Elements 同步 */
async function copyPanelUrl(panel: string) {
  if (!devtools.value) return;
  try {
    await copyToClipboard(`${devtools.value.devtoolsUrl}&panel=${panel}`);
    ElMessage.success(`已复制（直开 ${panel} 面板）`);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function toggleKeepScreen(value: boolean) {
  if (!selected.value) return;
  try {
    await adbKeepScreen(selected.value, value);
    ElMessage.success(value ? "设备已设为常亮" : "已恢复系统息屏");
  } catch (e) {
    keepScreen.value = !value;
    ElMessage.error(String(e));
  }
}

async function disconnectDevtools() {
  const serial = selected.value;
  devtools.value = null;
  if (!serial) return;
  try {
    await devtoolsDisconnect(serial);
  } catch {
    // 断开失败不影响界面
  }
}

async function pullLogs() {
  if (!selected.value || logLoading.value) return;
  logLoading.value = true;
  // 更新前先测：用户此刻是不是停在底部（不能靠 scroll 事件，文本整体替换会打乱状态）
  const box = logBox.value;
  const wasAtBottom = box
    ? box.scrollHeight - box.scrollTop - box.clientHeight < 40
    : true;
  const prevTop = box?.scrollTop ?? 0;
  try {
    logText.value = await adbLogcatTail(selected.value, 300, logFilter.value);
    await nextTick();
    const el = logBox.value;
    if (!el) return;
    if (wasAtBottom) {
      el.scrollTop = el.scrollHeight;
      stickToBottom.value = true;
    } else {
      // 用户在看历史：原样还原位置，完全不动
      el.scrollTop = prevTop;
      stickToBottom.value = false;
    }
  } catch (e) {
    logText.value = `读取日志失败：${String(e)}`;
  } finally {
    logLoading.value = false;
  }
}

function scrollLogsToBottom() {
  const box = logBox.value;
  if (!box) return;
  box.scrollTop = box.scrollHeight;
  stickToBottom.value = true;
}

/** 仅用于「回到最新」按钮的显隐 */
function onLogScroll() {
  const box = logBox.value;
  if (!box) return;
  stickToBottom.value = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
}

async function clearLogs() {
  if (!selected.value) return;
  try {
    await adbLogcatClear(selected.value);
    logText.value = "";
    ElMessage.success("设备日志已清空");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

function goBack() {
  router.push({ name: "project", params: { id: props.projectId } });
}

/** 项目配置里是否勾了「调试流量日志」（本地服务未启用时日志也不会产生） */
const trafficLogEnabled = computed(() => {
  const cfg = android.value?.localServer;
  return !!cfg?.enabled && !!cfg.debugLog;
});
const pullingLogs = ref(false);

async function pullTrafficLogs() {
  if (!current.value) {
    ElMessage.warning("先在上面选一台设备");
    return;
  }
  if (target.value !== "debug") {
    ElMessage.warning("流量日志只写在调试包里，先把目标包切到调试包");
    return;
  }
  if (!debugPackage.value) {
    ElMessage.warning("项目还没配置包名");
    return;
  }
  pullingLogs.value = true;
  try {
    const dir = await adbPullLogs(selected.value, debugPackage.value);
    ElMessage.success("流量日志已拉到本机");
    await revealItemInDir(dir);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    pullingLogs.value = false;
  }
}

onMounted(async () => {
  await loadConfig();
  await refreshDevices();
  await pullLogs();
  timer = window.setInterval(() => {
    if (autoRefresh.value) void pullLogs();
  }, 2500);
});

onUnmounted(() => {
  if (timer !== null) {
    window.clearInterval(timer);
    timer = null;
  }
  void disconnectDevtools();
});
</script>

<template>
  <div class="debug-page">
    <header class="head">
      <button class="back-btn" type="button" title="返回项目" @click="goBack">
        <el-icon :size="15"><ArrowLeft /></el-icon>
      </button>
      <div class="head-title">
        <h1 class="title">设备调试 · {{ appName || "未命名项目" }}</h1>
        <p class="subtitle">
          <span class="pkg-chip" :class="target">{{ target === "debug" ? "调试包" : "正式包" }}</span>
          {{ targetPackage || "未配置包名" }}
        </p>
      </div>
      <div class="head-actions">
        <el-button
          v-if="canPush"
          type="primary"
          :disabled="!current"
          :loading="pushing"
          @click="pushDist"
        >
          {{ target === "debug" ? "重新推送到调试包" : "重新推送到正式包" }}
        </el-button>
      </div>
    </header>

    <el-scrollbar class="page-body">
      <section class="card">
        <div class="step">1</div>
        <div class="card-main">
          <div class="card-head">
            <span class="card-title">目标包</span>
            <span v-if="current" class="pack-state" :class="{ ok: targetInstalled }">
              {{ targetInstalled ? "设备上已安装" : "设备上没装这个包" }}
            </span>
          </div>
          <el-segmented
            v-model="target"
            block
            class="pack-seg"
            :options="[
              { label: '正式包', value: 'release' },
              { label: '调试包', value: 'debug' },
            ]"
          />
          <div class="kv-row spaced">
            <span class="kv-key">包名</span>
            <span class="kv-value mono">{{ targetPackage || "未配置包名" }}</span>
          </div>
          <div class="kv-row">
            <span class="kv-key">前端 dist</span>
            <span class="kv-value mono">{{ distDir || "未配置" }}</span>
          </div>
          <p class="hint">
            正式包只有构建时勾了「WebView 调试」才能被 DevTools 连上；调试包包名带
            <code>.debug</code>，与正式包可同时安装。
          </p>
          <template v-if="debugApk">
            <div class="kv-row">
              <span class="kv-key">调试包产物</span>
              <span class="kv-value mono">{{ debugApk.apkPath }}</span>
            </div>
            <div class="kv-row">
              <span class="kv-key">版本</span>
              <span class="kv-value">
                {{ debugApk.versionName }} ({{ debugApk.versionCode }}) ·
                {{ (debugApk.sizeBytes / 1024 / 1024).toFixed(1) }} MB ·
                {{ new Date(debugApk.createdAt).toLocaleString() }}
              </span>
            </div>
          </template>
        </div>
      </section>

      <section class="card">
        <div class="step">2</div>
        <div class="card-main">
          <div class="card-head">
            <span class="card-title">选择设备</span>
            <el-button size="small" :loading="loadingDevices" @click="refreshDevices">
              刷新
            </el-button>
          </div>
          <div v-if="devicesError" class="error-box">{{ devicesError }}</div>
          <div v-else-if="!devices.length" class="empty">
            没有检测到设备：确认 USB 调试已开启、数据线可传数据。
          </div>
          <div v-else class="device-list">
            <div
              v-for="device in devices"
              :key="device.serial"
              class="device"
              :class="{ active: device.serial === selected }"
              @click="pickDevice(device.serial)"
            >
              <div>
                <div class="device-model">{{ device.model || device.serial }}</div>
                <div class="device-serial">{{ device.serial }}</div>
              </div>
              <span class="device-state" :class="device.state">{{ device.state }}</span>
            </div>
          </div>
          <div v-if="current" class="kv-row spaced">
            <span class="kv-key">System WebView</span>
            <span class="kv-value mono">
              {{ webviewVersion || versionError || "读取中…" }}
              <template v-if="chromeMajor"> → 建议 Chrome {{ chromeMajor }} 或同代</template>
            </span>
          </div>
          <div v-if="current" class="keep-screen">
            <el-switch v-model="keepScreen" size="small" @change="toggleKeepScreen" />
            <span>屏幕常亮（锁屏后 CDP 会被节流，看着像卡死）</span>
          </div>
        </div>
      </section>

      <section class="card">
        <div class="step">3</div>
        <div class="card-main">
          <div class="card-title">安装并启动</div>
          <div class="actions">
            <el-button
              v-if="debugApk && target === 'debug'"
              type="primary"
              :disabled="!current"
              :loading="installing"
              @click="install"
            >
              {{ installing ? "安装中…" : "安装调试包到设备" }}
            </el-button>
            <el-button :disabled="!current || !targetPackage" :loading="launching" @click="launch">
              启动应用
            </el-button>
          </div>
          <div v-if="!current" class="empty">先在上面选一台设备。</div>
          <div v-else-if="!targetInstalled" class="empty">
            设备上还没有「{{ target === "debug" ? "调试包" : "正式包" }}」：先装一次再启动。
          </div>
          <div v-if="installMessage" class="install-msg mono">{{ installMessage }}</div>
        </div>
      </section>

      <section class="card">
        <div class="step">4</div>
        <div class="card-main">
          <div class="card-title">完整 DevTools</div>
          <div class="actions">
            <el-button
              type="primary"
              :disabled="!current"
              :loading="devtoolsBusy"
              @click="connectDevtools"
            >
              {{ devtools ? "重新连接并复制地址" : "连接并复制地址" }}
            </el-button>
            <el-button v-if="devtools" @click="copyDevtoolsUrl">复制地址</el-button>
            <el-button v-if="devtools" @click="copyPanelUrl('console')">Console 直达</el-button>
            <el-button v-if="devtools" @click="copyPanelUrl('network')">Network 直达</el-button>
            <el-button v-if="devtools" @click="disconnectDevtools">断开</el-button>
          </div>
          <div v-if="devtools" class="devtools-box">
            <div class="kv-row">
              <span class="kv-key">目标页面</span>
              <span class="kv-value mono">{{ devtools.title }} · {{ devtools.url }}</span>
            </div>
            <div class="devtools-url mono">{{ devtools.devtoolsUrl }}</div>
          </div>
          <p class="hint">
            点上面的按钮会复制一个 <code>devtools://</code> 地址；粘到本机 Chromium 浏览器
            地址栏回车，即可用浏览器自带前端调试这台设备的页面（Elements / Console / Network /
            Sources），不依赖 appspot，离线可用。
          </p>
          <p class="hint">
            卡顿说明：远程 CDP 全部流量挤在一条 adb 通道上，同时页面会进入调试态（V8 限制 JIT 优化），
            叠加 Elements/Network 的实时同步。**同时只开一个面板**会明显变顺：查日志用「Console 直达」，
            查请求用「Network 直达」，别让 Elements 一直挂着。
          </p>
        </div>
      </section>

      <section class="card">
        <div class="step">5</div>
        <div class="card-main">
          <div class="card-head">
            <span class="card-title">设备日志</span>
            <div class="log-tools">
              <el-input
                v-model="logFilter"
                size="small"
                placeholder="过滤关键字"
                clearable
                class="log-filter"
                @change="pullLogs"
              />
              <el-checkbox v-model="autoRefresh" size="small">自动刷新</el-checkbox>
              <el-button size="small" @click="pullLogs">刷新</el-button>
              <el-button size="small" @click="clearLogs">清空</el-button>
            </div>
          </div>
          <div class="log-wrap">
            <div ref="logBox" class="log-body" @scroll="onLogScroll">{{ logText || "暂无日志" }}</div>
            <el-button
              v-if="!stickToBottom"
              class="log-latest"
              size="small"
              type="primary"
              @click="scrollLogsToBottom()"
            >
              回到最新
            </el-button>
          </div>
        </div>
      </section>

      <section class="card">
        <div class="step">6</div>
        <div class="card-main">
          <div class="card-head">
            <span class="card-title">流量日志</span>
            <span class="pack-state" :class="{ ok: trafficLogEnabled }">
              {{ trafficLogEnabled ? "已开启" : "未开启" }}
            </span>
          </div>
          <div class="actions">
            <el-button
              type="primary"
              :disabled="!current || target !== 'debug' || !trafficLogEnabled"
              :loading="pullingLogs"
              @click="pullTrafficLogs"
            >
              {{ pullingLogs ? "拉取中…" : "拉取到本机并打开" }}
            </el-button>
          </div>
          <div v-if="!trafficLogEnabled" class="empty">
            先在项目页开启调试流量日志，再重新打调试包。
          </div>
          <div v-else-if="target !== 'debug'" class="empty">
            日志只在调试包里，把目标包切到调试包。
          </div>
          <p class="hint">
            日志在设备的
            <code>Android/data/{{ debugPackage }}/files/jianling-logs/</code>，
            包含请求记录和上下行音频。正式包不写日志。
          </p>
        </div>
      </section>
    </el-scrollbar>
  </div>
</template>

<style scoped>
.debug-page {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 18px;
  border-bottom: 1px solid var(--border-subtle);
}

.back-btn {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
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

.head-actions {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
  margin-left: auto;
}

.title {
  margin: 0;
  font-size: 16px;
  font-weight: 650;
  color: var(--text-primary);
}

.subtitle {
  margin: 2px 0 0;
  font-size: 11.5px;
  color: var(--text-faint);
  word-break: break-all;
}

.pkg-chip {
  display: inline-block;
  margin-right: 6px;
  padding: 1px 7px;
  border-radius: 999px;
  background: var(--accent);
  color: #fff;
  font-size: 11px;
  font-weight: 600;
}

.pkg-chip.release {
  background: #5b6b7c;
}

.page-body {
  flex: 1;
  min-height: 0;
}

.card {
  display: flex;
  gap: 12px;
  margin-bottom: 12px;
  padding: 14px 16px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  background: var(--surface-solid);
}

.step {
  flex: none;
  width: 22px;
  height: 22px;
  border-radius: 999px;
  background: var(--accent);
  color: #fff;
  font-size: 12px;
  line-height: 22px;
  text-align: center;
}

.card-main {
  flex: 1;
  min-width: 0;
}

.card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 10px;
}

.card-title {
  margin-bottom: 8px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
}

.card-head .card-title {
  margin-bottom: 0;
}

/* 目标包切换：与项目页分段器同一套观感 */
.pack-seg {
  margin-bottom: 10px;
  --el-segmented-item-selected-bg-color: var(--accent);
  --el-segmented-item-selected-color: #fff;
}

.pack-state {
  font-size: 11.5px;
  color: var(--warning);
}

.pack-state.ok {
  color: var(--success, #34a853);
}

.kv-row {
  display: flex;
  gap: 12px;
  margin-top: 4px;
  font-size: 12.5px;
}

.kv-row.spaced {
  margin-top: 12px;
}

.keep-screen {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.kv-key {
  flex: 0 0 110px;
  color: var(--text-muted);
}

.kv-value {
  color: var(--text-primary);
  word-break: break-all;
}

.mono {
  font-family: var(--font-mono);
  font-size: 11.5px;
}

.actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.install-msg {
  margin-top: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-md);
  background: var(--surface-sunken);
  color: var(--text-muted);
  word-break: break-all;
}

.devtools-box {
  margin-top: 10px;
}

.devtools-url {
  margin-top: 8px;
  padding: 8px 10px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-sunken);
  color: #52606f;
  word-break: break-all;
}

.hint {
  margin: 10px 0 0;
  font-size: 11.5px;
  line-height: 1.7;
  color: var(--text-faint);
}

.device-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.device {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
  cursor: pointer;
}

.device.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent) inset;
}

.device-model {
  font-size: 13px;
  color: var(--text-primary);
}

.device-serial {
  margin-top: 2px;
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--text-faint);
}

.device-state {
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--surface-sunken);
  font-size: 11.5px;
  color: var(--text-muted);
}

.device-state.device {
  background: rgba(46, 125, 50, 0.12);
  color: #2e7d32;
}

.device-state.unauthorized,
.device-state.offline {
  background: rgba(214, 82, 82, 0.12);
  color: #d65252;
}

.empty {
  padding: 8px 0;
  font-size: 12.5px;
  color: var(--text-faint);
}

.error-box {
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: rgba(214, 82, 82, 0.08);
  color: #b23c3c;
  font-size: 12px;
  word-break: break-all;
}

.log-tools {
  display: flex;
  align-items: center;
  gap: 8px;
}

.log-filter {
  width: 180px;
}

.log-body {
  height: min(40vh, 360px);
  overflow: auto;
  padding: 10px 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-sunken);
  font-family: var(--font-mono);
  font-size: 11.5px;
  line-height: 1.6;
  color: #52606f;
  white-space: pre-wrap;
  word-break: break-all;
}

.log-wrap {
  position: relative;
}

.log-latest {
  position: absolute;
  right: 12px;
  bottom: 12px;
  box-shadow: 0 2px 8px rgba(15, 23, 42, 0.18);
}
</style>
