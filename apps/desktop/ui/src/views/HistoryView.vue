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
 * 历史页：本机最近 50 条剪贴板。可以复制回剪贴板（不发送）或重新发送。
 */
const historyStore = useHistoryStore();
const peersStore = usePeersStore();
const toast = useToast();

const clearing = ref(false);
const confirmClear = ref(false);

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
    toast.success("已复制到本机剪贴板", summarizeItem(item, 48));
  } catch (cause) {
    toast.error("复制失败", toMessage(cause));
  }
}

async function onResend(item: ClipboardItemView): Promise<void> {
  try {
    const result = await historyStore.resend(item.id);
    if (result.delivered > 0) {
      toast.success(`已重新发送到 ${result.delivered} 台设备`);
    } else {
      toast.warn("没有在线设备", "内容仍在历史里，等设备上线再试。");
    }
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

async function refresh(): Promise<void> {
  try {
    await historyStore.refresh();
  } catch (cause) {
    toast.error("刷新失败", toMessage(cause));
  }
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">历史</h1>
        <p class="cm-page-sub">
          最多保留 50 条，最新在前。图片只保存元数据，缩略图是在本机现取的。
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill :label="`共 ${historyStore.count} 条`" tone="idle" icon="clipboard" />
        <AppButton icon="refresh" :loading="historyStore.loading" @click="refresh">刷新</AppButton>
        <AppButton
          variant="danger"
          icon="trash"
          :disabled="historyStore.count === 0"
          @click="confirmClear = true"
        >
          清除剪贴板
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
            placeholder="搜索文本内容或图片尺寸"
          />
        </div>

        <div class="segmented" role="tablist" aria-label="内容类型">
          <button
            v-for="item in filters"
            :key="item.value"
            class="seg"
            :class="{ active: historyStore.filter === item.value }"
            type="button"
            role="tab"
            :aria-selected="historyStore.filter === item.value"
            @click="historyStore.filter = item.value"
          >
            {{ item.label }}
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
          title="没有匹配的条目"
          description="换个关键词，或者把筛选切回「全部」。"
        >
          <template #actions>
            <AppButton size="sm" @click="clearFilters">清除筛选</AppButton>
          </template>
        </EmptyState>

        <EmptyState
          v-else
          icon="clipboard"
          title="还没有任何历史"
          description="开启自动同步，其他设备同步过来的内容会自动出现在这里。"
        />
      </div>
    </AppCard>

    <ConfirmDialog
      :open="confirmClear"
      tone="danger"
      title="清除剪贴板历史？"
      :message="`将删除本机保存的 ${historyStore.count} 条记录（包含图片元数据），此操作不可恢复。已经同步到其他设备的内容不受影响。`"
      confirm-label="清除"
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
