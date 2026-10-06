<script setup lang="ts">
import { t } from "../i18n";
import { useToast, type ToastTone } from "../composables/useToast";
import AppIcon from "./AppIcon.vue";
import type { IconName } from "./icons";

/**
 * toast 容器。挂在 `App.vue` 里一次即可（组件内部 Teleport 到 body）。
 * 桌面放右下角，移动端放底部标签栏上方。
 */
withDefaults(
  defineProps<{
    placement?: "bottom-right" | "bottom-center";
  }>(),
  { placement: "bottom-right" },
);

const { toasts, dismiss } = useToast();

const TONE_ICONS: Record<ToastTone, IconName> = {
  info: "info",
  success: "check",
  warn: "alert",
  error: "alert",
};
</script>

<template>
  <Teleport to="body">
    <div class="toast-host" :class="`p-${placement}`" role="status" aria-live="polite">
      <TransitionGroup name="toast">
        <div v-for="toast in toasts" :key="toast.id" class="toast" :class="`t-${toast.tone}`">
          <span class="glyph"><AppIcon :name="TONE_ICONS[toast.tone]" :size="16" /></span>
          <div class="text">
            <p class="title">{{ toast.title }}</p>
            <p v-if="toast.description" class="desc">{{ toast.description }}</p>
          </div>
          <button
            class="close"
            type="button"
            :aria-label="t('components.toast.close')"
            @click="dismiss(toast.id)"
          >
            <AppIcon name="close" :size="14" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-host {
  position: fixed;
  z-index: 200;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
  max-width: min(380px, calc(100vw - 32px));
}

.p-bottom-right {
  right: 18px;
  bottom: 18px;
  align-items: flex-end;
}

.p-bottom-center {
  left: 50%;
  transform: translateX(-50%);
  /* 移动端：抬到底部标签栏 + 安全区之上 */
  bottom: calc(var(--tabbar-h) + env(safe-area-inset-bottom, 0px) + 12px);
  align-items: stretch;
  width: calc(100vw - 24px);
  max-width: 460px;
}

.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  width: 100%;
  padding: 11px 12px;
  border: 1px solid var(--border);
  border-left-width: 3px;
  border-radius: var(--radius);
  background: var(--surface);
  box-shadow: var(--shadow-2);
  pointer-events: auto;
}

.glyph {
  display: grid;
  place-items: center;
  flex: none;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--surface-2);
}

.text {
  flex: 1;
  min-width: 0;
}

.title {
  font-size: 13px;
  font-weight: 600;
}

.desc {
  margin-top: 1px;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.45;
  overflow-wrap: anywhere;
}

.close {
  flex: none;
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: var(--radius-xs);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

.close:hover {
  background: var(--surface-2);
  color: var(--text);
}

.t-success {
  border-left-color: var(--ok);
}

.t-success .glyph {
  background: var(--ok-soft);
  color: var(--ok);
}

.t-error {
  border-left-color: var(--danger);
}

.t-error .glyph {
  background: var(--danger-soft);
  color: var(--danger);
}

.t-warn {
  border-left-color: var(--warn);
}

.t-warn .glyph {
  background: var(--warn-soft);
  color: var(--warn);
}

.t-info {
  border-left-color: var(--accent);
}

.t-info .glyph {
  background: var(--accent-soft);
  color: var(--accent);
}

.toast-enter-active,
.toast-leave-active {
  transition:
    opacity var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.98);
}
</style>
