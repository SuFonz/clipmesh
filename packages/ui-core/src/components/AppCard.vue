<script setup lang="ts">
import type { IconName } from "./icons";
import AppIcon from "./AppIcon.vue";

withDefaults(
  defineProps<{
    title?: string;
    subtitle?: string;
    icon?: IconName;
    /** 视觉语气：普通 / 强调 / 危险。 */
    tone?: "default" | "accent" | "danger" | "success";
    /** 去掉内边距（表格、列表类内容自己控制）。 */
    flush?: boolean;
    /** 悬停时抬一下（可点击的卡片）。 */
    interactive?: boolean;
    /** 紧凑模式：更小的内边距和标题字号。 */
    compact?: boolean;
  }>(),
  {
    title: undefined,
    subtitle: undefined,
    icon: undefined,
    tone: "default",
    flush: false,
    interactive: false,
    compact: false,
  },
);
</script>

<template>
  <section class="card" :class="[`tone-${tone}`, { flush, interactive, compact }]">
    <header v-if="title || $slots.actions || $slots.header" class="head">
      <div class="head-main">
        <span v-if="icon" class="head-icon"><AppIcon :name="icon" :size="18" /></span>
        <div class="head-text">
          <h2 v-if="title" class="title">{{ title }}</h2>
          <p v-if="subtitle" class="subtitle">{{ subtitle }}</p>
          <slot name="header" />
        </div>
      </div>
      <div v-if="$slots.actions" class="head-actions">
        <slot name="actions" />
      </div>
    </header>

    <div class="body" :class="{ padded: !flush }">
      <slot />
    </div>

    <footer v-if="$slots.footer" class="foot">
      <slot name="footer" />
    </footer>
  </section>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-1);
  transition:
    border-color var(--dur) var(--ease),
    transform var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}

.card.interactive:hover {
  border-color: var(--border-strong);
  box-shadow: var(--shadow-2);
  transform: translateY(-1px);
}

.tone-accent {
  border-color: var(--border-accent);
}

.tone-danger {
  border-color: var(--border-danger);
}

.tone-success {
  border-color: var(--border-ok);
}

.head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-3);
  padding: 14px 16px 0;
}

.compact .head {
  padding: 11px 13px 0;
}

.head-main {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  min-width: 0;
}

.head-icon {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  flex: none;
  border-radius: var(--radius-sm);
  background: var(--accent-soft);
  color: var(--accent);
}

.head-text {
  min-width: 0;
}

.title {
  font-size: 14.5px;
  font-weight: 620;
  letter-spacing: -0.01em;
  line-height: 1.3;
}

.compact .title {
  font-size: 13.5px;
}

.subtitle {
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 12.5px;
  line-height: 1.45;
}

.head-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: none;
}

.body {
  flex: 1;
  min-width: 0;
}

.body.padded {
  padding: 14px 16px;
}

.compact .body.padded {
  padding: 11px 13px;
}

.foot {
  padding: 12px 16px;
  border-top: 1px solid var(--border-soft);
  background: var(--surface-2);
  border-radius: 0 0 var(--radius-lg) var(--radius-lg);
}
</style>
