<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink, useRoute } from "vue-router";

import {
  AppIcon,
  StatusPill,
  isMock,
  platformLabel,
  shortFingerprint,
  toMessage,
  usePairingStore,
  usePeersStore,
  useStatusStore,
  useToast,
  useTrustedStore,
  type IconName,
} from "@clipmesh/ui-core";

/**
 * 桌面端外壳：220px 侧边导航 + 可滚动内容区 + 底部状态条。
 * 多栏排版由各视图内部的 CSS Grid 负责（内容区最大宽度 1180px）。
 */
interface NavItem {
  to: string;
  label: string;
  icon: IconName;
  hint: string;
}

const navItems: NavItem[] = [
  { to: "/", label: "首页", icon: "dashboard", hint: "本机与在线设备" },
  { to: "/devices", label: "设备", icon: "devices", hint: "配对请求 / 发现 / 信任的设备" },
  { to: "/history", label: "历史", icon: "history", hint: "最近的剪贴板内容" },
  { to: "/settings", label: "设置", icon: "settings", hint: "同步与身份" },
];

const route = useRoute();
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const trustedStore = useTrustedStore();
const pairingStore = usePairingStore();
const toast = useToast();

const mock = isMock();
const toggling = ref(false);

/** 配对请求现在直接排在设备页最上面，所以徽标把"待决定的请求"也一起算进去（与 Android 一致）。 */
const badges = computed<Record<string, number>>(() => ({
  "/devices": pairingStore.incomingCount + peersStore.untrusted.length,
}));

const isActive = (to: string): boolean => route.path === to;
const pageTitle = computed<string>(() => {
  const meta = route.meta as { title?: string };
  return meta.title ?? "ClipMesh";
});

async function toggleEngine(): Promise<void> {
  toggling.value = true;
  try {
    if (statusStore.running) {
      await statusStore.stop();
      toast.info("引擎已停止", "不再监听剪贴板，也不再响应局域网请求。");
    } else {
      await statusStore.start();
      toast.success("引擎已启动", "正在通过 mDNS 广播并监听剪贴板。");
    }
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  } finally {
    toggling.value = false;
  }
}
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <span class="logo"><AppIcon name="link" :size="18" /></span>
        <span class="brand-text">
          <strong>ClipMesh</strong>
          <small>P2P 剪贴板同步</small>
        </span>
      </div>

      <nav class="nav" aria-label="主导航">
        <RouterLink
          v-for="item in navItems"
          :key="item.to"
          :to="item.to"
          class="nav-item"
          :class="{ active: isActive(item.to) }"
          :title="item.hint"
        >
          <AppIcon :name="item.icon" :size="17" />
          <span class="nav-label">{{ item.label }}</span>
          <span v-if="badges[item.to]" class="nav-badge">{{ badges[item.to] }}</span>
        </RouterLink>
      </nav>

      <div class="side-foot">
        <div class="self">
          <span class="self-name cm-truncate">{{ statusStore.deviceName }}</span>
          <span class="self-meta">{{ platformLabel(statusStore.platform) }}</span>
        </div>
        <StatusPill
          :label="statusStore.running ? '运行中' : '已停止'"
          :tone="statusStore.running ? 'ok' : 'idle'"
          :pulse="statusStore.running"
          size="sm"
        />
        <span v-if="mock" class="mock" title="没有检测到 Tauri 运行时，正在使用内存 mock 数据">
          MOCK
        </span>
      </div>
    </aside>

    <div class="main">
      <div class="scroll">
        <div class="content">
          <slot />
        </div>
      </div>

      <footer class="statusbar">
        <span class="sb-title">{{ pageTitle }}</span>
        <span class="sb-sep" />
        <span class="sb-item">
          <AppIcon name="devices" :size="13" />
          在线 <b>{{ statusStore.connectedPeers }}</b>
        </span>
        <span class="sb-item">
          <AppIcon name="shield" :size="13" />
          已信任 <b>{{ trustedStore.count }}</b>
        </span>
        <span class="sb-item">
          <AppIcon name="radar" :size="13" />
          发现 <b>{{ peersStore.count }}</b>
        </span>
        <span class="sb-item cm-mono" :title="`mDNS 监听端口 ${statusStore.listenPort}`">
          :{{ statusStore.listenPort }}
        </span>

        <span class="sb-spacer" />

        <span
          v-if="statusStore.lastError"
          class="sb-error"
          :title="statusStore.lastError"
        >
          <AppIcon name="alert" :size="13" />
          <span class="cm-truncate">{{ statusStore.lastError }}</span>
        </span>

        <span class="sb-fp cm-mono" :title="statusStore.fingerprint">
          {{ shortFingerprint(statusStore.fingerprint, 2) || "—" }}
        </span>

        <button
          class="sb-power"
          type="button"
          :disabled="toggling"
          :title="statusStore.running ? '停止引擎' : '启动引擎'"
          @click="toggleEngine"
        >
          <AppIcon :name="statusStore.running ? 'power' : 'zap'" :size="14" />
          {{ statusStore.running ? "停止" : "启动" }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: var(--sidebar-w) minmax(0, 1fr);
  height: 100vh;
  background: var(--bg);
}

