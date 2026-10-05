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
  toMessage,
  useIdentityStore,
  usePeersStore,
  useSettingsStore,
  useStatusStore,
  useToast,
} from "@clipmesh/ui-core";

/**
 * Android 首页：结构对齐桌面端 —— 顶部保留「广播剪贴板」主按钮（对应通知栏那颗
 * 按钮的行为），下面依次是「本机」和「在线设备」两个区块。
 *
 * 前台服务开关在设置页：打开它的时候会顺带申请通知权限，所以这里不再重复放一个
 * 「申请通知权限」的按钮。
 */
const statusStore = useStatusStore();
const peersStore = usePeersStore();
const settingsStore = useSettingsStore();
const identityStore = useIdentityStore();
const toast = useToast();

const sendingClipboard = ref(false);
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

async function saveName(): Promise<void> {
  if (!nameChanged.value) return;
  savingName.value = true;
  try {
    await settingsStore.setDeviceName(draftName.value);
    toast.success("设备名已更新", "mDNS 已重新广播，其他设备会看到新名字。");
  } catch (cause) {
    toast.error("改名失败", toMessage(cause));
  } finally {
    savingName.value = false;
  }
}

async function copy(value: string, label: string): Promise<void> {
  const ok = await copyToClipboard(value);
  if (ok) toast.success(`${label}已复制`);
  else toast.error("复制失败");
}

function exportCert(): void {
  if (identityStore.exportCertificate()) {
    toast.success("证书已导出", "可以把 .pem 文件发给对方，用来人工核对指纹。");
  } else {
    toast.error("没有可导出的证书");
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

    <div class="sections">
      <!-- 本机 -->
      <section class="sec">
        <header class="sec-head">
          <span class="sec-icon"><AppIcon name="phone" :size="16" /></span>
          <div class="sec-text">
            <h2 class="sec-title">本机</h2>
            <p class="sec-sub">名称、身份与证书</p>
          </div>
        </header>

        <div class="sec-grid">
          <AppCard title="本机名称" icon="phone" subtitle="同一网络里的其他设备会看到这个名字">
            <input
              v-model="draftName"
              class="cm-input"
              type="text"
              maxlength="64"
              placeholder="例如：我的 Pixel"
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
              保存名称
            </AppButton>
            <p class="cm-help mt-sm">
              当前生效：<b>{{ settingsStore.settings?.deviceName ?? statusStore.deviceName }}</b>，
              改名后会重新广播 mDNS。
            </p>
          </AppCard>

          <AppCard title="本机身份" icon="shield" subtitle="用于核对配对，不会上传到任何服务器">
            <template v-if="identity">
              <FingerprintBadge :fingerprint="identity.fingerprint" label="证书指纹" />

              <div class="rows">
                <div class="row">
                  <span class="k">设备 ID</span>
                  <span class="v cm-mono cm-truncate">{{ identity.deviceId }}</span>
                </div>
                <div class="row">
                  <span class="k">平台</span>
                  <span class="v">{{ platformLabel(identity.platform) }}</span>
                </div>
                <div class="row">
                  <span class="k">公钥</span>
                  <span class="v cm-mono cm-truncate">{{ identity.publicKey }}</span>
                </div>
              </div>

              <div class="actions">
                <AppButton block icon="copy" @click="copy(identity.deviceId, '设备 ID ')">
                  复制设备 ID
                </AppButton>
                <AppButton block icon="copy" @click="copy(identity.publicKey, '公钥')">
                  复制公钥
                </AppButton>
              </div>

              <div class="cert">
                <div class="cert-head">
                  <span class="cm-label">设备证书（PEM）</span>
                  <AppButton size="sm" icon="download" @click="exportCert">导出证书</AppButton>
                </div>
                <pre class="cm-mono cert-body">{{ identity.certificatePem }}</pre>
                <p class="cm-help">
                  对方应该能在自己的设备上看到同一串指纹。指纹不同 = 有人在中间，别继续。
                </p>
              </div>
            </template>
            <p v-else class="cm-help">正在读取身份信息…</p>
          </AppCard>
        </div>
      </section>

      <!-- 在线设备 -->
      <section class="sec">
        <header class="sec-head">
          <span class="sec-icon"><AppIcon name="devices" :size="16" /></span>
          <div class="sec-text">
            <h2 class="sec-title">在线设备</h2>
            <p class="sec-sub">
              {{ onlinePeers.length }} 台已连接 · {{ peersStore.count }} 台被发现
            </p>
          </div>
          <div class="sec-actions">
            <RouterLink to="/devices" class="link">管理</RouterLink>
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
            title="没有在线设备"
            description="确认对方也开着 ClipMesh，并且在同一网络里。"
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

/* ---------- 广播剪贴板 ---------- */

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
