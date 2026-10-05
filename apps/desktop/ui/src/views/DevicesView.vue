<script setup lang="ts">
import { computed, ref } from "vue";

import {
  AppButton,
  AppCard,
  AppIcon,
  ConfirmDialog,
  DeviceCard,
  EmptyState,
  FingerprintBadge,
  StatusPill,
  toMessage,
  usePairingStore,
  usePeersStore,
  useStatusStore,
  useToast,
  useTrustedStore,
  type PairingPrompt,
  type TrustedDeviceView,
} from "@clipmesh/ui-core";

/**
 * 设备页：最上面是等待决定的「配对请求」（收到的当场接受/拒绝，自己发出的只能等或取消），
 * 下面左边"发现的设备"（可以发起配对），右边"已信任的设备"（可以解除信任）。
 */
const peersStore = usePeersStore();
const trustedStore = useTrustedStore();
const pairingStore = usePairingStore();
const statusStore = useStatusStore();
const toast = useToast();

const query = ref("");
const refreshing = ref(false);
const pendingUnpair = ref<TrustedDeviceView | null>(null);
const unpairing = ref(false);

function matches(name: string, fingerprint: string, address = ""): boolean {
  const keyword = query.value.trim().toLowerCase();
  if (keyword === "") return true;
  return (
    name.toLowerCase().includes(keyword) ||
    fingerprint.toLowerCase().replace(/\s+/g, "").includes(keyword.replace(/\s+/g, "")) ||
    address.toLowerCase().includes(keyword)
  );
}

const discovered = computed(() =>
  peersStore.untrusted.filter((p) => matches(p.name, p.fingerprint, p.address)),
);

/** 已信任列表以 trusted store 为准，连接状态从 peers 里补。 */
const trustedRows = computed(() =>
  trustedStore.devices
    .filter((d) => matches(d.name, d.fingerprint))
    .map((device) => {
      const peer = peersStore.byId(device.deviceId);
      return {
        device,
        online: peer?.connected ?? device.online,
        address: peer?.address ?? "",
        lastSeen: peer?.lastSeen,
      };
    }),
);

async function refresh(): Promise<void> {
  refreshing.value = true;
  try {
    await Promise.all([peersStore.refresh(), trustedStore.refresh(), pairingStore.refresh()]);
    toast.info("已刷新设备列表");
  } catch (cause) {
    toast.error("刷新失败", toMessage(cause));
  } finally {
    refreshing.value = false;
  }
}

async function pair(deviceId: string): Promise<void> {
  try {
    await peersStore.pair(deviceId);
    toast.info("已发送配对请求", "等待对方在其设备上确认指纹。");
  } catch (cause) {
    toast.error("配对失败", toMessage(cause));
  }
}

/** 别人请求我们 —— 核对指纹后接受或拒绝。 */
async function respond(prompt: PairingPrompt, accept: boolean): Promise<void> {
  try {
    await pairingStore.respond(prompt.deviceId, accept);
    if (accept) {
      toast.success("已接受配对", `${prompt.name} 现在可以接收你的剪贴板了。`);
    } else {
      toast.info("已拒绝配对", `${prompt.name} 不会收到任何内容。`);
    }
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  }
}

/**
 * 我们请求别人 —— 只能等，等不了就取消。store 里没有单独的 cancel 命令，
 * 所以复用 respond(deviceId, false)：core 会给对方回一个"不接受"并撤下这条请求。
 */
async function cancelOutgoing(prompt: PairingPrompt): Promise<void> {
  try {
    await pairingStore.respond(prompt.deviceId, false);
    toast.info("已取消配对请求", `${prompt.name} 不会再收到这次请求。`);
  } catch (cause) {
    toast.error("取消失败", toMessage(cause));
  }
}

function askUnpair(device: TrustedDeviceView): void {
  pendingUnpair.value = device;
}

