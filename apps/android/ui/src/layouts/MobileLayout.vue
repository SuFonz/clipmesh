<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { RouterLink, useRoute } from "vue-router";

import {
  AppButton,
  AppIcon,
  StatusPill,
  androidApi,
  isMock,
  sendClipboard,
  toMessage,
  usePairingStore,
  usePeersStore,
  useStatusStore,
  useToast,
  type IconName,
} from "@clipmesh/ui-core";

/**
 * Android 外壳：顶部标题栏 + 单列内容 + 底部 4 标签栏。
 * 所有触摸目标 ≥48px，并且处理了 `env(safe-area-inset-*)`（刘海屏 / 手势条）。
 */
interface Tab {
  to: string;
  label: string;
  icon: IconName;
}

const tabs: Tab[] = [
  { to: "/", label: "首页", icon: "clipboard" },
  { to: "/devices", label: "设备", icon: "devices" },
  { to: "/history", label: "历史", icon: "history" },
  { to: "/settings", label: "设置", icon: "settings" },
];

const route = useRoute();
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const pairingStore = usePairingStore();
const toast = useToast();

const mock = isMock();
const busy = ref(false);

const title = computed<string>(() => {
  const meta = route.meta as { title?: string };
  return meta.title ?? "ClipMesh";
});

const deviceBadge = computed<number>(
  () => pairingStore.incomingCount + peersStore.untrusted.length,
);

const subtitle = computed<string>(() => {
  if (!statusStore.running) return "引擎已停止";
  return `${statusStore.connectedPeers} 台在线 · 已信任 ${statusStore.trustedPeers}`;
});

async function toggleEngine(): Promise<void> {
  busy.value = true;
  try {
    if (statusStore.running) {
      await statusStore.stop();
      toast.info("引擎已停止");
    } else {
      await statusStore.start();
      toast.success("引擎已启动");
    }
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  } finally {
    busy.value = false;
  }
}

// ---------------------------------------------------------------------------
// 广播剪贴板
// ---------------------------------------------------------------------------

const sending = ref(false);

/** 读剪贴板、推给在线设备，并提示结果；返回是否成功。 */
async function runBroadcast(): Promise<boolean> {
  sending.value = true;
  try {
    const result = await sendClipboard();
    toast.success(
      result.delivered > 0 ? `已广播到 ${result.delivered} 台设备` : "没有在线设备，已存入历史",
    );
    return true;
  } catch (cause) {
    toast.error("广播失败", toMessage(cause));
    return false;
  } finally {
    sending.value = false;
  }
}

/** 按钮：广播完留在应用里，用户没打算离开。 */
async function broadcast(): Promise<void> {
  await runBroadcast();
}

/**
 * 通知栏那颗「广播剪贴板」的落点。
 *
 * 那颗按钮只能把应用带到前台 —— Android 10+ 不允许后台读剪贴板 —— 并在
 * `onNewIntent` 里留一个标记。这里取走它：广播一次，然后把用户送回他原来的应用。
 * 这就是"半无感"能做到的上限：应用必须露个面，但不会把人困住。
 *
 * 轮询放在**外壳**而不是首页，因为通知可能在任何标签页上被点开 —— 挂在首页的话
 * 用户在设置页时首页根本没挂载，请求就丢了。
 */
let pendingPoll: ReturnType<typeof setInterval> | undefined;

async function collectPendingBroadcast(): Promise<void> {
  let requested: boolean;
  try {
    requested = await androidApi.takePendingBroadcast();
  } catch {
    // 非 Android 平台（浏览器调试）没有这个命令。
    return;
  }
  if (!requested) return;

  if (await runBroadcast()) {
    await androidApi.leaveApp().catch(() => undefined);
  }
}

onMounted(() => {
  pendingPoll = setInterval(() => {
    void collectPendingBroadcast();
  }, 500);
});

onBeforeUnmount(() => {
  if (pendingPoll !== undefined) clearInterval(pendingPoll);
});
</script>

