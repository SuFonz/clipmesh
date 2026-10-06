<script setup lang="ts">
import { computed, ref } from "vue";

import {
  AppButton,
  AppCard,
  DeviceCard,
  EmptyState,
  FingerprintBadge,
  StatusPill,
  t,
  toMessage,
  useI18n,
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
/** 「信任于 2025/3/5」里的日期也要跟着界面语言走，不能用系统 locale 直接格式化。 */
const { locale } = useI18n();

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
    toast.error(t("common.refreshFailed"), toMessage(cause));
  } finally {
    refreshing.value = false;
  }
}

async function pair(deviceId: string): Promise<void> {
  try {
    await peersStore.pair(deviceId);
    toast.info(
      t("android.devices.toast.pairRequested"),
      t("android.devices.toast.pairRequestedDescription"),
    );
  } catch (cause) {
    toast.error(t("android.devices.toast.pairFailed"), toMessage(cause));
  }
}

async function respond(deviceId: string, name: string, accept: boolean): Promise<void> {
  try {
    await pairingStore.respond(deviceId, accept);
    toast[accept ? "success" : "info"](
      accept
        ? t("android.devices.toast.trusted", { name })
        : t("android.devices.toast.rejected", { name }),
    );
  } catch (cause) {
    toast.error(t("common.actionFailed"), toMessage(cause));
  }
}

async function confirmUnpair(): Promise<void> {
  const target = pendingUnpair.value;
  if (!target) return;
  unpairing.value = true;
  try {
    await trustedStore.unpair(target.deviceId);
    toast.success(t("android.devices.toast.unpaired"), target.name);
    pendingUnpair.value = null;
  } catch (cause) {
    toast.error(t("android.devices.toast.unpairFailed"), toMessage(cause));
  } finally {
    unpairing.value = false;
  }
}
</script>

<template>
  <div class="view">
    <div class="top">
      <StatusPill
        :label="t('android.devices.discoveredCount', { count: peersStore.count })"
        tone="info"
        icon="radar"
        size="sm"
      />
      <span class="spacer" />
      <AppButton size="sm" icon="refresh" :loading="refreshing" @click="refresh">
        {{ t("common.refresh") }}
      </AppButton>
    </div>

    <AppCard
      v-if="pairingStore.incoming.length"
      tone="accent"
      :title="t('android.devices.incoming.title')"
      icon="link"
      :subtitle="t('android.devices.incoming.subtitle', { count: pairingStore.incoming.length })"
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
          <FingerprintBadge
            :fingerprint="prompt.fingerprint"
            :label="t('android.devices.verifyFingerprint')"
          />
          <div class="prompt-actions">
            <AppButton
              variant="primary"
              size="lg"
              block
              icon="check"
              :loading="pairingStore.isBusy(prompt.deviceId)"
              @click="respond(prompt.deviceId, prompt.name, true)"
            >
              {{ t("android.devices.accept") }}
            </AppButton>
            <AppButton
              variant="ghost"
              size="lg"
              block
              icon="close"
              :disabled="pairingStore.isBusy(prompt.deviceId)"
              @click="respond(prompt.deviceId, prompt.name, false)"
            >
              {{ t("android.devices.reject") }}
            </AppButton>
          </div>
        </div>
      </div>
    </AppCard>

    <AppCard
      :title="t('android.devices.discovered.title')"
      icon="radar"
      :subtitle="t('android.devices.discovered.subtitle', { count: discovered.length })"
    >
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
              {{ peer.pairing ? t("android.devices.waiting") : t("android.devices.pair") }}
            </AppButton>
          </template>
        </DeviceCard>
      </div>
      <EmptyState
        v-else
        compact
        icon="radar"
        :title="t('android.devices.discovered.empty.title')"
      />
    </AppCard>

    <AppCard
      :title="t('android.devices.trusted.title')"
      icon="shield"
      tone="success"
      :subtitle="
        t('android.devices.trusted.subtitle', {
          count: trustedStore.count,
          online: trustedStore.onlineCount,
        })
      "
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
          :subtitle="
            t('android.devices.trustedAt', {
              date: new Date(row.device.trustedAt).toLocaleDateString(locale),
            })
          "
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
              {{ t("android.devices.unpair") }}
            </AppButton>
          </template>
        </DeviceCard>
      </div>
      <EmptyState
        v-else
        compact
        icon="shield"
        :title="t('android.devices.trusted.empty.title')"
        :description="t('android.devices.trusted.empty.description')"
      />
    </AppCard>

    <AppCard
      v-if="pendingUnpair"
      tone="danger"
      :title="t('android.devices.unpairDialog.title')"
      icon="alert"
    >
      <p class="confirm-text">
        {{ t("android.devices.unpairDialog.message", { name: pendingUnpair.name }) }}
      </p>
      <div class="prompt-actions">
        <AppButton
          variant="ghost"
          size="lg"
          block
          :disabled="unpairing"
          @click="pendingUnpair = null"
        >
          {{ t("common.cancel") }}
        </AppButton>
        <AppButton variant="danger" size="lg" block :loading="unpairing" @click="confirmUnpair">
          {{ t("android.devices.unpairDialog.confirm") }}
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
