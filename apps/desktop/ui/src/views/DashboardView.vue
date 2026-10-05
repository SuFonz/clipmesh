<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";

import {
  AppButton,
  AppCard,
  AppIcon,
  AppToggle,
  ClipboardItemCard,
  DeviceCard,
  EmptyState,
  FingerprintBadge,
  StatusPill,
  platformLabel,
  sendClipboard,
  sendText,
  toMessage,
  useHistoryStore,
  useIdentityStore,
  usePeersStore,
  useSettingsStore,
  useStatusStore,
  useToast,
  type ClipboardItemView,
} from "@clipmesh/ui-core";

/**
 * 桌面仪表盘：一眼看到运行状态 + 一个能立刻用的"发送"区 + 实时设备/历史。
 */
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const historyStore = useHistoryStore();
const settingsStore = useSettingsStore();
const identityStore = useIdentityStore();
const toast = useToast();

const draft = ref("");
const sendingText = ref(false);
const sendingClipboard = ref(false);
const togglingEngine = ref(false);
const togglingAutoSync = ref(false);

const onlinePeers = computed(() => peersStore.peers.filter((p) => p.connected));
const visiblePeers = computed(() => peersStore.peers.slice(0, 5));
const recent = computed(() => historyStore.items.slice(0, 4));

async function toggleEngine(): Promise<void> {
  togglingEngine.value = true;
  try {
    if (statusStore.running) {
      await statusStore.stop();
      toast.info("引擎已停止");
    } else {
      await statusStore.start();
      toast.success("引擎已启动", "正在监听剪贴板并接受局域网连接。");
    }
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  } finally {
    togglingEngine.value = false;
  }
}

async function onAutoSync(value: boolean): Promise<void> {
  togglingAutoSync.value = true;
  try {
    await settingsStore.update({ autoSync: value });
    toast.info(value ? "已开启自动同步" : "已关闭自动同步");
  } catch (cause) {
    toast.error("设置未保存", toMessage(cause));
  } finally {
    togglingAutoSync.value = false;
  }
}

async function onSendText(): Promise<void> {
  const content = draft.value.trim();
  if (content === "") {
    toast.warn("内容为空", "先写点什么再发送。");
    return;
  }
  sendingText.value = true;
  try {
    const result = await sendText(content);
    draft.value = "";
    toast.success(
      result.delivered > 0 ? `已发送到 ${result.delivered} 台设备` : "已记录，暂无在线设备",
    );
  } catch (cause) {
    toast.error("发送失败", toMessage(cause));
  } finally {
    sendingText.value = false;
  }
}

async function onBroadcastClipboard(): Promise<void> {
  sendingClipboard.value = true;
  try {
    const result = await sendClipboard();
    toast.success(
      result.delivered > 0
        ? `系统剪贴板已广播到 ${result.delivered} 台设备`
        : "已读取剪贴板，但当前没有在线设备",
    );
  } catch (cause) {
    toast.error("广播失败", toMessage(cause));
  } finally {
    sendingClipboard.value = false;
  }
}

async function onCopy(item: ClipboardItemView): Promise<void> {
  try {
    await historyStore.copy(item.id);
    toast.success("已复制到本机剪贴板");
  } catch (cause) {
    toast.error("复制失败", toMessage(cause));
  }
}

