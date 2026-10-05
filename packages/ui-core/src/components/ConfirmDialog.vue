<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";

import AppButton from "./AppButton.vue";
import AppIcon from "./AppIcon.vue";

/**
 * 确认对话框。用在「解除信任」「清空历史」这类不可逆操作上。
 * 自带 Esc 关闭、点遮罩关闭、打开时把焦点移到确认按钮。
 */
const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    message?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    tone?: "default" | "danger";
    busy?: boolean;
  }>(),
  {
    message: undefined,
    confirmLabel: "确认",
    cancelLabel: "取消",
    tone: "default",
    busy: false,
  },
);

const emit = defineEmits<{
  (e: "confirm"): void;
  (e: "cancel"): void;
}>();

const panel = ref<HTMLElement | null>(null);

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && !props.busy) {
    event.stopPropagation();
    emit("cancel");
  }
}

watch(
  () => props.open,
  async (open) => {
    if (!open) {
      document.removeEventListener("keydown", onKeydown, true);
      return;
    }
    document.addEventListener("keydown", onKeydown, true);
    await nextTick();
    panel.value?.focus();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKeydown, true);
});
</script>

<template>
  <Teleport to="body">
    <Transition name="dlg">
      <div v-if="open" class="backdrop" @click.self="!busy && emit('cancel')">
        <div
          ref="panel"
          class="panel"
          role="alertdialog"
          aria-modal="true"
          :aria-label="title"
          tabindex="-1"
        >
          <div class="top">
            <span class="glyph" :class="`g-${tone}`">
              <AppIcon :name="tone === 'danger' ? 'alert' : 'info'" :size="18" />
            </span>
            <div class="text">
              <h2 class="title">{{ title }}</h2>
              <p v-if="message" class="message">{{ message }}</p>
              <slot />
            </div>
          </div>

          <div class="actions">
            <AppButton variant="ghost" :disabled="busy" @click="emit('cancel')">
              {{ cancelLabel }}
            </AppButton>
            <AppButton
              :variant="tone === 'danger' ? 'danger' : 'primary'"
              :loading="busy"
              @click="emit('confirm')"
            >
              {{ confirmLabel }}
            </AppButton>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  display: grid;
  place-items: center;
  padding: 20px;
  background: rgba(4, 6, 10, 0.62);
  backdrop-filter: blur(2px);
}

.panel {
  width: min(440px, 100%);
  padding: 18px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--surface);
  box-shadow: var(--shadow-3);
}

.panel:focus {
  outline: none;
}

.top {
  display: flex;
  gap: 12px;
}

.glyph {
  display: grid;
  place-items: center;
  flex: none;
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent);
}

.g-danger {
  background: var(--danger-soft);
  color: var(--danger);
}

.text {
  min-width: 0;
}

.title {
  font-size: 15px;
  font-weight: 640;
}

.message {
  margin-top: 5px;
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1.55;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
}

@media (max-width: 480px) {
  .actions {
    flex-direction: column-reverse;
  }

  .actions :deep(.btn) {
    width: 100%;
  }
}

.dlg-enter-active,
.dlg-leave-active {
  transition: opacity var(--dur) var(--ease);
}

.dlg-enter-active .panel,
.dlg-leave-active .panel {
  transition: transform var(--dur) var(--ease);
}

.dlg-enter-from,
.dlg-leave-to {
  opacity: 0;
}

.dlg-enter-from .panel,
.dlg-leave-to .panel {
  transform: translateY(10px) scale(0.98);
}
</style>
