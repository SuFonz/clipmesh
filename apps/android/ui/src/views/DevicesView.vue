<script setup lang="ts">
import { computed, ref } from "vue";

import {
  AppButton,
  AppCard,
  DeviceCard,
  EmptyState,
  FingerprintBadge,
  StatusPill,
  toMessage,
  usePairingStore,
  usePeersStore,
  useToast,
  useTrustedStore,
  type TrustedDeviceView,
} from "@clipmesh/ui-core";

/**
 * Android 设备页：收到的配对请求排在最上面（在大按钮上做决定），
 * 下面才是"发现的设备"和"已信任的设备"。
 */
const peersStore = usePeersStore();
const trustedStore = useTrustedStore();
const pairingStore = usePairingStore();
const toast = useToast();

const refreshing = ref(false);
const pendingUnpair = ref<TrustedDeviceView | null>(null);
const unpairing = ref(false);

const discovered = computed(() => peersStore.untrusted);
const trustedRows = computed(() =>
  trustedStore.devices.map((device) => {
    const peer = peersStore.byId(device.deviceId);
    return { device, online: peer?.connected ?? device.online };
  }),
);

async function refresh(): Promise<void> {
  refreshing.value = true;
  try {
    await Promise.all([peersStore.refresh(), trustedStore.refresh(), pairingStore.refresh()]);
  } catch (cause) {
    toast.error("刷新失败", toMessage(cause));
  } finally {
    refreshing.value = false;
  }
}

async function pair(deviceId: string): Promise<void> {
  try {
    await peersStore.pair(deviceId);
    toast.info("已发起配对", "让对方核对指纹后接受。");
  } catch (cause) {
    toast.error("配对失败", toMessage(cause));
  }
}

async function respond(deviceId: string, name: string, accept: boolean): Promise<void> {
  try {
    await pairingStore.respond(deviceId, accept);
    toast[accept ? "success" : "info"](accept ? `已信任 ${name}` : `已拒绝 ${name}`);
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  }
}

async function confirmUnpair(): Promise<void> {
  const target = pendingUnpair.value;
  if (!target) return;
  unpairing.value = true;
  try {
    await trustedStore.unpair(target.deviceId);
    toast.success("已解除信任", target.name);
    pendingUnpair.value = null;
  } catch (cause) {
    toast.error("解除失败", toMessage(cause));
  } finally {
    unpairing.value = false;
  }
}
</script>

<template>
  <div class="view">
    <div class="top">
      <StatusPill :label="`发现 ${peersStore.count}`" tone="info" icon="radar" size="sm" />
      <span class="spacer" />
      <AppButton size="sm" icon="refresh" :loading="refreshing" @click="refresh">刷新</AppButton>
    </div>

    <AppCard
      v-if="pairingStore.incoming.length"
      tone="accent"
      title="配对请求"
      icon="link"
      :subtitle="`${pairingStore.incoming.length} 个请求等待确认`"
    >
      <div class="cm-list">
        <div v-for="prompt in pairingStore.incoming" :key="prompt.deviceId" class="prompt">
          <DeviceCard
            :name="prompt.name"
            :platform="prompt.platform"
            :fingerprint="prompt.fingerprint"
            :address="prompt.address"
            :last-seen="prompt.requestedAt"
            :pairing="true"
            hide-fingerprint
          />
          <FingerprintBadge :fingerprint="prompt.fingerprint" label="与对方屏幕核对指纹" />
          <div class="prompt-actions">
            <AppButton
              variant="primary"
              size="lg"
              block
              icon="check"
              :loading="pairingStore.isBusy(prompt.deviceId)"
              @click="respond(prompt.deviceId, prompt.name, true)"
            >
              接受
            </AppButton>
            <AppButton
              variant="ghost"
              size="lg"
              block
              icon="close"
              :disabled="pairingStore.isBusy(prompt.deviceId)"
              @click="respond(prompt.deviceId, prompt.name, false)"
            >
              拒绝
            </AppButton>
          </div>
        </div>
      </div>
    </AppCard>

    <AppCard title="发现的设备" icon="radar" :subtitle="`${discovered.length} 台等待配对`">
      <div v-if="discovered.length" class="cm-list">
        <DeviceCard
          v-for="peer in discovered"
          :key="peer.deviceId"
          :name="peer.name"
          :platform="peer.platform"
          :fingerprint="peer.fingerprint"
          :online="peer.connected"
          :pairing="peer.pairing"
          :last-seen="peer.lastSeen"
          :busy="peersStore.isBusy(peer.deviceId)"
          :subtitle="peer.address"
          hide-fingerprint
        >
          <template #actions>
            <AppButton
              size="sm"
              variant="primary"
              icon="link"
              :disabled="peer.pairing"
              :loading="peersStore.isBusy(peer.deviceId)"
              @click="pair(peer.deviceId)"
            >
              {{ peer.pairing ? "等待中" : "配对" }}
            </AppButton>
          </template>
        </DeviceCard>
      </div>
      <EmptyState v-else compact icon="radar" title="没有发现新设备" />
    </AppCard>

    <AppCard
      title="已信任的设备"
      icon="shield"
      tone="success"
      :subtitle="`${trustedStore.count} 台 · ${trustedStore.onlineCount} 台在线`"
    >
      <div v-if="trustedRows.length" class="cm-list">
        <DeviceCard
          v-for="row in trustedRows"
          :key="row.device.deviceId"
          :name="row.device.name"
          :platform="row.device.platform"
          :fingerprint="row.device.fingerprint"
          :online="row.online"
          :trusted="true"
          :last-seen="row.device.trustedAt"
          :subtitle="`信任于 ${new Date(row.device.trustedAt).toLocaleDateString()}`"
          hide-fingerprint
        >
          <template #actions>
            <AppButton
              size="sm"
              variant="danger"
              icon="trash"
              :loading="trustedStore.busyId === row.device.deviceId"
              @click="pendingUnpair = row.device"
            >
              解除
            </AppButton>
          </template>
        </DeviceCard>
      </div>
      <EmptyState
        v-else
        compact
        icon="shield"
        title="还没有信任任何设备"
        description="先在上面找一台设备完成配对。"
      />
    </AppCard>

    <AppCard v-if="pendingUnpair" tone="danger" title="解除信任？" icon="alert">
      <p class="confirm-text">
        {{ pendingUnpair.name }} 会被移出信任列表并断开，需要重新配对才能同步。
      </p>
      <div class="prompt-actions">
        <AppButton variant="ghost" size="lg" block :disabled="unpairing" @click="pendingUnpair = null">
          取消
        </AppButton>
        <AppButton variant="danger" size="lg" block :loading="unpairing" @click="confirmUnpair">
          解除信任
        </AppButton>
      </div>
    </AppCard>
  </div>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.top {
  display: flex;
  align-items: center;
  gap: 8px;
}

.spacer {
  flex: 1;
}

.prompt {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px;
  border: 1px solid var(--border-accent);
  border-radius: var(--radius);
  background: var(--surface-2);
}

.prompt-actions {
  display: flex;
  gap: 8px;
}

.prompt-actions > * {
  flex: 1;
}

.confirm-text {
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1.55;
  margin-bottom: 12px;
}
</style>