async function confirmUnpair(): Promise<void> {
  const target = pendingUnpair.value;
  if (!target) return;
  unpairing.value = true;
  try {
    await trustedStore.unpair(target.deviceId);
    const peer = peersStore.byId(target.deviceId);
    if (peer) {
      peer.trusted = false;
      peer.connected = false;
    }
    toast.success("已解除信任", `${target.name} 需要重新配对才能同步。`);
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
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">设备</h1>
        <p class="cm-page-sub">
          本机在 <span class="cm-mono">{{ statusStore.listenPort }}</span> 端口监听，
          通过 mDNS 发现同一局域网内的其他 ClipMesh 节点。发现 ≠ 信任，必须先配对。
        </p>
      </div>
      <div class="cm-page-actions">
        <div class="cm-search">
          <span class="cm-search-icon"><AppIcon name="search" :size="15" /></span>
          <input v-model="query" class="cm-input" type="search" placeholder="按名称 / 指纹 / 地址筛选" />
        </div>
        <AppButton icon="refresh" :loading="refreshing" @click="refresh">刷新</AppButton>
      </div>
    </header>

    <!-- 配对请求：收到的当场做决定，自己发出的显示"等待中"并可以取消 -->
    <div v-if="pairingStore.incoming.length || pairingStore.outgoing.length" class="prompts">
      <AppCard
        v-if="pairingStore.incoming.length"
        tone="accent"
        title="配对请求"
        icon="link"
        :subtitle="`${pairingStore.incoming.length} 个请求等待确认`"
      >
        <div class="requests">
          <article v-for="prompt in pairingStore.incoming" :key="prompt.deviceId" class="request">
            <DeviceCard
              :name="prompt.name"
              :platform="prompt.platform"
              :fingerprint="prompt.fingerprint"
              :address="prompt.address"
              :last-seen="prompt.requestedAt"
              :pairing="true"
              :busy="pairingStore.isBusy(prompt.deviceId)"
              hide-fingerprint
            />

            <div class="verify">
              <FingerprintBadge
                :fingerprint="prompt.fingerprint"
                label="与对方屏幕核对指纹"
                size="lg"
              />
              <div class="prompt-actions">
                <AppButton
                  variant="primary"
                  icon="check"
                  :loading="pairingStore.isBusy(prompt.deviceId)"
                  @click="respond(prompt, true)"
                >
                  接受
                </AppButton>
                <AppButton
                  variant="ghost"
                  icon="close"
                  :disabled="pairingStore.isBusy(prompt.deviceId)"
                  @click="respond(prompt, false)"
                >
                  拒绝
                </AppButton>
              </div>
            </div>
          </article>
        </div>
      </AppCard>

      <AppCard
        v-if="pairingStore.outgoing.length"
        title="我发出的请求"
        icon="send"
        :subtitle="`${pairingStore.outgoing.length} 个请求等待对方确认`"
      >
        <div class="cm-list">
          <DeviceCard
            v-for="prompt in pairingStore.outgoing"
            :key="prompt.deviceId"
            :name="prompt.name"
            :platform="prompt.platform"
            :fingerprint="prompt.fingerprint"
            :address="prompt.address"
            :last-seen="prompt.requestedAt"
            :pairing="true"
            :busy="pairingStore.isBusy(prompt.deviceId)"
            hide-fingerprint
          >
            <template #actions>
              <AppButton
                size="sm"
                variant="ghost"
                icon="close"
                :disabled="pairingStore.isBusy(prompt.deviceId)"
                @click="cancelOutgoing(prompt)"
              >
                取消
              </AppButton>
            </template>
          </DeviceCard>
        </div>
      </AppCard>
    </div>

    <div class="cols">
      <AppCard
        title="发现的设备"
        icon="radar"
        :subtitle="`${discovered.length} 台等待配对`"
        tone="default"
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
            :address="peer.address"
            :last-seen="peer.lastSeen"
            :busy="peersStore.isBusy(peer.deviceId)"
          >
            <template #actions>
              <StatusPill
                v-if="peer.pairing"
                label="等待对方确认"
                tone="accent"
                pulse
                size="sm"
              />
              <AppButton
                v-else
                size="sm"
                variant="primary"
                icon="link"
                :loading="peersStore.isBusy(peer.deviceId)"
                @click="pair(peer.deviceId)"
              >
                配对
              </AppButton>
            </template>
          </DeviceCard>
        </div>
        <EmptyState
          v-else
          compact
          icon="radar"
          title="没有待配对的设备"
          description="同一局域网内的新设备会自动出现在这里。"
        />
      </AppCard>

      <AppCard
        title="已信任的设备"
        icon="shield"
        :subtitle="`${trustedStore.count} 台 · ${trustedStore.onlineCount} 台在线`"
        tone="success"
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
            :address="row.address"
            :last-seen="row.lastSeen"
            :busy="trustedStore.busyId === row.device.deviceId"
          >
            <template #actions>
              <AppButton
                size="sm"
                variant="danger"
                icon="close"
                :loading="trustedStore.busyId === row.device.deviceId"
                @click="askUnpair(row.device)"
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
          description="在左边找到设备并完成一次配对，之后就能互相同步剪贴板。"
        />
      </AppCard>
    </div>

    <ConfirmDialog
      :open="pendingUnpair !== null"
      tone="danger"
      title="解除信任？"
      :message="`${pendingUnpair?.name ?? ''} 会被移出信任列表并断开当前会话，需要重新配对才能同步。`"
      confirm-label="解除信任"
      :busy="unpairing"
      @cancel="pendingUnpair = null"
      @confirm="confirmUnpair"
    />
  </div>
</template>

<style scoped>
/* ---------- 配对请求 ---------- */

.prompts {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-bottom: var(--space-4);
}

.requests {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.request {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border-accent);
  border-radius: var(--radius);
  background: var(--surface-2);
}

.verify {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface);
}

.prompt-actions {
  display: flex;
  gap: 8px;
}

/* ---------- 设备列表 ---------- */

.cols {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: var(--space-4);
  align-items: start;
}

@media (max-width: 1080px) {
  .cols {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
