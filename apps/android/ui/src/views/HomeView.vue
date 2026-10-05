<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { RouterLink } from "vue-router";

import {
  AppButton,
  AppCard,
  AppToggle,
  ClipboardItemCard,
  DeviceCard,
  EmptyState,
  StatusPill,
  androidApi,
  platformLabel,
  sendClipboard,
  sendText,
  toMessage,
  useHistoryStore,
  usePeersStore,
  useSettingsStore,
  useStatusStore,
  useToast,
  type ClipboardItemView,
} from "@clipmesh/ui-core";

/**
 * Android 首页：一个巨大的「广播剪贴板」按钮（对应通知栏那颗按钮的行为）+
 * 前台服务开关 + 最近收到的内容。
 */
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const historyStore = useHistoryStore();
const settingsStore = useSettingsStore();
const toast = useToast();

const draft = ref("");
const sendingClipboard = ref(false);
const sendingText = ref(false);
const serviceRunning = ref(false);
const serviceAvailable = ref(true);
const serviceBusy = ref(false);
const notificationBusy = ref(false);

const onlinePeers = computed(() => peersStore.peers.filter((p) => p.connected));
const recent = computed(() => historyStore.items.slice(0, 3));

onMounted(async () => {
  try {
    serviceRunning.value = await androidApi.isServiceRunning();
  } catch (cause) {
    // 非 Android 平台调用这些命令会抛错：直接隐藏这块 UI，而不是弹一堆错误
    serviceAvailable.value = false;
    console.info("[clipmesh] Android 专属命令不可用：", toMessage(cause));
  }
});

async function broadcast(): Promise<void> {
  sendingClipboard.value = true;
  try {
    const result = await sendClipboard();
    toast.success(
      result.delivered > 0 ? `已广播到 ${result.delivered} 台设备` : "没有在线设备，已存入历史",
    );
  } catch (cause) {
    toast.error("广播失败", toMessage(cause));
  } finally {
    sendingClipboard.value = false;
  }
}

async function sendDraft(): Promise<void> {
  const content = draft.value.trim();
  if (content === "") {
    toast.warn("内容为空");
    return;
  }
  sendingText.value = true;
  try {
    await sendText(content);
    draft.value = "";
    toast.success("已发送");
  } catch (cause) {
    toast.error("发送失败", toMessage(cause));
  } finally {
    sendingText.value = false;
  }
}

async function toggleService(value: boolean): Promise<void> {
  serviceBusy.value = true;
  try {
    if (value) await androidApi.startService();
    else await androidApi.stopService();
    serviceRunning.value = value;
    toast.success(value ? "前台服务已启动" : "前台服务已停止");
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  } finally {
    serviceBusy.value = false;
  }
}

async function requestNotification(): Promise<void> {
  notificationBusy.value = true;
  try {
    const granted = await androidApi.requestNotificationPermission();
    toast[granted ? "success" : "warn"](
      granted ? "通知权限已授予" : "通知权限被拒绝",
      granted ? "收到远程剪贴板时会显示通知。" : "没有通知权限，后台收到内容不会有提示。",
    );
  } catch (cause) {
    toast.error("请求失败", toMessage(cause));
  } finally {
    notificationBusy.value = false;
  }
}

async function onAutoSync(value: boolean): Promise<void> {
  try {
    await settingsStore.update({ autoSync: value });
  } catch (cause) {
    toast.error("设置未保存", toMessage(cause));
  }
}

async function onSyncImages(value: boolean): Promise<void> {
  try {
    await settingsStore.update({ syncImages: value });
  } catch (cause) {
    toast.error("设置未保存", toMessage(cause));
  }
}

async function onCopy(item: ClipboardItemView): Promise<void> {
  try {
    await historyStore.copy(item.id);
    toast.success("已复制到剪贴板");
  } catch (cause) {
    toast.error("复制失败", toMessage(cause));
  }
}

