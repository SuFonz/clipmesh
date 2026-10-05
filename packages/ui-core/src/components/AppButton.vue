<script setup lang="ts">
import { computed } from "vue";

import AppIcon from "./AppIcon.vue";
import type { IconName } from "./icons";

type Variant = "primary" | "secondary" | "ghost" | "danger" | "success";
type Size = "sm" | "md" | "lg";

const props = withDefaults(
  defineProps<{
    variant?: Variant;
    size?: Size;
    /** 占满整行（移动端主按钮常用）。 */
    block?: boolean;
    loading?: boolean;
    disabled?: boolean;
    icon?: IconName;
    iconRight?: IconName;
    type?: "button" | "submit" | "reset";
    title?: string;
    /** 只显示图标，不显示文字（正方形按钮）。 */
    iconOnly?: boolean;
  }>(),
  {
    variant: "secondary",
    size: "md",
    block: false,
    loading: false,
    disabled: false,
    icon: undefined,
    iconRight: undefined,
    type: "button",
    title: undefined,
    iconOnly: false,
  },
);

const iconSize = computed<number>(() => (props.size === "sm" ? 15 : props.size === "lg" ? 20 : 17));
const isDisabled = computed<boolean>(() => props.disabled || props.loading);
</script>

<template>
  <button
    class="btn"
    :class="[`v-${variant}`, `s-${size}`, { block, 'icon-only': iconOnly, loading }]"
    :type="type"
    :disabled="isDisabled"
    :title="title"
    :aria-busy="loading ? 'true' : undefined"
  >
    <span v-if="loading" class="spinner" aria-hidden="true" />
    <AppIcon v-else-if="icon" :name="icon" :size="iconSize" />
    <span v-if="!iconOnly" class="label"><slot /></span>
    <AppIcon v-if="iconRight && !loading" class="right" :name="iconRight" :size="iconSize" />
  </button>
</template>

<style scoped>
.btn {
  --btn-h: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  height: var(--btn-h);
  padding: 0 14px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: var(--surface-2);
  color: var(--text);
  font-size: 13.5px;
  font-weight: 550;
  line-height: 1;
  white-space: nowrap;
  cursor: pointer;
  user-select: none;
  transition:
    background var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease),
    transform var(--dur-fast) var(--ease);
}

.btn:active:not(:disabled) {
  transform: translateY(1px);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn.block {
  display: flex;
  width: 100%;
}

.btn.icon-only {
  width: var(--btn-h);
  padding: 0;
}

/* 尺寸 */
.s-sm {
  --btn-h: 30px;
  padding: 0 10px;
  font-size: 12.5px;
}

.s-lg {
  --btn-h: 48px;
  padding: 0 20px;
  font-size: 15px;
  border-radius: var(--radius);
}

/* 变体 */
.v-primary {
  background: var(--accent);
  color: var(--accent-fg);
  box-shadow: var(--shadow-1);
}

.v-primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.v-primary:active:not(:disabled) {
  background: var(--accent-active);
}

.v-secondary {
  background: var(--surface-2);
  border-color: var(--border);
  color: var(--text);
}

.v-secondary:hover:not(:disabled) {
  background: var(--surface-3);
  border-color: var(--border-strong);
}

.v-ghost {
  background: transparent;
  color: var(--text-muted);
}

.v-ghost:hover:not(:disabled) {
  background: var(--surface-2);
  color: var(--text);
}

.v-danger {
  background: var(--danger-soft);
  border-color: transparent;
  color: var(--danger);
}

.v-danger:hover:not(:disabled) {
  background: var(--danger);
  color: #fff;
}

.v-success {
  background: var(--ok-soft);
  color: var(--ok);
}

.v-success:hover:not(:disabled) {
  background: var(--ok);
  color: var(--text-inverse);
}

.label {
  display: inline-block;
}

.right {
  opacity: 0.75;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  opacity: 0.85;
  animation: btn-spin 620ms linear infinite;
}

@keyframes btn-spin {
  to {
    transform: rotate(360deg);
  }
}

/* 触摸设备：主按钮至少 48px */
@media (pointer: coarse) {
  .s-md {
    --btn-h: 44px;
  }

  .s-sm {
    --btn-h: 38px;
  }
}
</style>
