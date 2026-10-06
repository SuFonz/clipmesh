<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";

import {
  AppButton,
  AppCard,
  AppIcon,
  DeviceCard,
  EmptyState,
  FingerprintBadge,
  StatusPill,
  copyToClipboard,
  platformLabel,
  sendClipboard,
  t,
  toMessage,
  useIdentityStore,
  usePeersStore,
  useSettingsStore,
  useStatusStore,
  useToast,
} from "@clipmesh/ui-core";

/**
 * 首页：只放「本机」和「在线设备」两块。
 * 每块内部的卡片用 auto-fit 网格排布 —— 窗口窄就一行一张，宽就并排。
 */
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const settingsStore = useSettingsStore();
const identityStore = useIdentityStore();
const toast = useToast();

const draftName = ref("");
const savingName = ref(false);
const togglingEngine = ref(false);
const sendingClipboard = ref(false);

const identity = computed(() => identityStore.identity);
/** 只列已经建立 TLS 会话的设备 —— 这一块的标题就是「在线设备」。 */
const onlinePeers = computed(() => peersStore.online);

const nameChanged = computed<boolean>(
  () =>
    draftName.value.trim() !== "" && draftName.value.trim() !== settingsStore.settings?.deviceName,
);

watch(
  () => settingsStore.settings?.deviceName,
  (name) => {
    if (name !== undefined) draftName.value = name;
  },
  { immediate: true },
);

async function saveName(): Promise<void> {
  if (!nameChanged.value) return;
  savingName.value = true;
  try {
    await settingsStore.setDeviceName(draftName.value);
    toast.success(t("common.toast.renamed"), t("common.toast.renamedDescription"));
  } catch (cause) {
    toast.error(t("common.toast.renameFailed"), toMessage(cause));
  } finally {
    savingName.value = false;
  }
}

/** 复制一段文本；成功文案由调用方给（设备 ID 与公钥的措辞不一样）。 */
async function copy(value: string, successMessage: string): Promise<void> {
  const ok = await copyToClipboard(value);
  if (ok) toast.success(successMessage);
  else toast.error(t("common.copyFailed"));
}

async function exportCert(): Promise<void> {
  const ok = identityStore.exportCertificate();
  if (ok) {
    toast.success(t("common.toast.certExported"), t("common.toast.certExportedDescription"));
  } else {
    toast.error(t("common.toast.noCertificate"));
  }
}

async function toggleEngine(): Promise<void> {
  togglingEngine.value = true;
  try {
    if (statusStore.running) {
      await statusStore.stop();
      toast.info(t("desktop.engine.stoppedToast"), t("desktop.engine.stoppedDescription"));
    } else {
      await statusStore.start();
      toast.success(
        t("desktop.engine.startedToast"),
        t("desktop.home.toast.engineStartedDescription"),
      );
    }
  } catch (cause) {
    toast.error(t("common.actionFailed"), toMessage(cause));
  } finally {
    togglingEngine.value = false;
  }
}

/**
 * 把本机剪贴板广播给已配对的设备。
 *
 * 提示由这里给而不是靠事件：`clipmesh://clipboard-sent` 只从自动同步的监听路径
 * 发出（`manager.rs` 的 `handle_local_change`），手动发送不经过它，所以不会重复弹。
 */
async function sendLocalClipboard(): Promise<void> {
  sendingClipboard.value = true;
  try {
    const result = await sendClipboard();
    if (result.delivered > 0) {
      toast.success(
        t("desktop.home.toast.sent"),
        t("desktop.home.toast.sentDescription", { count: result.delivered }),
      );
    } else {
      toast.warn(t("common.noOnlineDevices"), t("common.toast.noOnlineDevices.description"));
    }
  } catch (cause) {
    toast.error(t("desktop.home.toast.sendFailed"), toMessage(cause));
  } finally {
    sendingClipboard.value = false;
  }
}

/**
 * 关掉错误横幅。
 *
 * 必须走后端：横幅读的是状态快照里的 `lastError`，只清前端变量的话，
 * 下一条 `clipmesh://status` 会把它原样带回来，看起来就是"关不掉"。
 */
