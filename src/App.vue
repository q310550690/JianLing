<script setup lang="ts">
import { computed, nextTick, onMounted, provide, watch } from "vue";
import { useRoute } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, FullScreen, Close } from "@element-plus/icons-vue";
import { APP_INFO } from "./app-info";
import { checkUpdate, notifyUpdate } from "./update";

const route = useRoute();
const appWindow = getCurrentWindow();

const pageTitle = computed(() => String(route.meta.title ?? ""));
const isProject = computed(() => route.name === "project");

async function resetScroll() {
  await nextTick();
  // 内容区是 el-scrollbar，滚动容器在 .el-scrollbar__wrap 上
  const scroller =
    document.querySelector<HTMLElement>(".page-body .el-scrollbar__wrap") ??
    document.querySelector<HTMLElement>(".page-body");
  if (scroller) {
    scroller.scrollTop = 0;
  }
}

provide("resetScroll", resetScroll);

// keep-alive 的页面保留滚动位置，其余页面进入时回到顶部
watch(
  () => route.fullPath,
  () => {
    if (!route.meta.keepAlive) resetScroll();
  },
);

function minimize() {
  appWindow.minimize();
}

function toggleMaximize() {
  appWindow.toggleMaximize();
}

function closeWindow() {
  appWindow.close();
}

// 启动后检测新版本：有更新时弹出提示，点击打开下载页面
onMounted(async () => {
  try {
    const info = await checkUpdate(APP_INFO.version);
    if (info)
      notifyUpdate(info);
  }
  catch {
    // 启动检测失败不打扰用户
  }
});
</script>

<template>
  <div class="app">
    <header class="titlebar" data-tauri-drag-region>
      <div class="brand" data-tauri-drag-region>
        <img class="logo" src="/brand-logo.png" alt="茧灵" />
        <span class="brand-name">茧灵</span>
        <span class="brand-slash">/</span>
        <span class="brand-page">{{ isProject ? "项目管理" : pageTitle }}</span>
        <template v-if="isProject">
          <span class="brand-slash">/</span>
          <span class="brand-page">{{ pageTitle }}</span>
        </template>
      </div>

      <div class="win-controls">
        <button class="win-btn" type="button" title="最小化" @click="minimize">
          <el-icon :size="14"><Minus /></el-icon>
        </button>
        <button class="win-btn" type="button" title="最大化" @click="toggleMaximize">
          <el-icon :size="12"><FullScreen /></el-icon>
        </button>
        <button class="win-btn close" type="button" title="关闭" @click="closeWindow">
          <el-icon :size="14"><Close /></el-icon>
        </button>
      </div>
    </header>

    <router-view />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.55);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 0 0 1px rgba(15, 23, 42, 0.08), 0 24px 60px -20px rgba(15, 23, 42, 0.35);
}

.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 46px;
  flex: 0 0 46px;
  padding-left: 16px;
  user-select: none;
  background: rgba(255, 255, 255, 0.5);
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.logo {
  display: block;
  width: 22px;
  height: 22px;
  object-fit: contain;
}

.brand-name {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.01em;
  color: var(--text-primary);
}

.brand-slash {
  color: var(--text-faint);
  font-size: 12px;
}

.brand-page {
  font-size: 12.5px;
  color: var(--text-muted);
}

.win-controls {
  display: flex;
  height: 100%;
}

.win-btn {
  display: grid;
  place-items: center;
  width: 46px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background-color 0.14s var(--ease), color 0.14s var(--ease);
}

.win-btn:hover {
  background: rgba(15, 23, 42, 0.06);
  color: var(--text-primary);
}

.win-btn.close:hover {
  background: #d92b2b;
  color: #ffffff;
}
</style>
