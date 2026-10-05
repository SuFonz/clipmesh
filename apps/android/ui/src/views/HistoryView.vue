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
  toMessage,
  useHistoryStore,
  usePeersStore,
  useToast,
  type ClipboardItemView,
  type HistoryFilter,
} from "@clipmesh/ui-core";

/**
 * Android 历史页：单列、触摸优先。点卡片上的按钮复制或重发。
 */
const historyStore = useHistoryStore();
const peersStore = usePeersStore();
const toast = useToast();

const confirmClear = ref(false);
const clearing = ref(false);

const filters: Array<{ value: HistoryFilter; label: string; count: () => number }> = [
  { value: "all", label: "全部", count: () => historyStore.count },
  { value: "text", label: "文本", count: () => historyStore.textCount },
  { value: "image", label: "图片", count: () => historyStore.imageCount },
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
    toast.success("已复制到剪贴板", summarizeItem(item, 40));
  } catch (cause) {
    toast.error("复制失败", toMessage(cause));
  }
}

async function onResend(item: ClipboardItemView): Promise<void> {
  try {
    const result = await historyStore.resend(item.id);
    toast.success(`已重发（${result.delivered} 台设备）`);
  } catch (cause) {
    toast.error("重发失败", toMessage(cause));
  }
}

async function doClear(): Promise<void> {
  clearing.value = true;
  try {
    await historyStore.clear();
    confirmClear.value = false;
    toast.success("历史已清空");
  } catch (cause) {
    toast.error("清空失败", toMessage(cause));
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
        <input v-model="historyStore.query" class="cm-input" type="search" placeholder="搜索历史" />
      </div>
      <AppButton
        icon="trash"
        variant="danger"
        icon-only
        title="清空历史"
        :disabled="historyStore.count === 0"
        @click="confirmClear = true"
      />
    </div>

    <div class="chips">
      <button
        v-for="item in filters"
        :key="item.value"
        class="chip"
        :class="{ active: historyStore.filter === item.value }"
        type="button"
        @click="historyStore.filter = item.value"
      >
        {{ item.label }}
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
        title="没有匹配的条目"
        description="换个关键词试试。"
      >
        <template #actions>
          <AppButton size="sm" @click="clearFilters">清除筛选</AppButton>
        </template>
      </EmptyState>
      <EmptyState
        v-else
        compact
        icon="clipboard"
        title="还没有历史"
        description="收到的剪贴板会自动出现在这里。"
      />
    </AppCard>

    <div class="tail">
      <StatusPill :label="`共 ${historyStore.count} 条`" tone="idle" icon="clipboard" size="sm" />
    </div>

    <ConfirmDialog
      :open="confirmClear"
      tone="danger"
      title="清空全部历史？"
      :message="`将删除本机保存的 ${historyStore.count} 条记录，无法恢复。`"
      confirm-label="清空"
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
