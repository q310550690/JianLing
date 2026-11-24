<script setup lang="ts">
import { computed, inject, onMounted, onUnmounted, ref } from "vue";
import { ElMessage } from "element-plus";
import { Refresh } from "@element-plus/icons-vue";
import { open } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { checkEnvironment, deleteWindowsKernel, downloadWindowsKernel, fetchWindowsKernelVersions, importWindowsKernel, installJdk, installSdk, loadSettings, saveSettings } from "../api";
import type { InstallProgress, KernelProgress, KernelVersion, Settings, WindowsKernel } from "../api";
import type { EnvReport, EnvStatus } from "../types";

const resetScroll = inject<() => void>("resetScroll", () => {});
const ready = ref(false);

const report = ref<EnvReport | null>(null);
const loading = ref(false);
const loaded = ref<Settings | null>(null);
const form = ref({ jdkPath: "", sdkPath: "", adbPath: "" });

const mirror = ref(true);

interface Job {
  running: boolean;
  percent: number;
  message: string;
}

const jobs = ref<Record<"jdk" | "sdk", Job>>({
  jdk: { running: false, percent: 0, message: "" },
  sdk: { running: false, percent: 0, message: "" },
});

let unlisten: UnlistenFn | null = null;

/* ===== Windows 内核（WebView2 固定版本运行时）===== */
const kernels = computed(() => loaded.value?.windowsKernels ?? []);
const versions = ref<KernelVersion[]>([]);
const versionLoading = ref(false);
const selectedVersion = ref("");
const kernelJob = ref({ running: false, percent: 0, message: "" });
let unlistenKernel: UnlistenFn | null = null;

function mb(bytes: number) {
  return (bytes / 1024 / 1024).toFixed(0);
}

async function saveKernels(list: WindowsKernel[]) {
  const base = loaded.value;
  if (!base) return;
  const next: Settings = { ...base, windowsKernels: list };
  await saveSettings(next);
  loaded.value = next;
}

async function loadVersions() {
  if (versionLoading.value) return;
  versionLoading.value = true;
  try {
    versions.value = await fetchWindowsKernelVersions();
    if (!selectedVersion.value && versions.value.length) {
      selectedVersion.value = versions.value[0].version;
    }
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    versionLoading.value = false;
  }
}

async function downloadKernel() {
  if (kernelJob.value.running) return;
  const target =
    versions.value.find((item) => item.version === selectedVersion.value) ?? versions.value[0];
  if (!target) {
    ElMessage.warning("先点「获取版本」拿到可下载列表");
    return;
  }
  kernelJob.value = { running: true, percent: 0, message: `准备下载 ${target.version}…` };
  try {
    const kernel = await downloadWindowsKernel(target.version, target.url);
    const list = [
      ...(loaded.value?.windowsKernels ?? []).filter((item) => item.path !== kernel.path),
      kernel,
    ];
    await saveKernels(list);
    kernelJob.value = { running: false, percent: 100, message: `已就绪：${kernel.version}` };
    ElMessage.success(`内核 ${kernel.version} 已就绪`);
  } catch (e) {
    kernelJob.value = { running: false, percent: 0, message: String(e) };
    ElMessage.error(String(e));
  }
}

