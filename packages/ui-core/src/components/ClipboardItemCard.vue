<script setup lang="ts">
import { computed, ref } from "vue";

import { useRelativeTime } from "../composables/useRelativeTime";
import type { ClipboardItemView } from "../types";
import { formatBytes } from "../utils/format";
import AppButton from "./AppButton.vue";
import AppIcon from "./AppIcon.vue";

/**
 * 一条剪贴板历史：文本显示预览（可展开），图片显示本地缩略图 + 尺寸/大小。
 * 操作按钮只 **emit 事件**，具体调用哪个命令由视图决定。
 */
const props = withDefaults(
  defineProps<{
    item: ClipboardItemView;
    /** 来源设备名（已翻译成人类可读的名字）。 */
    sourceName?: string;
    /** 图片缩略图 data URL；没加载好就显示占位。 */
    thumbnail?: string | null;
    /** 正在执行 copy / resend。 */
    busy?: boolean;
    /** 刚收到，闪一下。 */
    highlight?: boolean;
    /** 移动端精简版。 */
    compact?: boolean;
    /** 隐藏操作按钮（只读预览场景）。 */
    hideActions?: boolean;
  }>(),
  {
    sourceName: undefined,
    thumbnail: null,
    busy: false,
    highlight: false,
    compact: false,
    hideActions: false,
  },
);

const emit = defineEmits<{
  (e: "copy", item: ClipboardItemView): void;
  (e: "resend", item: ClipboardItemView): void;
}>();

const { format } = useRelativeTime();
const expanded = ref(false);

const isText = computed<boolean>(() => props.item.kind === "text");
const textContent = computed<string>(() => (props.item.kind === "text" ? props.item.content : ""));
const needsExpand = computed<boolean>(
  () => textContent.value.length > 220 || textContent.value.split("\n").length > 3,
);
const imageMeta = computed<string>(() =>
  props.item.kind === "image"
    ? `${props.item.width}×${props.item.height} · ${formatBytes(props.item.size)} · ${props.item.mime}`
    : "",
);
const imageAlt = computed<string>(() =>
  props.item.kind === "image" ? `剪贴板图片 ${props.item.width}×${props.item.height}` : "剪贴板图片",
);
const source = computed<string>(() => props.sourceName ?? "未知设备");
const time = computed<string>(() => format(props.item.timestamp));
</script>

<template>
  <article class="item" :class="{ compact, highlight, busy }">
    <header class="head">
      <span class="kind" :class="isText ? 'kind-text' : 'kind-image'">
        <AppIcon :name="isText ? 'text' : 'image'" :size="compact ? 15 : 16" />
      </span>
      <div class="meta">
        <span class="source cm-truncate">{{ source }}</span>
        <span class="dot">·</span>
        <time class="time" :datetime="new Date(item.timestamp).toISOString()">{{ time }}</time>
      </div>
      <span v-if="highlight" class="new">新</span>
    </header>

    <div class="body">
      <template v-if="isText">
        <p class="text cm-mono" :class="{ expanded, clamped: !expanded && needsExpand }">
          {{ textContent }}
        </p>
        <button v-if="needsExpand" class="more" type="button" @click="expanded = !expanded">
          <AppIcon :name="expanded ? 'chevron-down' : 'chevron-right'" :size="13" />
          {{ expanded ? "收起" : `展开全部（${textContent.length} 字符）` }}
        </button>
      </template>

      <template v-else>
        <div class="thumb" :class="{ 'no-image': !thumbnail }">
          <img v-if="thumbnail" :src="thumbnail" :alt="imageAlt" />
          <span v-else class="thumb-placeholder">
            <AppIcon name="image" :size="20" />
            <span>缩略图加载中…</span>
          </span>
        </div>
        <p class="image-meta cm-mono">{{ imageMeta }}</p>
      </template>
    </div>

    <footer v-if="!hideActions" class="foot">
      <AppButton
        size="sm"
        variant="ghost"
        icon="copy"
        :disabled="busy"
        title="复制到本机剪贴板（不发送）"
        @click="emit('copy', item)"
      >
        复制
      </AppButton>
      <AppButton
        size="sm"
        variant="ghost"
        icon="send"
        :disabled="busy"
        :loading="busy"
        title="重新发送到所有在线设备"
        @click="emit('resend', item)"
      >
        重发
      </AppButton>
    </footer>
  </article>
</template>

<style scoped>
.item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}

.item:hover {
  border-color: var(--border-strong);
}

.item.compact {
  padding: 10px;
}

.item.highlight {
  border-color: var(--border-accent);
  background: linear-gradient(180deg, var(--accent-soft), transparent 70%), var(--surface);
}

.item.busy {
  opacity: 0.75;
}

.head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.kind {
  display: grid;
  place-items: center;
  flex: none;
  width: 26px;
  height: 26px;
  border-radius: var(--radius-xs);
  background: var(--surface-2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.kind-image {
  background: var(--info-soft);
  border-color: transparent;
  color: var(--info);
}

.kind-text {
  background: var(--accent-soft);
  border-color: transparent;
  color: var(--accent);
}

.meta {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.source {
  font-weight: 600;
  color: var(--text);
  max-width: 14ch;
}

.dot {
  color: var(--text-dim);
}

.new {
  margin-left: auto;
  padding: 1px 6px;
  border-radius: var(--radius-full);
  background: var(--accent);
  color: var(--accent-fg);
  font-size: 10.5px;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.body {
  min-width: 0;
}

.text {
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-soft);
  border: 1px solid var(--border-soft);
  color: var(--text);
  font-size: 12.5px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.text.clamped {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.more {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 6px;
  padding: 0;
  border: none;
  background: none;
  color: var(--accent);
  font-size: 12px;
  cursor: pointer;
}

.more:hover {
  text-decoration: underline;
}

.thumb {
  display: grid;
  place-items: center;
  overflow: hidden;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-soft);
  background: var(--bg-soft);
  min-height: 96px;
}

.thumb img {
  width: 100%;
  max-height: 190px;
  object-fit: cover;
}

.thumb.no-image {
  border-style: dashed;
  border-color: var(--border-strong);
}

.thumb-placeholder {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 22px 0;
  color: var(--text-dim);
  font-size: 12px;
}

.image-meta {
  margin-top: 6px;
  color: var(--text-dim);
  font-size: 11.5px;
}

.foot {
  display: flex;
  gap: 4px;
  margin: 0 -4px -4px;
}

@media (pointer: coarse) {
  .foot :deep(.btn) {
    flex: 1;
  }
}
</style>