async function onResend(item: ClipboardItemView): Promise<void> {
  try {
    const result = await historyStore.resend(item.id);
    toast.success(`已重发（${result.delivered} 台）`);
  } catch (cause) {
    toast.error("重发失败", toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <AppCard tone="accent" class="hero">
      <div class="hero-top">
        <div class="hero-text">
          <p class="hero-name cm-truncate">{{ statusStore.deviceName }}</p>
          <p class="hero-meta">
            {{ platformLabel(statusStore.platform) }} · 端口 {{ statusStore.listenPort }}
          </p>
        </div>
        <StatusPill
          :label="statusStore.running ? '运行中' : '已停止'"
          :tone="statusStore.running ? 'ok' : 'idle'"
          :pulse="statusStore.running"
        />
      </div>

      <AppButton
        class="broadcast"
        variant="primary"
        size="lg"
        block
        icon="send"
        :loading="sendingClipboard"
        @click="broadcast"
      >
        广播当前剪贴板
      </AppButton>
      <p class="hero-help">
        读取系统剪贴板并推送给 {{ onlinePeers.length }} 台在线设备。这和通知栏上的那颗按钮是同一个动作。
      </p>
    </AppCard>

    <AppCard title="发送文本" icon="text">
      <textarea
        v-model="draft"
        class="cm-textarea"
        rows="3"
        placeholder="输入或粘贴要同步的文本"
      />
      <AppButton
        class="mt"
        variant="primary"
        block
        icon="send"
        :loading="sendingText"
        :disabled="draft.trim() === ''"
        @click="sendDraft"
      >
        发送
      </AppButton>
    </AppCard>

    <AppCard title="同步设置" icon="refresh">
      <AppToggle
        :model-value="settingsStore.autoSync"
        label="后台自动同步"
        description="复制内容后自动推送，不需要手动点广播。"
        @update:model-value="onAutoSync"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncImages ?? false"
        label="同步图片"
        description="大图按二进制分片传输，注意流量。"
        @update:model-value="onSyncImages"
      />
    </AppCard>

    <AppCard v-if="serviceAvailable" title="后台常驻" icon="bell" subtitle="Android 前台服务">
      <AppToggle
        :model-value="serviceRunning"
        :disabled="serviceBusy"
        label="常驻前台服务"
        description="关闭后系统可能在熄屏后杀掉进程，导致收不到剪贴板。"
        @update:model-value="toggleService"
      />
      <div class="service-row">
        <p class="service-help">Android 13+ 需要通知权限才能显示常驻通知与接收提醒。</p>
        <AppButton
          size="sm"
          icon="bell"
          :loading="notificationBusy"
          @click="requestNotification"
        >
          申请通知权限
        </AppButton>
      </div>
    </AppCard>

    <AppCard title="最近收到" icon="inbox">
      <template #actions>
        <RouterLink to="/history" class="link">全部</RouterLink>
      </template>

      <div v-if="recent.length" class="cm-list">
        <ClipboardItemCard
          v-for="item in recent"
          :key="item.id"
          :item="item"
          :source-name="peersStore.nameOf(item.sourceDevice)"
          :thumbnail="historyStore.thumbnails[item.id] ?? null"
          :busy="historyStore.isBusy(item.id)"
          compact
          @copy="onCopy"
          @resend="onResend"
        />
      </div>
      <EmptyState v-else compact icon="inbox" title="还没有收到内容" />
    </AppCard>

    <AppCard title="在线设备" icon="devices">
      <template #actions>
        <RouterLink to="/devices" class="link">管理</RouterLink>
      </template>

      <div v-if="onlinePeers.length" class="cm-list">
        <DeviceCard
          v-for="peer in onlinePeers"
          :key="peer.deviceId"
          :name="peer.name"
          :platform="peer.platform"
          :fingerprint="peer.fingerprint"
          :online="true"
          :trusted="peer.trusted"
          :last-seen="peer.lastSeen"
          hide-fingerprint
          :subtitle="peer.address"
        />
      </div>
      <EmptyState
        v-else
        compact
        icon="radar"
        title="没有在线设备"
        description="确认对方也开着 ClipMesh，并且在同一网络里。"
      />
    </AppCard>
  </div>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.hero {
  background: linear-gradient(160deg, var(--accent-soft), transparent 65%), var(--surface);
}

.hero-top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 14px;
}

.hero-name {
  font-size: 16px;
  font-weight: 680;
  letter-spacing: -0.01em;
}

.hero-meta {
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 12.5px;
}

.broadcast {
  font-size: 16px;
}

.hero-help {
  margin-top: 9px;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.mt {
  margin-top: 10px;
}

.service-row {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.service-help {
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.link {
  font-size: 12.5px;
  font-weight: 600;
}
</style>