/* ---------- 侧边栏 ---------- */

.sidebar {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  padding: 14px 12px;
  background: var(--bg-soft);
  border-right: 1px solid var(--border);
  min-height: 0;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 6px 10px;
  border-bottom: 1px solid var(--border-soft);
}

.logo {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  flex: none;
  border-radius: var(--radius-sm);
  background: var(--accent);
  color: var(--accent-fg);
  box-shadow: var(--shadow-1);
}

.brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.25;
  min-width: 0;
}

.brand-text strong {
  font-size: 14.5px;
  font-weight: 680;
  letter-spacing: -0.01em;
}

.brand-text small {
  color: var(--text-dim);
  font-size: 11px;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 38px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  color: var(--text-muted);
  font-size: 13.5px;
  font-weight: 550;
  text-decoration: none;
  transition:
    background var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.nav-item:hover {
  background: var(--surface-2);
  color: var(--text);
  text-decoration: none;
}

.nav-item.active {
  background: var(--accent-soft);
  color: var(--accent);
}

.nav-item.active :deep(.cm-icon) {
  color: var(--accent);
}

.nav-label {
  flex: 1;
  min-width: 0;
}

.nav-badge {
  min-width: 20px;
  padding: 0 6px;
  border-radius: var(--radius-full);
  background: var(--accent);
  color: var(--accent-fg);
  font-size: 11px;
  font-weight: 700;
  text-align: center;
  line-height: 18px;
}

.side-foot {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
}

.self {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}

.self-name {
  font-size: 12.5px;
  font-weight: 600;
}

.self-meta {
  color: var(--text-dim);
  font-size: 11px;
}

.mock {
  padding: 1px 6px;
  border-radius: var(--radius-full);
  background: var(--warn-soft);
  color: var(--warn);
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.08em;
}

/* ---------- 主区 ---------- */

.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.content {
  max-width: var(--content-max);
  margin: 0 auto;
  padding: 22px 26px 28px;
}

/* ---------- 状态条 ---------- */

.statusbar {
  display: flex;
  align-items: center;
  gap: 14px;
  height: 34px;
  padding: 0 14px;
  border-top: 1px solid var(--border);
  background: var(--bg-soft);
  color: var(--text-muted);
  font-size: 11.5px;
  flex: none;
}

.sb-title {
  font-weight: 650;
  color: var(--text);
}

.sb-sep {
  width: 1px;
  height: 14px;
  background: var(--border);
}

.sb-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.sb-item b {
  color: var(--text);
  font-weight: 650;
}

.sb-spacer {
  flex: 1;
}

.sb-error {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 34ch;
  color: var(--danger);
}

.sb-fp {
  color: var(--text-dim);
  letter-spacing: 0.04em;
}

.sb-power {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-full);
  background: var(--surface);
  color: var(--text-muted);
  font-size: 11.5px;
  font-weight: 600;
  cursor: pointer;
  transition:
    background var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.sb-power:hover:not(:disabled) {
  background: var(--surface-2);
  color: var(--text);
}

.sb-power:disabled {
  opacity: 0.5;
  cursor: progress;
}
</style>