async function importKernel() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "选择 WebView2 固定版本运行时目录",
  });
  if (typeof selected !== "string") return;
  try {
    const kernel = await importWindowsKernel(selected);
    const list = [
      ...(loaded.value?.windowsKernels ?? []).filter((item) => item.path !== kernel.path),
      kernel,
    ];
    await saveKernels(list);
    ElMessage.success(`已导入 ${kernel.version}`);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function removeKernel(item: WindowsKernel) {
  try {
    await deleteWindowsKernel(item.id, item.path);
    await saveKernels((loaded.value?.windowsKernels ?? []).filter((k) => k.id !== item.id));
    ElMessage.success("已删除");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

const statusText: Record<EnvStatus, string> = {
  ok: "正常",
  warn: "待处理",
  error: "缺失",
};const summary = ref({ ok: 0, warn: 0, error: 0 });

function refreshSummary() {
  const items = report.value?.items ?? [];
  summary.value = {
    ok: items.filter((i) => i.status === "ok").length,
    warn: items.filter((i) => i.status === "warn").length,
    error: items.filter((i) => i.status === "error").length,
  };
}

async function pickDir(key: "jdkPath" | "sdkPath") {
  const selected = await open({ directory: true, multiple: false, title: "选择目录" });
  if (typeof selected === "string") {
    form.value[key] = selected;
    await runCheck();
  }
}

async function pickExe(key: "adbPath", title: string) {
  const selected = await open({
    multiple: false,
    title,
    filters: [{ name: "可执行文件", extensions: ["exe"] }],
  });
  if (typeof selected === "string") {
    form.value[key] = selected;
    await savePaths();
  }
}

/** adb 路径改完即保存，避免忘记点「保存并重新检测」 */
async function savePaths() {
  try {
    const base = loaded.value ?? (await loadSettings());
    const next: Settings = {
      ...base,
      adbPath: form.value.adbPath.trim() || null,
    };
    await saveSettings(next);
    loaded.value = next;
    ElMessage.success("调试路径已保存");
  } catch (e) {
    ElMessage.error(`保存失败：${String(e)}`);
  }
}

/** 检测结果里对应项正常，就说明这个目录里已经装好了，一键安装置灰。 */
function installed(id: string) {
  return (report.value?.items ?? []).some((item) => item.id === id && item.status === "ok");
}

const jdkInstalled = computed(() => installed("jdk"));
const sdkInstalled = computed(() => installed("sdk"));

async function runCheck() {
  loading.value = true;
  try {
    report.value = await checkEnvironment({
      jdkPath: form.value.jdkPath.trim() || null,
      sdkPath: form.value.sdkPath.trim() || null,
    });
    refreshSummary();
  } catch (e) {
    ElMessage.error(`检测失败：${String(e)}`);
  } finally {
    loading.value = false;
  }
}

async function persist() {
  try {
    const next: Settings = {
      jdkPath: form.value.jdkPath.trim() || null,
      sdkPath: form.value.sdkPath.trim() || null,
      adbPath: form.value.adbPath.trim() || null,
      projects: loaded.value?.projects ?? [],
      windowsKernels: loaded.value?.windowsKernels ?? [],
    };
    await saveSettings(next);
    loaded.value = next;
    ElMessage.success("环境配置已保存");
    await runCheck();
  } catch (e) {
    ElMessage.error(`保存失败：${String(e)}`);
  }
}

async function runInstall(kind: "jdk" | "sdk") {
  const job = jobs.value[kind];
  if (job.running) return;
  jobs.value[kind] = { running: true, percent: 0, message: "准备下载…" };
  try {
    if (kind === "jdk") {
      const path = await installJdk(form.value.jdkPath.trim(), mirror.value);
      form.value.jdkPath = path;
      jobs.value.jdk = { running: false, percent: 100, message: "JDK 安装完成" };
      ElMessage.success(`JDK 已安装到 ${path}`);
    } else {
      if (!form.value.jdkPath.trim()) {
        throw new Error("请先安装或选择 JDK");
      }
      const path = await installSdk(
        form.value.sdkPath.trim(),
        form.value.jdkPath.trim(),
        mirror.value,
      );
      form.value.sdkPath = path;
      jobs.value.sdk = { running: false, percent: 100, message: "Android SDK 安装完成" };
      ElMessage.success(`SDK 已安装到 ${path}`);
    }
    await persist();
  } catch (e) {
    jobs.value[kind] = { running: false, percent: 0, message: String(e) };
    ElMessage.error(String(e));
  }
}

onMounted(async () => {
  unlisten = await listen<InstallProgress>("install-progress", (event) => {
    const { kind, percent, message } = event.payload;
    if (kind === "jdk" || kind === "sdk") {
      jobs.value[kind] = { running: true, percent, message };
    }
  });
  unlistenKernel = await listen<KernelProgress>("kernel-progress", (event) => {
    const { stage, received, total } = event.payload;
    const percent = total > 0 ? Math.min(99, Math.round((received / total) * 100)) : 0;
    if (stage === "download") {
      kernelJob.value = {
        running: true,
        percent,
        message: `下载中 ${mb(received)} / ${mb(total)} MB`,
      };
    } else if (stage === "extract") {
      kernelJob.value = { running: true, percent: 99, message: "解压中…" };
    }
  });
  try {
    const settings = await loadSettings();
    loaded.value = settings;
    form.value = {
      jdkPath: settings.jdkPath ?? "",
      sdkPath: settings.sdkPath ?? "",
      adbPath: settings.adbPath ?? "",
    };
  } catch {
    ElMessage.warning("读取配置失败，使用默认值");
  }
  await runCheck();
  ready.value = true;
  resetScroll();
});

onUnmounted(() => {
  unlisten?.();
  unlistenKernel?.();
});
</script>

<template>
  <div v-if="ready" class="page page-frame">
    <header class="page-head">
      <div class="head-left">
        <div>
          <h1>设置</h1>
          <p>配置打包环境（JDK 与 Android SDK）。</p>
        </div>
      </div>
      <el-button :icon="Refresh" :loading="loading" @click="runCheck">重新检测</el-button>
    </header>

    <el-scrollbar class="page-body">
    <section class="card">
      <div class="card-head">
        <span class="card-title">环境状态</span>
        <div class="chips">
          <span class="chip"><i class="dot ok"></i>正常 {{ summary.ok }}</span>
          <span class="chip"><i class="dot warn"></i>待处理 {{ summary.warn }}</span>
          <span class="chip"><i class="dot error"></i>缺失 {{ summary.error }}</span>
        </div>
      </div>

      <div v-loading="loading" class="env-list">
        <div v-for="item in report?.items ?? []" :key="item.id" class="env-row">
          <i class="dot" :class="item.status" :title="statusText[item.status]"></i>
          <div class="env-main">
            <div class="env-label">{{ item.label }}</div>
            <div class="env-value">{{ item.value }}</div>
            <div v-if="item.hint" class="env-hint">{{ item.hint }}</div>
          </div>
          <div class="env-path mono" :title="item.path ?? ''">{{ item.path ?? "—" }}</div>
        </div>
        <div v-if="!report?.items?.length && !loading" class="env-empty">尚未检测</div>
      </div>
    </section>

    <section class="card">
      <div class="card-head">
        <span class="card-title">环境与路径</span>
        <div class="mirror-toggle">
          <span class="mirror-label">国内镜像</span>
          <el-switch v-model="mirror" size="small" />
        </div>
      </div>

      <div class="fields">
        <div class="field span-2">
          <span class="field-label">JDK 目录</span>
          <div class="input-row">
            <el-input v-model="form.jdkPath" placeholder="留空则自动读取 JAVA_HOME" clearable />
            <el-button @click="pickDir('jdkPath')">选择</el-button>
            <el-button
              :disabled="jdkInstalled || jobs.jdk.running"
              :loading="jobs.jdk.running"
              @click="runInstall('jdk')"
            >
              {{ jdkInstalled ? "已安装" : "一键安装" }}
            </el-button>
          </div>
          <span class="field-note">
            约 180 MB（Microsoft OpenJDK 21），装进上面的目录；留空则装到应用数据目录。
          </span>
          <el-progress
            v-if="jobs.jdk.running || jobs.jdk.percent > 0"
            class="install-progress"
            :percentage="jobs.jdk.percent"
            :status="jobs.jdk.running ? undefined : 'success'"
          />
          <div v-if="jobs.jdk.message" class="install-msg">{{ jobs.jdk.message }}</div>
        </div>

        <div class="field span-2">
          <span class="field-label">Android SDK</span>
          <div class="input-row">
            <el-input
              v-model="form.sdkPath"
              placeholder="留空则读取 ANDROID_HOME / 常见安装位置"
              clearable
            />
            <el-button @click="pickDir('sdkPath')">选择</el-button>
            <el-button
              :disabled="sdkInstalled || jobs.sdk.running"
              :loading="jobs.sdk.running"
              @click="runInstall('sdk')"
            >
              {{ sdkInstalled ? "已安装" : "一键安装" }}
            </el-button>
          </div>
          <span class="field-note">
            cmdline-tools 约 130 MB，并装 platform-tools / android-34 / build-tools 34.0.0。
          </span>
          <el-progress
            v-if="jobs.sdk.running || jobs.sdk.percent > 0"
            class="install-progress"
            :percentage="jobs.sdk.percent"
            :status="jobs.sdk.running ? undefined : 'success'"
          />
          <div v-if="jobs.sdk.message" class="install-msg">{{ jobs.sdk.message }}</div>
        </div>

        <div class="field span-2">
          <span class="field-label">adb 路径（调试用，可选）</span>
          <div class="input-row">
            <el-input v-model="form.adbPath" placeholder="留空自动探测 SDK/platform-tools 与 PATH" clearable @change="savePaths" />
            <el-button @click="pickExe('adbPath', '选择 adb.exe')">选择</el-button>
          </div>
        </div>
      </div>

      <p class="card-note">
        选择目录后会自动重新检测：目录里已有可用组件时一键安装置灰。需要先有可用的 JDK 才能安装 SDK；
        安装完成后路径会自动写入并保存，国内镜像失败时回退官方源。
      </p>

      <div class="actions">
        <el-button type="primary" @click="persist">保存并重新检测</el-button>
        <el-button @click="runCheck">仅重新检测</el-button>
      </div>
    </section>

    <section class="card">
      <div class="card-head">
        <span class="card-title">SDK 组件</span>
      </div>
      <div class="tag-row">
        <span class="tag-label mono">platforms</span>
        <template v-if="report?.sdkPlatforms.length">
          <span v-for="name in report.sdkPlatforms" :key="name" class="tag mono">{{ name }}</span>
        </template>
        <span v-else class="tag-empty">未检测到</span>
      </div>
      <div class="tag-row">
        <span class="tag-label mono">build-tools</span>
        <template v-if="report?.sdkBuildTools.length">
          <span v-for="name in report.sdkBuildTools" :key="name" class="tag mono">{{ name }}</span>
        </template>
        <span v-else class="tag-empty">未检测到</span>
      </div>
    </section>

    <section class="card">
      <div class="card-head">
        <span class="card-title">Windows 内核</span>
        <span class="card-hint">WebView2 固定版本运行时，项目里可选是否内置</span>
      </div>
      <div class="fields">
        <div class="field span-2">
          <span class="field-label">下载内核</span>
          <div class="input-row">
            <el-select
              v-model="selectedVersion"
              :loading="versionLoading"
              placeholder="先获取可下载版本"
              style="flex: 1"
            >
              <el-option
                v-for="item in versions"
                :key="item.version"
                :value="item.version"
                :label="item.version"
              />
            </el-select>
            <el-button :loading="versionLoading" @click="loadVersions">获取版本</el-button>
            <el-button
              type="primary"
              :loading="kernelJob.running"
              :disabled="!versions.length"
              @click="downloadKernel"
            >
              下载
            </el-button>
            <el-button @click="importKernel">导入本地目录</el-button>
          </div>
          <span class="field-note">
            来源为微软 WebView2 固定版本运行时（x64，约 300 MB），解压后约 680 MB 存到应用数据目录；也可以导入已解压、含
            msedgewebview2.exe 的目录。
          </span>
          <el-progress
            v-if="kernelJob.running || kernelJob.percent > 0"
            class="install-progress"
            :percentage="kernelJob.percent"
            :status="kernelJob.running ? undefined : 'success'"
          />
          <div v-if="kernelJob.message" class="install-msg">{{ kernelJob.message }}</div>
        </div>
      </div>

      <div v-if="kernels.length" class="kernel-list">
        <div v-for="item in kernels" :key="item.id" class="kernel-row">
          <span class="kernel-ver mono">{{ item.version }}</span>
          <span class="kernel-tag">{{ item.source === "auto" ? "下载" : "导入" }}</span>
          <span class="kernel-size mono">{{ mb(item.sizeBytes) }} MB</span>
          <span class="kernel-path mono" :title="item.path">{{ item.path }}</span>
          <el-button size="small" type="danger" plain @click="removeKernel(item)">删除</el-button>
        </div>
      </div>
      <p v-else class="card-note">还没有内核，可以先下载指定版本，或导入本地已解压的运行时目录。</p>
    </section>
    </el-scrollbar>
  </div>
</template>

<style scoped>
.head-left {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

h1 {
  margin: 0;
  font-size: 22px;
  font-weight: 650;
  letter-spacing: -0.02em;
}

.page-head p {
  margin: 6px 0 0;
  font-size: 12.5px;
  color: var(--text-muted);
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
  margin-bottom: 12px;
}

.card-title {
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--text-muted);
}

.chips {
  display: flex;
  gap: 6px;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 9px;
  border-radius: 999px;
  background: var(--surface-sunken);
  font-size: 11.5px;
  color: var(--text-secondary);
}

.dot {
  width: 6px;
  height: 6px;
  flex: 0 0 6px;
  border-radius: 50%;
  background: #b6bec9;
}

.dot.ok {
  background: var(--accent);
}

.dot.warn {
  background: var(--warning);
}

.dot.error {
  background: var(--danger);
}

.env-list {
  display: flex;
  flex-direction: column;
}

.env-row {
  display: flex;
  align-items: flex-start;
  gap: 11px;
  padding: 10px 10px;
  margin: 0 -10px;
  border-radius: 9px;
  border-bottom: 1px solid var(--border-subtle);
  transition: background-color 0.16s var(--ease);
}

.env-row:hover {
  background: var(--surface-sunken);
}

.env-row:last-child {
  border-bottom: none;
}

.env-row > .dot {
  margin-top: 6px;
}

.env-main {
  flex: 1;
  min-width: 0;
}

.env-label {
  font-size: 13.5px;
  font-weight: 500;
  color: var(--text-primary);
}

.env-value {
  margin-top: 2px;
  font-size: 12.5px;
  color: var(--text-secondary);
  word-break: break-word;
}

.env-hint {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--warning);
}

.env-path {
  flex: 0 0 42%;
  max-width: 42%;
  text-align: right;
  font-size: 11.5px;
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.env-empty {
  padding: 14px 0;
  font-size: 12.5px;
  color: var(--text-faint);
}

.tag-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  padding: 5px 0;
}

.tag-label {
  width: 88px;
  flex: 0 0 88px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.tag {
  padding: 2px 8px;
  border-radius: 6px;
  background: var(--surface-sunken);
  font-size: 11px;
  color: var(--text-secondary);
}

.tag-empty {
  font-size: 11.5px;
  color: var(--text-faint);
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

.mirror-toggle {
  display: flex;
  align-items: center;
  gap: 7px;
}

.mirror-label {
  font-size: 12px;
  color: var(--text-muted);
}

.install-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 11px 13px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
}

.install-row + .install-row {
  margin-top: 12px;
}

.install-main {
  min-width: 0;
}

.install-name {
  font-size: 13px;
  color: var(--text-primary);
}

.install-desc {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.install-progress {
  margin-top: 8px;
}

.install-msg {
  margin-top: 6px;
  font-size: 11.5px;
  color: var(--text-faint);
  word-break: break-all;
}

.card-note {
  margin: 12px 0 0;
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--text-faint);
}

.field-label {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-secondary);
}

.actions {
  display: flex;
  gap: 8px;
  margin-top: 16px;
}

.kernel-list {
  margin-top: 14px;
  border-top: 1px dashed var(--border-subtle);
}

.kernel-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 0;
  border-bottom: 1px solid var(--border-subtle);
}

.kernel-ver {
  flex: 0 0 auto;
  font-size: 12px;
  color: var(--text-primary);
}

.kernel-tag {
  flex: 0 0 auto;
  padding: 1px 7px;
  border-radius: 999px;
  background: var(--surface-input);
  font-size: 11px;
  color: var(--text-secondary);
}

.kernel-size {
  flex: 0 0 auto;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.kernel-path {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--text-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

</style>
