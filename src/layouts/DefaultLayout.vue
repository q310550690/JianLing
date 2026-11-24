<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Files, Setting, Link } from "@element-plus/icons-vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { APP_INFO } from "../app-info";

const route = useRoute();
const router = useRouter();

const navItems = [
  { name: "projects", label: "项目管理", icon: Files, to: "/projects" },
  { name: "settings", label: "设置", icon: Setting, to: "/settings" },
];

/** 项目详情属于「项目管理」这一段，侧栏保持高亮 */
const activeName = computed(() =>
  route.name === "project" || route.name === "project-debug" ? "projects" : String(route.name),
);

function go(to: string) {
  router.push(to);
}

async function openHomepage() {
  try {
    await openUrl(APP_INFO.homepage);
  } catch {
    // 外部打不开就静默失败，不影响界面
  }
}
</script>

<template>
  <div class="layout">
    <aside class="sidebar">
      <div class="brand">
        <img class="brand-logo" src="/brand-logo.png" alt="茧灵" />
        <div class="brand-text">
          <div class="brand-name">{{ APP_INFO.name }}</div>
          <div class="brand-sub">{{ APP_INFO.summary }}</div>
        </div>
      </div>

      <div class="nav-group">
        <div class="nav-label">工作区</div>
        <button
          v-for="item in navItems"
          :key="item.name"
          class="nav-item"
          :class="{ active: activeName === item.name }"
          type="button"
          @click="go(item.to)"
        >
          <el-icon :size="15"><component :is="item.icon" /></el-icon>
          <span>{{ item.label }}</span>
        </button>
      </div>

      <div class="spacer" />

      <div class="app-info">
        <div class="info-name">{{ APP_INFO.name }}</div>
        <div class="info-line">{{ APP_INFO.summary }} · v{{ APP_INFO.version }}</div>
        <div class="info-line">{{ APP_INFO.stack }}</div>
        <button class="info-link" type="button" @click="openHomepage">
          <el-icon :size="12"><Link /></el-icon>
          <span>{{ APP_INFO.homepage }}</span>
        </button>
      </div>
    </aside>

    <main class="main">
      <router-view v-slot="{ Component }">
        <keep-alive>
          <component v-if="route.meta.keepAlive" :is="Component" :key="String(route.name)" />
        </keep-alive>
        <component v-if="!route.meta.keepAlive" :is="Component" :key="route.fullPath" />
      </router-view>
    </main>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  flex: 1;
  min-height: 0;
  gap: 12px;
  padding: 12px;
}

.sidebar {
  display: flex;
  flex-direction: column;
  flex: 0 0 232px;
  width: 232px;
  padding: 14px 12px 12px;
  border-radius: var(--radius-lg);
  background: rgba(248, 249, 251, 0.92);
  border: 1px solid var(--border-subtle);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 6px 14px;
}

.brand-logo {
  display: block;
  width: 32px;
  height: 32px;
  object-fit: contain;
}

.brand-text {
  min-width: 0;
}

.brand-name {
  font-size: 14px;
  font-weight: 650;
  color: var(--text-primary);
  letter-spacing: -0.01em;
}

.brand-sub {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--text-faint);
}

.nav-group {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-label {
  padding: 6px 8px 4px;
  font-size: 11.5px;
  color: var(--text-faint);
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  height: 34px;
  padding: 0 10px;
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--text-secondary);
  font-family: inherit;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  transition: background-color 0.14s var(--ease), color 0.14s var(--ease);
}

.nav-item:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.nav-item.active {
  background: var(--surface-solid);
  color: var(--text-primary);
  font-weight: 550;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.06);
}

.spacer {
  flex: 1;
  min-height: 12px;
}

.app-info {
  padding: 10px 8px 4px;
  border-top: 1px solid var(--border-subtle);
}

.info-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.info-line {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--text-faint);
  word-break: break-all;
}

.info-link {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-top: 8px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text-muted);
  font-family: inherit;
  font-size: 11.5px;
  cursor: pointer;
  text-align: left;
  word-break: break-all;
}

.info-link:hover {
  color: var(--accent);
  text-decoration: underline;
  text-underline-offset: 3px;
}

.main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border-subtle);
  background: rgba(255, 255, 255, 0.88);
  overflow: hidden;
}
</style>
