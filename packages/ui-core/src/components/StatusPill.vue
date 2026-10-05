<script setup lang="ts">
import { computed } from "vue";

import AppIcon from "./AppIcon.vue";
import type { IconName } from "./icons";

type Tone = "ok" | "warn" | "error" | "info" | "idle" | "accent";

/**
 * 状态胶囊：一个圆点 + 一句话。用于在线/离线、引擎运行中、等待配对等。
 */
const props = withDefaults(
  defineProps<{
    label: string;
    tone?: Tone;
    icon?: IconName;
    /** 是否显示左侧圆点（有 icon 时自动隐藏）。 */
    dot?: boolean;
    /** 呼吸动画（表示"正在进行"）。 */
    pulse?: boolean;
    size?: "sm" | "md";
  }>(),
  { tone: "idle", icon: undefined, dot: true, pulse: false, size: "md" },
);

const showDot = computed<boolean>(() => props.dot && props.icon === undefined);
</script>

<template>
  <span class="pill" :class="[`t-${tone}`, `s-${size}`]">
    <span v-if="showDot" class="dot" :class="{ pulse }" aria-hidden="true" />
    <AppIcon v-else-if="icon" :name="icon" :size="size === 'sm' ? 12 : 13" />
    <span class="label">{{ label }}</span>
  </span>
</template>

<style scoped>
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 9px;
  border: 1px solid transparent;
  border-radius: var(--radius-full);
  font-size: 12px;
  font-weight: 550;
  line-height: 1.5;
  white-space: nowrap;
}

.s-sm {
  padding: 1px 7px;
  font-size: 11px;
  gap: 5px;
}

.dot {
  width: 7px;
  height: 7px;
  flex: none;
  border-radius: 50%;
  background: currentColor;
}

.dot.pulse {
  animation: pill-pulse 1.6s var(--ease) infinite;
}

@keyframes pill-pulse {
  0%,
  100% {
    opacity: 1;
    box-shadow: 0 0 0 0 currentColor;
  }
  60% {
    opacity: 0.75;
    box-shadow: 0 0 0 4px transparent;
  }
}

.t-ok {
  background: var(--ok-soft);
  color: var(--ok);
  border-color: var(--border-ok);
}

.t-warn {
  background: var(--warn-soft);
  color: var(--warn);
}

.t-error {
  background: var(--danger-soft);
  color: var(--danger);
  border-color: var(--border-danger);
}

.t-info {
  background: var(--info-soft);
  color: var(--info);
}

.t-accent {
  background: var(--accent-soft);
  color: var(--accent);
  border-color: var(--border-accent);
}

.t-idle {
  background: var(--surface-2);
  color: var(--text-muted);
  border-color: var(--border);
}
</style>
