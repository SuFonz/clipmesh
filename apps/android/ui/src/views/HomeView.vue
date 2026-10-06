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
  copyToClipboard,
  platformLabel,
  t,
  toMessage,
  useIdentityStore,
  usePeersStore,
  useSettingsStore,
  useStatusStore,
  useToast,
} from "@clipmesh/ui-core";

/**
 * Android 首页：结构对齐桌面端 —— 一条操作行（只有「广播剪贴板」），下面依次是
 * 「本机」和「在线设备」两个区块。
 *
 * 引擎开关不在这里：外壳 MobileLayout 的顶栏已经有一颗，所有标签页共用。放两份
 * 只会让人猜哪个才算数。
 *
 * 页面标题和「引擎运行中 / N 台在线」的状态副标题同样由外壳提供，设备名在
 * 「本机名称」卡片里、平台在「本机身份」卡片里、监听端口在设置页的「关于」卡片里
 * —— 所以这一页不再自己放一张状态卡片，免得同一个屏幕上出现两遍。
 *
 * 前台服务开关在设置页：打开它的时候会顺带申请通知权限，所以这里不再重复放一个
 * 「申请通知权限」的按钮。
 */
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const settingsStore = useSettingsStore();
const identityStore = useIdentityStore();
const toast = useToast();

const draftName = ref("");
const savingName = ref(false);

const identity = computed(() => identityStore.identity);
/** 只列已经建立 TLS 会话的设备 —— 这一块的标题就是「在线设备」。 */
const onlinePeers = computed(() => peersStore.online);

watch(
  () => settingsStore.settings?.deviceName,
  (name) => {
    if (name !== undefined) draftName.value = name;
  },
  { immediate: true },
);

const nameChanged = computed<boolean>(
  () =>
    draftName.value.trim() !== "" && draftName.value.trim() !== settingsStore.settings?.deviceName,
);

/**
 * 「广播剪贴板」和引擎开关都在外壳 `MobileLayout` 的顶栏里 —— 两个都是全局动作，
 * 不属于某一页。通知栏那条广播链（轮询 + 发完退回原应用）也在那里，因为通知可能在
 * 任何标签页上被点开，挂在首页会漏掉。
 */

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

function exportCert(): void {
  if (identityStore.exportCertificate()) {
    toast.success(t("common.toast.certExported"), t("common.toast.certExportedDescription"));
  } else {
    toast.error(t("common.toast.noCertificate"));
  }
}
</script>

<template>
  <div class="view">
    <div class="sections">
      <!-- 本机 -->
      <section class="sec">
        <header class="sec-head">
          <span class="sec-icon"><AppIcon name="phone" :size="16" /></span>
          <div class="sec-text">
            <h2 class="sec-title">{{ t("common.thisDevice") }}</h2>
            <p class="sec-sub">{{ t("android.home.thisDeviceSubtitle") }}</p>
          </div>
        </header>

        <div class="sec-grid">
          <AppCard
            :title="t('android.home.name.title')"
            icon="phone"
            :subtitle="t('android.home.name.subtitle')"
          >
            <input
              v-model="draftName"
              class="cm-input"
              type="text"
              maxlength="64"
              :placeholder="t('android.home.name.placeholder')"
              @keydown.enter="saveName"
            />
            <AppButton
              class="mt"
              variant="primary"
              block
              icon="check"
              :disabled="!nameChanged"
              :loading="savingName"
              @click="saveName"
            >
              {{ t("android.home.name.save") }}
            </AppButton>
            <p class="cm-help mt-sm">
              {{ t("common.currentName")
              }}<b>{{ settingsStore.settings?.deviceName ?? statusStore.deviceName }}</b
              >{{ t("common.currentNameAfter") }}
            </p>
          </AppCard>

          <AppCard
            :title="t('android.home.identity.title')"
            icon="shield"
            :subtitle="t('android.home.identity.subtitle')"
          >
            <template v-if="identity">
              <FingerprintBadge
                :fingerprint="identity.fingerprint"
                :label="t('common.certificateFingerprint')"
              />

              <div class="rows">
                <div class="row">
                  <span class="k">{{ t("common.deviceId") }}</span>
                  <span class="v cm-mono cm-truncate">{{ identity.deviceId }}</span>
                </div>
                <div class="row">
                  <span class="k">{{ t("common.platform") }}</span>
                  <span class="v">{{ platformLabel(identity.platform) }}</span>
                </div>
                <div class="row">
                  <span class="k">{{ t("common.publicKey") }}</span>
                  <span class="v cm-mono cm-truncate">{{ identity.publicKey }}</span>
                </div>
              </div>

              <div class="actions">
                <AppButton
                  block
                  icon="copy"
                  @click="copy(identity.deviceId, t('common.copiedDeviceId'))"
                >
                  {{ t("common.copyDeviceId") }}
                </AppButton>
                <AppButton
                  block
                  icon="copy"
                  @click="copy(identity.publicKey, t('common.copiedPublicKey'))"
                >
                  {{ t("common.copyPublicKey") }}
                </AppButton>
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
            </template>
            <p v-else class="cm-help">{{ t("common.identityLoading") }}</p>
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
            <RouterLink to="/devices" class="link">{{ t("android.home.online.manage") }}</RouterLink>
          </div>
        </header>

        <div v-if="onlinePeers.length" class="sec-grid">
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
        <AppCard v-else flush>
          <EmptyState
            compact
            icon="radar"
            :title="t('common.noOnlineDevices')"
            :description="t('android.home.online.empty.description')"
          />
        </AppCard>
      </section>
    </div>
  </div>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* ---------- 区块 ---------- */

.sections {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
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

/*
 * 卡片行：按可用宽度自动换行。
 * `min(100%, 300px)` 而不是写死 300px —— 手机内容区比 300px 还窄时，
 * 写死的下限会把卡片顶出屏幕。
 */
.sec-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 300px), 1fr));
  gap: var(--space-3);
  align-items: start;
}

.link {
  font-size: 12.5px;
  font-weight: 600;
}

/* ---------- 本机身份 ---------- */

.rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 12px;
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.k {
  flex: none;
  width: 58px;
  color: var(--text-dim);
  font-size: 11.5px;
  font-weight: 600;
}

.v {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
}

.actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.actions > * {
  flex: 1;
}

.cert {
  margin-top: 12px;
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

.mt {
  margin-top: 12px;
}

.mt-sm {
  margin-top: 8px;
}
</style>
