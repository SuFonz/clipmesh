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
 * 历史页：本机最近 50 条剪贴板。可以复制回剪贴板（不发送）或重新发送。
 */
const historyStore = useHistoryStore();
const peersStore = usePeersStore();
const toast = useToast();

const clearing = ref(false);
const confirmClear = ref(false);

/** 筛选按钮上存的是文案键 —— 数组在模块加载时建好，文字要等渲染时再取。 */
const FILTERS: Array<{ value: HistoryFilter; labelKey: MessageKey; count: () => number }> = [
  { value: "all", labelKey: "desktop.history.filter.all", count: () => historyStore.count },
  { value: "text", labelKey: "desktop.history.filter.text", count: () => historyStore.textCount },
  { value: "image", labelKey: "desktop.history.filter.image", count: () => historyStore.imageCount },
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
    toast.success(t("desktop.history.toast.copied"), summarizeItem(item, 48));
  } catch (cause) {
    toast.error(t("common.copyFailed"), toMessage(cause));
  }
}

async function onResend(item: ClipboardItemView): Promise<void> {
  try {
    const result = await historyStore.resend(item.id);
    if (result.delivered > 0) {
      toast.success(t("desktop.history.toast.resent", { count: result.delivered }));
    } else {
      toast.warn(
        t("common.noOnlineDevices"),
        t("desktop.history.toast.noOnlineDevicesDescription"),
      );
    }
  } catch (cause) {
    toast.error(t("desktop.history.toast.resendFailed"), toMessage(cause));
  }
}

async function doClear(): Promise<void> {
  clearing.value = true;
  try {
    await historyStore.clear();
    confirmClear.value = false;
    toast.success(t("desktop.history.toast.cleared"));
  } catch (cause) {
    toast.error(t("desktop.history.toast.clearFailed"), toMessage(cause));
  } finally {
    clearing.value = false;
  }
}

async function refresh(): Promise<void> {
  try {
    await historyStore.refresh();
  } catch (cause) {
    toast.error(t("common.refreshFailed"), toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">{{ t("desktop.nav.history") }}</h1>
        <p class="cm-page-sub">
          {{ t("desktop.history.subtitle") }}
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill
          :label="t('desktop.history.count', { count: historyStore.count })"
          tone="idle"
          icon="clipboard"
        />
        <AppButton icon="refresh" :loading="historyStore.loading" @click="refresh">
          {{ t("common.refresh") }}
        </AppButton>
        <AppButton
          variant="danger"
          icon="trash"
          :disabled="historyStore.count === 0"
          @click="confirmClear = true"
        >
          {{ t("desktop.history.clear") }}
        </AppButton>
      </div>
    </header>

    <AppCard flush>
      <div class="toolbar">
        <div class="cm-search">
          <span class="cm-search-icon"><AppIcon name="search" :size="15" /></span>
          <input
            v-model="historyStore.query"
            class="cm-input"
            type="search"
            :placeholder="t('desktop.history.searchPlaceholder')"
          />
        </div>

        <div class="segmented" role="tablist" :aria-label="t('desktop.history.filterAria')">
          <button
            v-for="item in FILTERS"
            :key="item.value"
            class="seg"
            :class="{ active: historyStore.filter === item.value }"
            type="button"
            role="tab"
            :aria-selected="historyStore.filter === item.value"
            @click="historyStore.filter = item.value"
          >
            {{ t(item.labelKey) }}
            <span class="seg-count">{{ item.count() }}</span>
          </button>
        </div>
      </div>

      <div class="list-wrap">
        <div v-if="list.length" class="cm-list">
          <ClipboardItemCard
            v-for="item in list"
            :key="item.id"
            :item="item"
            :source-name="peersStore.nameOf(item.sourceDevice)"
            :thumbnail="historyStore.thumbnails[item.id] ?? null"
            :busy="historyStore.isBusy(item.id)"
            @copy="onCopy"
            @resend="onResend"
          />
        </div>

        <EmptyState
          v-else-if="isFiltering"
          icon="search"
          :title="t('desktop.history.emptyFiltered.title')"
          :description="t('desktop.history.emptyFiltered.description')"
        >
          <template #actions>
            <AppButton size="sm" @click="clearFilters">
              {{ t("desktop.history.clearFilters") }}
            </AppButton>
          </template>
        </EmptyState>

        <EmptyState
          v-else
          icon="clipboard"
          :title="t('desktop.history.empty.title')"
          :description="t('desktop.history.empty.description')"
        />
      </div>
    </AppCard>

    <ConfirmDialog
      :open="confirmClear"
      tone="danger"
      :title="t('desktop.history.confirm.title')"
      :message="t('desktop.history.confirm.message', { count: historyStore.count })"
      :confirm-label="t('desktop.history.confirm.confirm')"
      :busy="clearing"
      @cancel="confirmClear = false"
      @confirm="doClear"
    />
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-soft);
}

.segmented {
  display: inline-flex;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.seg {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 11px;
  border: none;
  border-radius: var(--radius-xs);
  background: transparent;
  color: var(--text-muted);
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
  transition:
    background var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.seg:hover {
  color: var(--text);
}

.seg.active {
  background: var(--surface);
  color: var(--text);
  box-shadow: var(--shadow-1);
}

.seg-count {
  color: var(--text-dim);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.list-wrap {
  padding: 14px 16px 16px;
}
</style>
