<script setup lang="ts">
import { onActivated, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { ElMessage, ElMessageBox } from "element-plus";
import { Plus, CopyDocument, Delete, EditPen, FolderOpened } from "@element-plus/icons-vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { loadSettings, newProject, saveSettings } from "../api";
import type { Project, Settings } from "../api";

const router = useRouter();

function open(id: string) {
  router.push({ name: "project", params: { id } });
}

const AVATAR_COLORS = [
  "#15803d",
  "#0e7490",
  "#7c3aed",
  "#b45309",
  "#be123c",
  "#1d4ed8",
  "#0f766e",
  "#9333ea",
];

const projects = ref<Project[]>([]);
const settings = ref<Settings | null>(null);
const loading = ref(false);
const createVisible = ref(false);
const createName = ref("");
const failedIcons = ref<Set<string>>(new Set());

function sortList(list: Project[]) {
  return [...list].sort((a, b) => b.updatedAt - a.updatedAt);
}

function hashSeed(seed: string) {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function avatarBg(project: Project) {
  const seed = project.id || project.name || "app";
  return AVATAR_COLORS[hashSeed(seed) % AVATAR_COLORS.length];
}

function initial(name: string) {
  const trimmed = (name ?? "").trim();
  if (!trimmed) return "?";
  return Array.from(trimmed)[0].toUpperCase();
}

function hasIcon(project: Project) {
  return Boolean(project.android.iconPath) && !failedIcons.value.has(project.id);
}

function iconSrc(project: Project) {
  return convertFileSrc(project.android.iconPath as string, "asset");
}

function markIconFailed(id: string) {
  failedIcons.value = new Set([...failedIcons.value, id]);
}

async function reload() {
  loading.value = true;
  try {
    const loaded = await loadSettings();
    settings.value = loaded;
    projects.value = sortList(loaded.projects);
  } catch (e) {
    ElMessage.error(`读取项目失败：${String(e)}`);
  } finally {
    loading.value = false;
  }
}

async function persist(list: Project[]) {
  const current = settings.value;
  const next: Settings = {
    jdkPath: current?.jdkPath ?? null,
    sdkPath: current?.sdkPath ?? null,
    adbPath: current?.adbPath ?? null,
    projects: list,
    windowsKernels: current?.windowsKernels ?? [],
  };
  await saveSettings(next);
  settings.value = next;
  projects.value = sortList(list);
}

function openCreate() {
  createName.value = "";
  createVisible.value = true;
}

async function create() {
  const name = createName.value.trim();
  if (!name) {
    ElMessage.warning("请输入项目名称");
    return;
  }
  const project = newProject(name);
  try {
    await persist([...(settings.value?.projects ?? []), project]);
    createVisible.value = false;
    open(project.id);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function duplicate(source: Project) {
  try {
    const copy = newProject(`${source.name} 副本`);
    copy.target = source.target;
    copy.android = JSON.parse(JSON.stringify(source.android));
    await persist([...(settings.value?.projects ?? []), copy]);
    ElMessage.success("已复制项目");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function remove(project: Project) {
  try {
    await ElMessageBox.confirm(`删除后不可恢复，确定删除「${project.name}」？`, "删除项目", {
      type: "warning",
      confirmButtonText: "删除",
      cancelButtonText: "取消",
      confirmButtonClass: "danger-btn",
    });
  } catch {
    return;
  }
  try {
    await persist((settings.value?.projects ?? []).filter((p) => p.id !== project.id));
    ElMessage.success("已删除");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

function formatTime(ts: number) {
  if (!ts) return "—";
  const date = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

onMounted(reload);

// 本页被 keep-alive 缓存：从项目详情返回时不会重新 mount，
// 这里在「再次激活」时重读磁盘，保证改了图标 / 名称后列表立即生效。
let firstActivate = true;
onActivated(() => {
  if (firstActivate) {
    firstActivate = false;
    return;
  }
  void reload();
});
</script>

<template>
  <div class="page page-frame">
    <header class="page-head">
      <div class="head-text">
        <h1>
          项目管理
          <span v-if="projects.length" class="count mono">{{ projects.length }}</span>
        </h1>
        <p>每个项目保存一套独立的套壳配置，可随时回来修改或重新出包。</p>
      </div>
      <div class="head-actions">
        <el-button type="primary" :icon="Plus" @click="openCreate">新建项目</el-button>
      </div>
    </header>

    <el-scrollbar v-loading="loading" class="page-body">
      <div v-if="!projects.length && !loading" class="empty">
        <div class="empty-icon">
          <el-icon :size="24"><FolderOpened /></el-icon>
        </div>
        <div class="empty-title">还没有项目</div>
        <div class="empty-desc">新建一个项目，填写应用名与 H5 地址，即可打包出 APK。</div>
        <el-button type="primary" :icon="Plus" @click="openCreate">新建项目</el-button>
      </div>

      <div v-else class="grid">
        <article
          v-for="project in projects"
          :key="project.id"
          class="card"
          tabindex="0"
          @click="open(project.id)"
          @keyup.enter="open(project.id)"
        >
          <div class="card-head">
            <div class="avatar" :style="{ background: avatarBg(project) }">
              <img
                v-if="hasIcon(project)"
                :src="iconSrc(project)"
                alt=""
                @error="markIconFailed(project.id)"
              />
              <span v-else class="avatar-letter">{{ initial(project.name) }}</span>
            </div>
            <div class="card-title">{{ project.name }}</div>
            <div class="card-actions">
              <button class="icon-btn" title="编辑" @click.stop="open(project.id)">
                <el-icon :size="14"><EditPen /></el-icon>
              </button>
              <button class="icon-btn" title="复制" @click.stop="duplicate(project)">
                <el-icon :size="14"><CopyDocument /></el-icon>
              </button>
              <button class="icon-btn danger" title="删除" @click.stop="remove(project)">
                <el-icon :size="14"><Delete /></el-icon>
              </button>
            </div>
          </div>

          <div class="card-pkg mono">{{ project.android.packageName }}</div>

          <div class="card-meta">
            <span class="pill mono">v{{ project.android.versionName }}</span>
            <span class="stamp">{{ formatTime(project.updatedAt) }}</span>
          </div>
        </article>
      </div>
    </el-scrollbar>

    <el-dialog v-model="createVisible" title="新建项目" width="400px" align-center>
      <el-form label-position="top">
        <el-form-item>
          <el-input v-model="createName" placeholder="请输入项目名称" @keyup.enter="create" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="createVisible = false">取消</el-button>
        <el-button type="primary" @click="create">创建并编辑</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
h1 {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  font-size: 21px;
  font-weight: 650;
  letter-spacing: -0.02em;
}

.count {
  display: inline-grid;
  place-items: center;
  min-width: 22px;
  height: 20px;
  padding: 0 7px;
  border-radius: 999px;
  background: var(--surface-sunken);
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 500;
}

.head-text p {
  margin: 6px 0 0;
  font-size: 12.5px;
  color: var(--text-muted);
}

.head-actions {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(292px, 1fr));
  gap: 13px;
}

.card {
  padding: 15px 16px 13px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  background: var(--surface);
  box-shadow: var(--shadow-card);
  cursor: pointer;
  transition: border-color 0.16s var(--ease), transform 0.16s var(--ease),
    box-shadow 0.16s var(--ease);
}

.card:hover {
  border-color: var(--border-strong);
  transform: translateY(-2px);
  box-shadow: var(--shadow-raised);
}

.card-head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.avatar {
  position: relative;
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  flex: 0 0 34px;
  overflow: hidden;
  border-radius: 10px;
  color: #ffffff;
  font-size: 14.5px;
  font-weight: 600;
  user-select: none;
}

.avatar img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.avatar-letter {
  line-height: 1;
}

.card-title {
  flex: 1;
  min-width: 0;
  font-size: 14.5px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-actions {
  display: flex;
  gap: 2px;
  flex: 0 0 auto;
  opacity: 0;
  transition: opacity 0.16s var(--ease);
}

.card:hover .card-actions,
.card:focus-within .card-actions {
  opacity: 1;
}

.icon-btn {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: background-color 0.16s var(--ease), color 0.16s var(--ease);
}

.icon-btn:hover {
  background: var(--surface-sunken);
  color: var(--text-primary);
}

.icon-btn.danger:hover {
  background: var(--danger-soft);
  color: var(--danger);
}

.card-pkg {
  margin-top: 11px;
  font-size: 11.5px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}

.pill {
  padding: 2px 8px;
  border-radius: 6px;
  background: var(--surface-sunken);
  color: var(--text-secondary);
  font-size: 11px;
  line-height: 18px;
}

.stamp {
  margin-left: auto;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-faint);
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 66px 20px;
  border: 1px dashed var(--border-default);
  border-radius: var(--radius-lg);
}

.empty-icon {
  display: grid;
  place-items: center;
  width: 50px;
  height: 50px;
  margin-bottom: 4px;
  border-radius: 15px;
  border: 1px solid var(--border-subtle);
  background: var(--surface);
  color: var(--text-muted);
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
}

.empty-desc {
  margin-bottom: 10px;
  font-size: 12.5px;
  color: var(--text-muted);
  text-align: center;
}
</style>