async function dismissError(): Promise<void> {
  try {
    await statusStore.clearError();
  } catch (cause) {
    toast.error(t("desktop.home.toast.clearErrorFailed"), toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">{{ t("desktop.nav.home") }}</h1>
        <p class="cm-page-sub">
          {{ statusStore.deviceName }} ·
          {{ statusStore.running ? t("desktop.home.engineRunning") : t("desktop.home.engineStopped") }}
          · {{ t("desktop.home.devicesOnline", { count: onlinePeers.length }) }}
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill
          :label="statusStore.running ? t('desktop.status.running') : t('desktop.status.stopped')"
          :tone="statusStore.running ? 'ok' : 'idle'"
          :pulse="statusStore.running"
        />
        <StatusPill
          :label="t('desktop.home.discovered', { count: peersStore.count })"
          tone="info"
          icon="radar"
        />
        <AppButton
          variant="primary"
          icon="send"
          :loading="sendingClipboard"
          :disabled="sendingClipboard"
          @click="sendLocalClipboard"
        >
          {{ t("desktop.home.sendClipboard") }}
        </AppButton>
        <AppButton
          :variant="statusStore.running ? 'secondary' : 'primary'"
          :icon="statusStore.running ? 'power' : 'zap'"
          :loading="togglingEngine"
          @click="toggleEngine"
        >
          {{ statusStore.running ? t("desktop.engine.stopTitle") : t("desktop.engine.startTitle") }}
        </AppButton>
      </div>
    </header>

    <div v-if="statusStore.lastError" class="banner" role="alert">
      <AppIcon name="alert" :size="16" />
      <span class="banner-text">{{ statusStore.lastError }}</span>
      <AppButton size="sm" variant="ghost" @click="dismissError">
        {{ t("desktop.home.dismissError") }}
      </AppButton>
    </div>

    <div class="sections">
      <!-- 本机 -->
      <section class="sec">
        <header class="sec-head">
          <span class="sec-icon"><AppIcon name="monitor" :size="16" /></span>
          <div class="sec-text">
            <h2 class="sec-title">{{ t("common.thisDevice") }}</h2>
            <p class="sec-sub">{{ t("desktop.home.thisDeviceSubtitle") }}</p>
          </div>
        </header>

        <div class="sec-grid">
          <AppCard
            :title="t('desktop.home.identity.title')"
            icon="key"
            :subtitle="t('desktop.home.identity.subtitle')"
          >
            <div class="field-row">
              <label class="cm-field grow">
                <span class="cm-label">{{ t("common.deviceName") }}</span>
                <input
                  v-model="draftName"
                  class="cm-input"
                  type="text"
                  maxlength="64"
                  :placeholder="t('desktop.home.identity.deviceNamePlaceholder')"
                  @keydown.enter="saveName"
                />
              </label>
              <AppButton
                variant="primary"
                icon="check"
                :disabled="!nameChanged"
                :loading="savingName"
                @click="saveName"
              >
                {{ t("common.save") }}
              </AppButton>
            </div>
            <p class="cm-help name-help">
              {{ t("common.currentName")
              }}<b>{{ settingsStore.settings?.deviceName ?? statusStore.deviceName }}</b
              >{{ t("common.currentNameAfter") }}
            </p>

            <div v-if="identity" class="identity">
              <FingerprintBadge
                :fingerprint="identity.fingerprint"
                :label="t('common.certificateFingerprint')"
                size="lg"
              />

              <div class="kv">
                <span class="k">{{ t("common.deviceId") }}</span>
                <span class="v cm-mono">{{ identity.deviceId }}</span>
                <AppButton
                  size="sm"
                  variant="ghost"
                  icon="copy"
                  icon-only
                  :title="t('common.copyDeviceId')"
                  @click="copy(identity.deviceId, t('common.copiedDeviceId'))"
                />
              </div>

              <div class="kv">
                <span class="k">{{ t("common.platform") }}</span>
                <span class="v">{{ platformLabel(identity.platform) }}</span>
              </div>

              <div class="kv">
                <span class="k">{{ t("common.publicKey") }}</span>
                <span class="v cm-mono clamp">{{ identity.publicKey }}</span>
                <AppButton
                  size="sm"
                  variant="ghost"
                  icon="copy"
                  icon-only
                  :title="t('common.copyPublicKey')"
                  @click="copy(identity.publicKey, t('common.copiedPublicKey'))"
                />
              </div>

              <div class="cert">
                <div class="cert-head">
                  <span class="cm-label">{{ t("common.certificate") }}</span>
                  <AppButton size="sm" icon="download" @click="exportCert">
                    {{ t("common.exportCertificate") }}
                  </AppButton>
                </div>
                <pre class="cm-mono cert-body">{{ identity.certificatePem }}</pre>
                <p class="cm-help">
                  {{ t("common.fingerprintNote") }}
                </p>
              </div>
            </div>
            <div v-else class="loading">{{ t("common.identityLoading") }}</div>
          </AppCard>

          <AppCard
            :title="t('desktop.home.runtime.title')"
            icon="radar"
            :subtitle="t('desktop.home.runtime.subtitle')"
          >
            <div class="stats">
              <div class="stat">
                <span class="k">{{ t("common.engine") }}</span>
                <span class="v">
                  <StatusPill
                    :label="
                      statusStore.running ? t('desktop.status.running') : t('desktop.status.stopped')
                    "
                    :tone="statusStore.running ? 'ok' : 'idle'"
                    :pulse="statusStore.running"
                    size="sm"
                  />
                </span>
              </div>
              <div class="stat">
                <span class="k">{{ t("common.listenPort") }}</span>
                <span class="v cm-mono">{{ statusStore.listenPort }}</span>
              </div>
              <div class="stat">
                <span class="k">{{ t("common.autoSync") }}</span>
                <span class="v">
                  <StatusPill
                    :label="
                      statusStore.autoSync
                        ? t('desktop.home.runtime.autoSyncOn')
                        : t('desktop.home.runtime.autoSyncOff')
                    "
                    :tone="statusStore.autoSync ? 'ok' : 'idle'"
                    size="sm"
                  />
                </span>
              </div>
              <div class="stat">
                <span class="k">{{ t("desktop.home.runtime.trustedDevices") }}</span>
                <span class="v">{{
                  t("desktop.home.runtime.deviceCount", { count: statusStore.trustedPeers })
                }}</span>
              </div>
            </div>
            <p class="cm-help stats-help">
              {{ t("desktop.home.runtime.settingsHintBefore")
              }}<RouterLink to="/settings" class="link">{{ t("desktop.nav.settings") }}</RouterLink
              >{{ t("desktop.home.runtime.settingsHintAfter") }}
            </p>
          </AppCard>
        </div>
      </section>

      <!-- 在线设备 -->
      <section class="sec">
        <header class="sec-head">
          <span class="sec-icon"><AppIcon name="devices" :size="16" /></span>
          <div class="sec-text">
            <h2 class="sec-title">{{ t("common.onlineDevices") }}</h2>
            <p class="sec-sub">
              {{
                t("common.onlineDevicesSubtitle", {
                  online: onlinePeers.length,
                  discovered: peersStore.count,
                })
              }}
            </p>
          </div>
          <div class="sec-actions">
            <RouterLink to="/devices" class="link">{{ t("desktop.home.online.manage") }}</RouterLink>
          </div>
        </header>

        <div v-if="onlinePeers.length" class="sec-grid">
          <DeviceCard
            v-for="peer in onlinePeers"
            :key="peer.deviceId"
            :name="peer.name"
            :platform="peer.platform"
            :fingerprint="peer.fingerprint"
            :online="peer.connected"
            :trusted="peer.trusted"
            :pairing="peer.pairing"
            :address="peer.address"
            :last-seen="peer.lastSeen"
          />
        </div>
        <AppCard v-else flush>
          <EmptyState
            compact
            icon="radar"
            :title="t('desktop.home.online.empty.title')"
            :description="t('desktop.home.online.empty.description')"
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

/* ---------- 区块 ---------- */

.sections {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.sec-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.sec-icon {
  display: grid;
  place-items: center;
  flex: none;
  width: 28px;
  height: 28px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
  color: var(--text-muted);
}

.sec-text {
  min-width: 0;
}

.sec-title {
  font-size: 14.5px;
  font-weight: 650;
  letter-spacing: -0.01em;
  line-height: 1.3;
}

.sec-sub {
  color: var(--text-dim);
  font-size: 12px;
}

.sec-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-left: auto;
}

/* 卡片行：按可用宽度自动换行 */
.sec-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
  gap: var(--space-4);
  align-items: start;
}

.link {
  font-size: 12.5px;
  font-weight: 600;
}

/* ---------- 本机身份 ---------- */

.field-row {
  display: flex;
  align-items: flex-end;
  gap: var(--space-3);
}

.grow {
  flex: 1;
  min-width: 0;
}

.name-help {
  margin-top: 6px;
}

.identity {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: var(--space-4);
}

.kv {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.k {
  flex: none;
  width: 62px;
  color: var(--text-dim);
  font-size: 11.5px;
  font-weight: 600;
}

.v {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

.v.clamp {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.cert {
  padding: 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.cert-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
}

.cert-body {
  max-height: 148px;
  margin: 0 0 8px;
  padding: 9px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
  background: var(--bg);
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.loading {
  padding: 18px 0;
  color: var(--text-dim);
  font-size: 13px;
}

/* ---------- 运行状态 ---------- */

.stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 10px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 9px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.stats-help {
  margin-top: var(--space-4);
}
</style>
