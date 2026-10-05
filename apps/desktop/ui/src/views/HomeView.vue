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

async function exportCert(): Promise<void> {
  const ok = identityStore.exportCertificate();
  if (ok) {
    toast.success("证书已导出", "可以把 .pem 文件发给对方，用来人工核对指纹。");
  } else {
    toast.error("没有可导出的证书");
  }
}

async function toggleEngine(): Promise<void> {
  togglingEngine.value = true;
  try {
    if (statusStore.running) {
      await statusStore.stop();
      toast.info("引擎已停止", "不再监听剪贴板，也不再响应局域网请求。");
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
      toast.success("已发送剪贴板", `投递到 ${result.delivered} 台在线设备`);
    } else {
      toast.warn("没有在线设备", "内容已留在历史里，等设备上线后可重发。");
    }
  } catch (cause) {
    toast.error("发送失败", toMessage(cause));
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
    toast.error("无法清除这条错误", toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">首页</h1>
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
        <StatusPill :label="`发现 ${peersStore.count}`" tone="info" icon="radar" />
        <AppButton
          variant="primary"
          icon="send"
          :loading="sendingClipboard"
          :disabled="sendingClipboard"
          @click="sendLocalClipboard"
        >
          发送剪贴板
        </AppButton>
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
      <AppButton size="sm" variant="ghost" @click="dismissError">知道了</AppButton>
    </div>

    <div class="sections">
      <!-- 本机 -->
      <section class="sec">
        <header class="sec-head">
          <span class="sec-icon"><AppIcon name="monitor" :size="16" /></span>
          <div class="sec-text">
            <h2 class="sec-title">本机</h2>
            <p class="sec-sub">身份、证书与运行状态</p>
          </div>
        </header>

        <div class="sec-grid">
          <AppCard title="本机身份" icon="key" subtitle="长期保存，卸载重装会重新生成">
            <div class="field-row">
              <label class="cm-field grow">
                <span class="cm-label">设备名</span>
                <input
                  v-model="draftName"
                  class="cm-input"
                  type="text"
                  maxlength="64"
                  placeholder="例如：书房的台式机"
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
                保存
              </AppButton>
            </div>
            <p class="cm-help name-help">
              当前生效：<b>{{ settingsStore.settings?.deviceName ?? statusStore.deviceName }}</b>，
              改名后会重新广播 mDNS。
            </p>

            <div v-if="identity" class="identity">
              <FingerprintBadge :fingerprint="identity.fingerprint" label="证书指纹" size="lg" />

              <div class="kv">
                <span class="k">设备 ID</span>
                <span class="v cm-mono">{{ identity.deviceId }}</span>
                <AppButton
                  size="sm"
                  variant="ghost"
                  icon="copy"
                  icon-only
                  title="复制设备 ID"
                  @click="copy(identity.deviceId, '设备 ID ')"
                />
              </div>

              <div class="kv">
                <span class="k">平台</span>
                <span class="v">{{ platformLabel(identity.platform) }}</span>
              </div>

              <div class="kv">
                <span class="k">公钥</span>
                <span class="v cm-mono clamp">{{ identity.publicKey }}</span>
                <AppButton
                  size="sm"
                  variant="ghost"
                  icon="copy"
                  icon-only
                  title="复制公钥"
                  @click="copy(identity.publicKey, '公钥')"
                />
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
            </div>
            <div v-else class="loading">正在读取身份信息…</div>
          </AppCard>

          <AppCard title="运行状态" icon="radar" subtitle="监听端口与同步情况">
            <div class="stats">
              <div class="stat">
                <span class="k">引擎</span>
                <span class="v">
                  <StatusPill
                    :label="statusStore.running ? '运行中' : '已停止'"
                    :tone="statusStore.running ? 'ok' : 'idle'"
                    :pulse="statusStore.running"
                    size="sm"
                  />
                </span>
              </div>
              <div class="stat">
                <span class="k">监听端口</span>
                <span class="v cm-mono">{{ statusStore.listenPort }}</span>
              </div>
              <div class="stat">
                <span class="k">自动同步</span>
                <span class="v">
                  <StatusPill
                    :label="statusStore.autoSync ? '已开启' : '已关闭'"
                    :tone="statusStore.autoSync ? 'ok' : 'idle'"
                    size="sm"
                  />
                </span>
              </div>
              <div class="stat">
                <span class="k">已信任设备</span>
                <span class="v">{{ statusStore.trustedPeers }} 台</span>
              </div>
            </div>
            <p class="cm-help stats-help">
              自动同步、图片大小上限等开关在<RouterLink to="/settings" class="link">设置</RouterLink
              >里调整。
            </p>
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
            <RouterLink to="/devices" class="link">管理设备</RouterLink>
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
            title="当前没有在线设备"
            description="同一局域网里完成配对的设备上线后，会自动出现在这里。"
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