async function onResend(item: ClipboardItemView): Promise<void> {
  try {
    const result = await historyStore.resend(item.id);
    toast.success(`已重新发送（${result.delivered} 台设备收到）`);
  } catch (cause) {
    toast.error("重发失败", toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">仪表盘</h1>
        <p class="cm-page-sub">
          {{ statusStore.deviceName }} ·
          {{ statusStore.running ? "引擎运行中" : "引擎已停止" }} ·
          {{ onlinePeers.length }} 台设备在线
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill
          :label="statusStore.running ? '运行中' : '已停止'"
          :tone="statusStore.running ? 'ok' : 'idle'"
          :pulse="statusStore.running"
        />
        <StatusPill
          :label="`发现 ${peersStore.count}`"
          tone="info"
          icon="radar"
        />
        <AppButton
          :variant="statusStore.running ? 'secondary' : 'primary'"
          :icon="statusStore.running ? 'power' : 'zap'"
          :loading="togglingEngine"
          @click="toggleEngine"
        >
          {{ statusStore.running ? "停止引擎" : "启动引擎" }}
        </AppButton>
      </div>
    </header>

    <div v-if="statusStore.lastError" class="banner" role="alert">
      <AppIcon name="alert" :size="16" />
      <span class="banner-text">{{ statusStore.lastError }}</span>
      <AppButton size="sm" variant="ghost" @click="statusStore.setError(null)">知道了</AppButton>
    </div>

    <div class="dash">
      <section class="col">
        <AppCard title="本机" icon="monitor" subtitle="身份与监听信息">
          <template #actions>
            <RouterLink to="/settings" class="link">设置</RouterLink>
          </template>

          <div class="identity">
            <div class="ident-row">
              <span class="k">设备名</span>
              <span class="v">{{ identityStore.deviceName || statusStore.deviceName }}</span>
            </div>
            <div class="ident-row">
              <span class="k">平台</span>
              <span class="v">
                <StatusPill
                  :label="platformLabel(statusStore.platform)"
                  tone="idle"
                  :icon="statusStore.platform === 'android' ? 'android' : 'monitor'"
                  size="sm"
                />
              </span>
            </div>
            <div class="ident-row">
              <span class="k">监听端口</span>
              <span class="v cm-mono">{{ statusStore.listenPort }}</span>
            </div>
            <div class="ident-row">
              <span class="k">自动同步</span>
              <span class="v">
                <StatusPill
                  :label="statusStore.autoSync ? '已开启' : '已关闭'"
                  :tone="statusStore.autoSync ? 'ok' : 'idle'"
                  size="sm"
                />
              </span>
            </div>
          </div>

          <div class="fp-wrap">
            <FingerprintBadge :fingerprint="statusStore.fingerprint" label="本机证书指纹" size="lg" />
            <p class="cm-help">
              对方设备上应该显示完全相同的字符。不一致就说明有人在中途替换证书 —— 拒绝配对。
            </p>
          </div>
        </AppCard>

        <AppCard title="最近的剪贴板" icon="history" subtitle="最新 4 条，可复制或重发">
          <template #actions>
            <RouterLink to="/history" class="link">全部历史</RouterLink>
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
          <EmptyState
            v-else
            compact
            icon="clipboard"
            title="还没有历史"
            description="同步过的文本和图片会出现在这里。"
          />
        </AppCard>
      </section>

      <section class="col">
        <AppCard title="快速发送" icon="send" subtitle="发送到所有已信任且在线的设备">
          <div class="composer">
            <textarea
              v-model="draft"
              class="cm-textarea"
              placeholder="粘贴或输入要同步的文本，Ctrl + Enter 直接发送"
              rows="4"
              @keydown.ctrl.enter.prevent="onSendText"
              @keydown.meta.enter.prevent="onSendText"
            />
            <div class="composer-actions">
              <AppButton
                variant="primary"
                icon="send"
                :loading="sendingText"
                :disabled="draft.trim() === ''"
                @click="onSendText"
              >
                发送文本
              </AppButton>
              <AppButton
                icon="clipboard"
                :loading="sendingClipboard"
                title="读取系统剪贴板并广播"
                @click="onBroadcastClipboard"
              >
                广播当前剪贴板
              </AppButton>
            </div>
            <div class="switch-row">
              <AppToggle
                :model-value="settingsStore.autoSync"
                :disabled="togglingAutoSync"
                label="后台自动同步"
                description="本机剪贴板变化时自动推送，不需要手动点发送。"
                @update:model-value="onAutoSync"
              />
            </div>
          </div>
        </AppCard>

        <AppCard title="在线设备" icon="devices" :subtitle="`${onlinePeers.length} 台已连接 · ${peersStore.count} 台被发现`">
          <template #actions>
            <RouterLink to="/devices" class="link">管理设备</RouterLink>
          </template>

          <div v-if="visiblePeers.length" class="cm-list">
            <DeviceCard
              v-for="peer in visiblePeers"
              :key="peer.deviceId"
              :name="peer.name"
              :platform="peer.platform"
              :fingerprint="peer.fingerprint"
              :online="peer.connected"
              :trusted="peer.trusted"
              :pairing="peer.pairing"
              :address="peer.address"
              :last-seen="peer.lastSeen"
              :dimmed="!peer.connected"
            />
          </div>
          <EmptyState
            v-else
            compact
            icon="radar"
            title="还没发现设备"
            description="确认两台设备在同一个局域网，并且防火墙放行了 mDNS 与监听端口。"
          />
        </AppCard>
      </section>
    </div>
  </div>
</template>

<style scoped>
.view {
  display: block;
}

.banner {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: var(--space-4);
  padding: 10px 12px;
  border: 1px solid var(--border-danger);
  border-radius: var(--radius);
  background: var(--danger-soft);
  color: var(--danger);
}

.banner-text {
  flex: 1;
  min-width: 0;
  font-size: 13px;
}

.dash {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: var(--space-4);
  align-items: start;
}

.col {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.link {
  font-size: 12.5px;
  font-weight: 600;
}

.identity {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 10px;
}

.ident-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 9px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.k {
  color: var(--text-dim);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
}

.v {
  font-size: 13px;
  font-weight: 600;
}

.fp-wrap {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: var(--space-4);
}

.composer {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.composer-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.switch-row {
  padding-top: 10px;
  border-top: 1px solid var(--border-soft);
}

@media (max-width: 1080px) {
  .dash {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