<template>
  <div class="shell">
    <header class="header">
      <div class="head-text">
        <h1 class="head-title cm-truncate">{{ title }}</h1>
        <p class="head-sub cm-truncate">
          <span class="dot" :class="statusStore.running ? 'on' : 'off'" />
          {{ subtitle }}
        </p>
      </div>

      <div class="head-actions">
        <span v-if="mock" class="mock">MOCK</span>
        <StatusPill
          v-if="pairingStore.incomingCount > 0"
          :label="`${pairingStore.incomingCount} 个配对请求`"
          tone="warn"
          pulse
          size="sm"
        />
        <!--
          广播剪贴板和引擎开关并排放在顶栏：两个都是全局动作，任何标签页都用得上，
          所以不放在某一个页面的操作行里。用 `size="lg"`（48px）与旁边那颗
          `icon-btn` 的 `--tap-min` 对齐 —— 顶栏本来就以它为准，不会再变高。
        -->
        <AppButton
          variant="primary"
          size="lg"
          icon="send"
          :loading="sending"
          :disabled="!statusStore.running"
          @click="broadcast"
        >
          广播剪贴板
        </AppButton>
        <button
          class="icon-btn"
          type="button"
          :disabled="busy"
          :aria-label="statusStore.running ? '停止引擎' : '启动引擎'"
          @click="toggleEngine"
        >
          <AppIcon :name="statusStore.running ? 'power' : 'zap'" :size="18" />
        </button>
      </div>
    </header>

    <main class="content">
      <slot />
    </main>

    <nav class="tabbar" aria-label="主导航">
      <RouterLink
        v-for="tab in tabs"
        :key="tab.to"
        :to="tab.to"
        class="tab"
        :class="{ active: route.path === tab.to }"
      >
        <span class="tab-icon">
          <AppIcon :name="tab.icon" :size="21" />
          <span v-if="tab.to === '/devices' && deviceBadge > 0" class="tab-badge">
            {{ deviceBadge }}
          </span>
        </span>
        <span class="tab-label">{{ tab.label }}</span>
      </RouterLink>
    </nav>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  height: 100dvh;
  background: var(--bg);
}

/* ---------- 顶部栏 ---------- */

.header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex: none;
  padding: 10px 14px;
  /* 刘海屏 / 状态栏 */
  padding-top: calc(10px + env(safe-area-inset-top, 0px));
  background: var(--bg-soft);
  border-bottom: 1px solid var(--border);
}

.head-text {
  flex: 1;
  min-width: 0;
}

.head-title {
  font-size: 17px;
  font-weight: 680;
  letter-spacing: -0.01em;
  line-height: 1.25;
}

.head-sub {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 1px;
  color: var(--text-muted);
  font-size: 12px;
}

.dot {
  width: 7px;
  height: 7px;
  flex: none;
  border-radius: 50%;
  background: var(--text-dim);
}

.dot.on {
  background: var(--ok);
  box-shadow: 0 0 0 3px var(--ok-soft);
}

.head-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}

.mock {
  padding: 2px 7px;
  border-radius: var(--radius-full);
  background: var(--warn-soft);
  color: var(--warn);
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.06em;
}

.icon-btn {
  display: grid;
  place-items: center;
  width: var(--tap-min);
  height: var(--tap-min);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-muted);
  cursor: pointer;
}

.icon-btn:active:not(:disabled) {
  background: var(--surface-2);
  color: var(--text);
}

.icon-btn:disabled {
  opacity: 0.55;
}

/* ---------- 内容 ---------- */

.content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  padding: 14px 14px 20px;
}

/* ---------- 底部标签栏 ---------- */

.tabbar {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  flex: none;
  background: var(--bg-soft);
  border-top: 1px solid var(--border);
  /* 手势条 */
  padding-bottom: env(safe-area-inset-bottom, 0px);
}

.tab {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 3px;
  min-height: var(--tabbar-h);
  padding: 6px 2px;
  color: var(--text-dim);
  text-decoration: none;
  transition: color var(--dur-fast) var(--ease);
}

.tab:hover {
  text-decoration: none;
}

.tab.active {
  color: var(--accent);
}

.tab-icon {
  position: relative;
  display: grid;
  place-items: center;
}

.tab-label {
  font-size: 11.5px;
  font-weight: 600;
}

.tab-badge {
  position: absolute;
  top: -4px;
  right: -9px;
  min-width: 16px;
  padding: 0 4px;
  border-radius: var(--radius-full);
  background: var(--warn);
  color: #1b1300;
  font-size: 10px;
  font-weight: 800;
  line-height: 15px;
  text-align: center;
}
</style>
