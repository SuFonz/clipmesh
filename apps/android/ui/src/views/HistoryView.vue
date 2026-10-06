<script setup lang="ts">
import { computed, ref } from "vue";

import {
  AppButton,
  AppCard,
  AppIcon,
  ClipboardItemCard,
  ConfirmDialog,
  EmptyState,
  StatusPill,
  summarizeItem,
  t,
  toMessage,
  useHistoryStore,
  usePeersStore,
  useToast,
  type ClipboardItemView,
  type HistoryFilter,
  type MessageKey,
} from "@clipmesh/ui-core";

/**
 * Android 历史页：单列、触摸优先。点卡片上的按钮复制或重发。
 */
const historyStore = useHistoryStore();
const peersStore = usePeersStore();
const toast = useToast();

const confirmClear = ref(false);
const clearing = ref(false);

/** 筛选按钮上存的是文案键 —— 数组在模块加载时建好，文字要等渲染时再取。 */
const FILTERS: Array<{ value: HistoryFilter; labelKey: MessageKey; count: () => number }> = [
  { value: "all", labelKey: "android.history.filter.all", count: () => historyStore.count },
  { value: "text", labelKey: "android.history.filter.text", count: () => historyStore.textCount },
  { value: "image", labelKey: "android.history.filter.image", count: () => historyStore.imageCount },
];

const list = computed(() => historyStore.filtered);
const isFiltering = computed(() => historyStore.query.trim() !== "" || historyStore.filter !== "all");

function clearFilters(): void {
  historyStore.query = "";
  historyStore.filter = "all";
}

async function onCopy(item: ClipboardItemView): Promise<void> {
  try {
    await historyStore.copy(item.id);
    toast.success(t("android.history.toast.copied"), summarizeItem(item, 40));
  } catch (cause) {
    toast.error(t("common.copyFailed"), toMessage(cause));
  }
}

async function onResend(item: ClipboardItemView): Promise<void> {
  try {
    const result = await historyStore.resend(item.id);
    toast.success(t("android.history.toast.resent", { count: result.delivered }));
  } catch (cause) {
    toast.error(t("android.history.toast.resendFailed"), toMessage(cause));
  }
}

async function doClear(): Promise<void> {
  clearing.value = true;
  try {
    await historyStore.clear();
    confirmClear.value = false;
    toast.success(t("android.history.toast.cleared"));
  } catch (cause) {
    toast.error(t("android.history.toast.clearFailed"), toMessage(cause));
  } finally {
    clearing.value = false;
  }
}
</script>

<template>
  <div class="view">
    <div class="bar">
      <div class="cm-search">
        <span class="cm-search-icon"><AppIcon name="search" :size="15" /></span>
        <input
          v-model="historyStore.query"
          class="cm-input"
          type="search"
          :placeholder="t('android.history.searchPlaceholder')"
        />
      </div>
      <AppButton
        icon="trash"
        variant="danger"
        icon-only
        :title="t('android.history.clearTitle')"
        :disabled="historyStore.count === 0"
        @click="confirmClear = true"
      />
    </div>

    <div class="chips">
      <button
        v-for="item in FILTERS"
        :key="item.value"
        class="chip"
        :class="{ active: historyStore.filter === item.value }"
        type="button"
        @click="historyStore.filter = item.value"
      >
        {{ t(item.labelKey) }}
        <span class="chip-count">{{ item.count() }}</span>
      </button>
    </div>

    <div v-if="list.length" class="cm-list">
      <ClipboardItemCard
        v-for="item in list"
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

    <AppCard v-else>
      <EmptyState
        v-if="isFiltering"
        compact
        icon="search"
        :title="t('android.history.emptyFiltered.title')"
        :description="t('android.history.emptyFiltered.description')"
      >
        <template #actions>
          <AppButton size="sm" @click="clearFilters">
            {{ t("android.history.clearFilters") }}
          </AppButton>
        </template>
      </EmptyState>
      <EmptyState
        v-else
        compact
        icon="clipboard"
        :title="t('android.history.empty.title')"
        :description="t('android.history.empty.description')"
      />
    </AppCard>

    <div class="tail">
      <StatusPill
        :label="t('android.history.count', { count: historyStore.count })"
        tone="idle"
        icon="clipboard"
        size="sm"
      />
    </div>

    <ConfirmDialog
      :open="confirmClear"
      tone="danger"
      :title="t('android.history.confirm.title')"
      :message="t('android.history.confirm.message', { count: historyStore.count })"
      :confirm-label="t('android.history.confirm.confirm')"
      :busy="clearing"
      @cancel="confirmClear = false"
      @confirm="doClear"
    />
  </div>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.bar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.chips {
  display: flex;
  gap: 8px;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 38px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-full);
  background: var(--surface);
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}

.chip.active {
  background: var(--accent-soft);
  border-color: var(--border-accent);
  color: var(--accent);
}

.chip-count {
  color: var(--text-dim);
  font-size: 11.5px;
  font-variant-numeric: tabular-nums;
}

.chip.active .chip-count {
  color: var(--accent);
}

.tail {
  display: flex;
  justify-content: center;
  padding: 4px 0 8px;
}
</style>
