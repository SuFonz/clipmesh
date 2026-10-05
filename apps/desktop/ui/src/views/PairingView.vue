<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";

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
} from "@clipmesh/ui-core";

/**
 * 配对页：把"需要用户做决定"的事情集中在一起。
 * 收到的请求要点接受/拒绝；自己发出去的请求只能等。
 */
const pairingStore = usePairingStore();
const peersStore = usePeersStore();
const toast = useToast();

const refreshing = ref(false);

const incoming = computed(() => pairingStore.incoming);
const outgoing = computed(() => pairingStore.outgoing);

async function respond(deviceId: string, name: string, accept: boolean): Promise<void> {
  try {
    await pairingStore.respond(deviceId, accept);
    if (accept) {
      toast.success("已接受配对", `${name} 现在可以接收你的剪贴板了。`);
    } else {
      toast.info("已拒绝配对", `${name} 不会收到任何内容。`);
    }
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  }
}

async function refresh(): Promise<void> {
  refreshing.value = true;
  try {
    await pairingStore.refresh();
    await peersStore.refresh();
  } catch (cause) {
    toast.error("刷新失败", toMessage(cause));
  } finally {
    refreshing.value = false;
  }
}

async function cancelOutgoing(deviceId: string): Promise<void> {
  try {
    await pairingStore.respond(deviceId, false);
    toast.info("已取消配对请求");
  } catch (cause) {
    toast.error("取消失败", toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">配对</h1>
        <p class="cm-page-sub">
          配对是一次性的信任建立过程：双方核对同一串证书指纹后，设备才会进入信任列表。
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill
          :label="`${incoming.length} 个待处理`"
          :tone="incoming.length > 0 ? 'warn' : 'idle'"
          :pulse="incoming.length > 0"
        />
        <AppButton icon="refresh" :loading="refreshing" @click="refresh">刷新</AppButton>
      </div>
    </header>

    <div class="cols">
      <AppCard
        title="收到的配对请求"
        icon="link"
        :subtitle="incoming.length ? `${incoming.length} 个请求等待你的决定` : '暂无请求'"
        :tone="incoming.length ? 'accent' : 'default'"
      >
        <div v-if="incoming.length" class="requests">
          <article v-for="prompt in incoming" :key="prompt.deviceId" class="request">
            <DeviceCard
              :name="prompt.name"
              :platform="prompt.platform"
              :fingerprint="prompt.fingerprint"
              :address="prompt.address"
              :last-seen="prompt.requestedAt"
              :pairing="true"
              :hide-fingerprint="true"
            />

            <div class="verify">
              <FingerprintBadge
                :fingerprint="prompt.fingerprint"
                label="对方证书指纹 —— 必须与对方屏幕上的一致"
                size="lg"
              />
              <div class="verify-actions">
                <AppButton
                  variant="primary"
                  icon="check"
                  :loading="pairingStore.isBusy(prompt.deviceId)"
                  @click="respond(prompt.deviceId, prompt.name, true)"
                >
                  接受
                </AppButton>
                <AppButton
                  variant="ghost"
                  icon="close"
                  :disabled="pairingStore.isBusy(prompt.deviceId)"
                  @click="respond(prompt.deviceId, prompt.name, false)"
                >
                  拒绝
                </AppButton>
              </div>
            </div>
          </article>
        </div>
        <EmptyState
          v-else
          compact
          icon="check"
          title="没有待处理的请求"
          description="有人在局域网里向你发起配对时，这里会亮起来。"
        />
      </AppCard>

      <AppCard title="我发出的请求" icon="send" subtitle="等待对方确认">
        <div v-if="outgoing.length" class="cm-list">
          <DeviceCard
            v-for="prompt in outgoing"
            :key="prompt.deviceId"
            :name="prompt.name"
            :platform="prompt.platform"
            :fingerprint="prompt.fingerprint"
            :address="prompt.address"
            :last-seen="prompt.requestedAt"
            :pairing="true"
          >
            <template #actions>
              <AppButton
                size="sm"
                variant="ghost"
                :disabled="pairingStore.isBusy(prompt.deviceId)"
                @click="cancelOutgoing(prompt.deviceId)"
              >
                取消
              </AppButton>
            </template>
          </DeviceCard>
        </div>
        <EmptyState
          v-else
          compact
          icon="send"
          title="没有进行中的请求"
          description="去设备页挑一台设备发起配对吧。"
        >
          <template #actions>
            <RouterLink to="/devices"><AppButton size="sm" icon="devices">去设备页</AppButton></RouterLink>
          </template>
        </EmptyState>
      </AppCard>
    </div>
  </div>
</template>

<style scoped>
.cols {
  display: grid;
  grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr);
  gap: var(--space-4);
  align-items: start;
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
  border-radius: var(--radius-sm);
  background: var(--surface);
  border: 1px solid var(--border-soft);
}

.verify-actions {
  display: flex;
  gap: 8px;
}

@media (max-width: 1080px) {
  .cols {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
