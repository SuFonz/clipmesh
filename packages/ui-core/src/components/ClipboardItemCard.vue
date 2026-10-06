<script setup lang="ts">
import { computed, ref } from "vue";

import { useRelativeTime } from "../composables/useRelativeTime";
import { t } from "../i18n";
import type { ClipboardItemView } from "../types";
import { formatBytes } from "../utils/format";
import AppButton from "./AppButton.vue";
import AppIcon from "./AppIcon.vue";

/**
 * 一条剪贴板历史：文本显示预览（可展开），图片显示本地缩略图 + 尺寸/大小。
 * 操作按钮只 **emit 事件**，具体调用哪个命令由视图决定。
 *
 * 图片那一栏的动作可以由视图换掉：Android 上「复制」对一张图没什么用（系统剪贴板
 * 收得下图片，但用户想做的通常是发给别人），所以那边传 `image-action="share"`，
 * 按钮变成「分享」。默认仍是复制 —— 桌面端什么都不用传，行为和以前一模一样。
 */
const props = withDefaults(
  defineProps<{
    item: ClipboardItemView;
    /** 来源设备名（已翻译成人类可读的名字）。 */
    sourceName?: string;
    /** 图片缩略图 data URL；没加载好就显示占位。 */
    thumbnail?: string | null;
    /** 正在执行 copy / resend / share。 */
    busy?: boolean;
    /** 刚收到，闪一下。 */
    highlight?: boolean;
    /** 移动端精简版。 */
    compact?: boolean;
    /** 隐藏操作按钮（只读预览场景）。 */
    hideActions?: boolean;
    /**
     * 图片条目上的动作：`copy`（默认，两个平台原先的行为）或 `share`
     * （Android：交给系统分享面板）。文本条目永远是复制。
     */
    imageAction?: "copy" | "share";
  }>(),
  {
    sourceName: undefined,
    thumbnail: null,
    busy: false,
    highlight: false,
    compact: false,
    hideActions: false,
    imageAction: "copy",
  },
);

const emit = defineEmits<{
  (e: "copy", item: ClipboardItemView): void;
  (e: "resend", item: ClipboardItemView): void;
  (e: "share", item: ClipboardItemView): void;
}>();

const { format } = useRelativeTime();
const expanded = ref(false);

const isText = computed<boolean>(() => props.item.kind === "text");
/** 图片条目显示「分享」而不是「复制」。 */
const sharesImage = computed<boolean>(() => !isText.value && props.imageAction === "share");
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
  props.item.kind === "image"
    ? t("components.clipboardItem.imageAlt", {
        width: props.item.width,
        height: props.item.height,
      })
    : t("components.clipboardItem.imageAltPlain"),
);
const source = computed<string>(() => props.sourceName ?? t("common.unknownDevice"));
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
      <span v-if="highlight" class="new">{{ t("components.clipboardItem.new") }}</span>
    </header>

    <div class="body">
      <template v-if="isText">
        <p class="text cm-mono" :class="{ expanded, clamped: !expanded && needsExpand }">
          {{ textContent }}
        </p>
        <button v-if="needsExpand" class="more" type="button" @click="expanded = !expanded">
          <AppIcon :name="expanded ? 'chevron-down' : 'chevron-right'" :size="13" />
          {{
            expanded
              ? t("components.clipboardItem.collapse")
              : t("components.clipboardItem.expand", { count: textContent.length })
          }}
        </button>
      </template>

      <template v-else>
        <div class="thumb" :class="{ 'no-image': !thumbnail }">
          <img v-if="thumbnail" :src="thumbnail" :alt="imageAlt" />
          <span v-else class="thumb-placeholder">
            <AppIcon name="image" :size="20" />
            <span>{{ t("components.clipboardItem.thumbnailLoading") }}</span>
          </span>
        </div>
        <p class="image-meta cm-mono">{{ imageMeta }}</p>
      </template>
    </div>

    <footer v-if="!hideActions" class="foot">
      <AppButton
        v-if="sharesImage"
        size="sm"
        variant="ghost"
        icon="share"
        :disabled="busy"
        :title="t('components.clipboardItem.shareTitle')"
        @click="emit('share', item)"
      >
        {{ t("components.clipboardItem.share") }}
      </AppButton>
      <AppButton
        v-else
        size="sm"
        variant="ghost"
        icon="copy"
        :disabled="busy"
        :title="t('components.clipboardItem.copyTitle')"
        @click="emit('copy', item)"
      >
        {{ t("components.clipboardItem.copy") }}
      </AppButton>
      <AppButton
        size="sm"
        variant="ghost"
        icon="send"
        :disabled="busy"
        :loading="busy"
        :title="t('components.clipboardItem.resendTitle')"
        @click="emit('resend', item)"
      >
        {{ t("components.clipboardItem.resend") }}
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
