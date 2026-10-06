<script setup lang="ts">
import AppIcon from "./AppIcon.vue";
import type { IconName } from "./icons";

withDefaults(
  defineProps<{
    title: string;
    description?: string;
    icon?: IconName;
    /** 紧凑模式：放在卡片里当占位时用。 */
    compact?: boolean;
  }>(),
  { description: undefined, icon: "inbox", compact: false },
);
</script>

<template>
  <div class="empty" :class="{ compact }">
    <span class="glyph"><AppIcon :name="icon" :size="compact ? 22 : 28" :stroke-width="1.5" /></span>
    <p class="title">{{ title }}</p>
    <p v-if="description" class="desc">{{ description }}</p>
    <div v-if="$slots.actions" class="actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 40px 20px;
  text-align: center;
}

.empty.compact {
  padding: 22px 14px;
}

.glyph {
  display: grid;
  place-items: center;
  width: 54px;
  height: 54px;
  margin-bottom: 4px;
  border-radius: 50%;
  background: var(--surface-2);
  border: 1px dashed var(--border-strong);
  color: var(--text-dim);
}

.compact .glyph {
  width: 40px;
  height: 40px;
}

.title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text);
}

.desc {
  max-width: 34ch;
  color: var(--text-muted);
  font-size: 12.5px;
  line-height: 1.5;
}

.actions {
  display: flex;
  gap: var(--space-2);
  margin-top: var(--space-3);
}
</style>
