<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { proxyHeadersError, type ProxyRule } from "../api";

const props = defineProps<{ rule: ProxyRule; index: number }>();
const emit = defineEmits<{ (e: "remove"): void }>();

const CN_DIGITS = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];

/** 1 → 一，11 → 十一，23 → 二十三（只覆盖规则数量会出现的范围） */
function chineseNumber(n: number): string {
  if (n <= 10) return n === 10 ? "十" : CN_DIGITS[n];
  if (n < 20) return `十${CN_DIGITS[n % 10]}`;
  if (n < 100) {
    const ones = n % 10;
    return `${CN_DIGITS[Math.floor(n / 10)]}十${ones ? CN_DIGITS[ones] : ""}`;
  }
  return String(n);
}

const title = computed(() => `第${chineseNumber(props.index + 1)}条`);

const headersError = computed(() => proxyHeadersError(props.rule.headers));
/** 有内容就默认展开，避免历史配置被开关误关 */
const headersEnabled = ref(!!(props.rule.headers ?? "").trim());

watch(
  () => props.rule.headers,
  (text) => {
    headersEnabled.value = !!(text ?? "").trim();
  },
);

function toggleHeaders(value: string | number | boolean) {
  if (!value) {
    props.rule.headers = "";
  }
}
</script>

<template>
  <div class="proxy-row">
    <div class="proxy-line proxy-head-line">
      <span class="proxy-title">{{ title }}</span>
      <el-button size="small" text type="danger" @click="emit('remove')">删除</el-button>
    </div>
    <div class="proxy-line proxy-line-fields">
      <div class="proxy-field">
        <span class="proxy-label">路径前缀</span>
        <el-input v-model="props.rule.prefix" placeholder="/api" />
      </div>
      <div class="proxy-field">
        <span class="proxy-label">目标地址</span>
        <el-input
          v-model="props.rule.target"
          placeholder="https://api.example.com 或 wss://ws.example.com"
        />
      </div>
      <div class="proxy-field">
        <span class="proxy-label">固定 IP（可选）</span>
        <el-input v-model="props.rule.resolveIp" placeholder="域名解析不到时填写" />
      </div>
    </div>
    <div class="proxy-toggles">
      <div class="proxy-toggle">
        <div class="proxy-toggle-text">
          <div class="proxy-toggle-name">改写 Origin</div>
          <div class="proxy-toggle-desc">后端校验来源时，改成本机地址</div>
        </div>
        <el-switch v-model="props.rule.rewriteOrigin" />
      </div>
      <div class="proxy-toggle">
        <div class="proxy-toggle-text">
          <div class="proxy-toggle-name">忽略证书</div>
          <div class="proxy-toggle-desc">上游自签名证书也照样转发</div>
        </div>
        <el-switch v-model="props.rule.insecure" />
      </div>
      <div class="proxy-toggle">
        <div class="proxy-toggle-text">
          <div class="proxy-toggle-name">追加 Header</div>
          <div class="proxy-toggle-desc">给上游请求补自定义请求头</div>
        </div>
        <el-switch v-model="headersEnabled" @change="toggleHeaders" />
      </div>
    </div>
    <div v-if="headersEnabled" class="proxy-line proxy-field">
      <span class="proxy-label">追加到上游请求的 header（JSON 对象，值为字符串）</span>
      <el-input
        v-model="props.rule.headers"
        type="textarea"
        :rows="3"
        resize="vertical"
        spellcheck="false"
        class="proxy-headers-input"
        :class="{ 'is-invalid': headersError }"
        placeholder='JSON 对象，例如 {"Authorization": "Bearer xxx"}'
      />
    </div>
    <p v-if="headersEnabled && headersError" class="proxy-error">{{ headersError }}</p>
  </div>
</template>

<style scoped>
.proxy-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
  box-sizing: border-box;
}

.proxy-line {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-width: 0;
}

/* 标题行：左侧规则序号，右侧删除 */
.proxy-head-line {
  justify-content: space-between;
}

.proxy-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
}

.proxy-line-fields {
  align-items: flex-end;
}

/* 三个输入框等分宽度 */
.proxy-line-fields .proxy-field {
  flex: 1 1 0;
  min-width: 0;
}

.proxy-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.proxy-line.proxy-field {
  align-items: stretch;
}

.proxy-label {
  font-size: 11px;
  line-height: 1.2;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.proxy-toggles {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

@media (max-width: 1000px) {
  .proxy-toggles {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 720px) {
  .proxy-toggles {
    grid-template-columns: minmax(0, 1fr);
  }
}

.proxy-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--surface-input);
}

.proxy-toggle-text {
  min-width: 0;
}

.proxy-toggle-name {
  font-size: 13px;
  color: var(--text-primary);
}

.proxy-toggle-desc {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.proxy-headers-input {
  flex: 1 1 auto;
  min-width: 0;
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
}

.proxy-headers-input.is-invalid :deep(.el-textarea__inner) {
  box-shadow: 0 0 0 1px var(--el-color-danger) inset;
}

.proxy-error {
  margin: 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--el-color-danger);
}
</style>
