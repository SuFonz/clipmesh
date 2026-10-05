import { defineStore } from "pinia";
import { computed, ref } from "vue";

import {
  clearHistory,
  copyHistoryItem,
  getHistory,
  getImageThumbnail,
  resendHistoryItem,
} from "../api/commands";
import type { ClipboardItemView, SendResult } from "../types";
import { toMessage } from "./status";

export type HistoryFilter = "all" | "text" | "image";

const HISTORY_LIMIT = 50;

/**
 * 剪贴板历史。`clipmesh://history` 是快照事件（最多 50 条，最新在前）。
 * 图片只有元数据，像素要单独用 `get_image_thumbnail` 取本地缩略图。
 */
export const useHistoryStore = defineStore("history", () => {
  const items = ref<ClipboardItemView[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const query = ref("");
  const filter = ref<HistoryFilter>("all");
  const busyIds = ref<string[]>([]);
  /** 图片 id -> data URL 缩略图。 */
  const thumbnails = ref<Record<string, string>>({});

  const inflight = new Set<string>();

  const filtered = computed<ClipboardItemView[]>(() => {
    const keyword = query.value.trim().toLowerCase();
    return items.value.filter((item) => {
      if (filter.value !== "all" && item.kind !== filter.value) return false;
      if (keyword === "") return true;
      if (item.kind === "text") return item.content.toLowerCase().includes(keyword);
      return `${item.width}x${item.height} ${item.mime}`.toLowerCase().includes(keyword);
    });
  });

  const count = computed<number>(() => items.value.length);
  const textCount = computed<number>(() => items.value.filter((i) => i.kind === "text").length);
  const imageCount = computed<number>(() => items.value.filter((i) => i.kind === "image").length);
  const latest = computed<ClipboardItemView | null>(() => items.value[0] ?? null);

  function isBusy(id: string): boolean {
    return busyIds.value.includes(id);
  }

  function markBusy(id: string, busy: boolean): void {
    busyIds.value = busy
      ? [...busyIds.value.filter((x) => x !== id), id]
      : busyIds.value.filter((x) => x !== id);
  }

  /** 拉一张本地缩略图（幂等，失败静默 —— 列表不该因为缩略图挂掉）。 */
  async function ensureThumbnail(item: ClipboardItemView, maxSize = 360): Promise<void> {
    if (item.kind !== "image") return;
    if (thumbnails.value[item.id] !== undefined || inflight.has(item.id)) return;
    inflight.add(item.id);
    try {
      const url = await getImageThumbnail(item.id, maxSize);
      if (typeof url === "string" && url !== "") {
        thumbnails.value = { ...thumbnails.value, [item.id]: url };
      }
    } catch {
      /* 缩略图是可选的 */
    } finally {
      inflight.delete(item.id);
    }
  }

  function prefetchThumbnails(): void {
    for (const item of items.value) {
      if (item.kind === "image") void ensureThumbnail(item);
    }
  }

  function setItems(next: ClipboardItemView[]): void {
    items.value = next.slice(0, HISTORY_LIMIT);
    prefetchThumbnails();
  }

  /** 乐观插入（用于 `clipboard-received` / `clipboard-sent` 事件），按 id 去重。 */
  function prepend(item: ClipboardItemView): void {
    items.value = [item, ...items.value.filter((i) => i.id !== item.id)].slice(0, HISTORY_LIMIT);
    void ensureThumbnail(item);
  }

  function byId(id: string): ClipboardItemView | undefined {
    return items.value.find((i) => i.id === id);
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setItems(await getHistory());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  /** 重新发送一条历史（会被计一次投递）。 */
  async function resend(id: string): Promise<SendResult> {
    markBusy(id, true);
    try {
      const result = await resendHistoryItem(id);
      error.value = null;
      return result;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      markBusy(id, false);
    }
  }

  /** 只写本机剪贴板，不发送。 */
  async function copy(id: string): Promise<void> {
    markBusy(id, true);
    try {
      await copyHistoryItem(id);
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      markBusy(id, false);
    }
  }

  async function clear(): Promise<void> {
    try {
      await clearHistory();
      items.value = [];
      thumbnails.value = {};
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    }
  }

  return {
    items,
    loading,
    error,
    query,
    filter,
    busyIds,
    thumbnails,
    filtered,
    count,
    textCount,
    imageCount,
    latest,
    isBusy,
    byId,
    setItems,
    prepend,
    ensureThumbnail,
    refresh,
    resend,
    copy,
    clear,
  };
});
