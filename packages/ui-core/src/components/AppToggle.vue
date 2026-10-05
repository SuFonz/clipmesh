<script setup lang="ts">
import { computed, useId } from "vue";

/**
 * 开关。视觉上是自绘的 track + thumb，语义上是一个真正的
 * `<input type="checkbox" role="switch">`，所以键盘、读屏都能用。
 */
const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    label?: string;
    description?: string;
    disabled?: boolean;
    /** 关掉时右侧的说明文字。 */
    hint?: string;
  }>(),
  { label: undefined, description: undefined, disabled: false, hint: undefined },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: boolean): void;
}>();

const fieldId = useId();
const descriptionId = computed<string>(() => `${fieldId}-desc`);

function onChange(event: Event): void {
  const target = event.target as HTMLInputElement;
  emit("update:modelValue", target.checked);
}
</script>

<template>
  <label class="toggle" :class="{ disabled }" :for="fieldId">
    <span class="text">
      <span v-if="label" class="label">{{ label }}</span>
      <span v-if="description" :id="descriptionId" class="desc">{{ description }}</span>
    </span>

    <span v-if="hint" class="hint">{{ hint }}</span>

    <input
      :id="fieldId"
      class="input"
      type="checkbox"
      role="switch"
      :checked="modelValue"
      :disabled="disabled"
      :aria-describedby="description ? descriptionId : undefined"
      @change="onChange"
    />
    <span class="track" aria-hidden="true"><span class="thumb" /></span>
  </label>
</template>

<style scoped>
.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-height: 40px;
  cursor: pointer;
  user-select: none;
}

.toggle.disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.text {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  flex: 1;
}

.label {
  font-size: 13.5px;
  font-weight: 550;
}

.desc {
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.45;
}

.hint {
  color: var(--text-dim);
  font-size: 12px;
  flex: none;
}

/* 真的 input 藏在视觉层下面，但保留可聚焦性 */
.input {
  position: absolute;
  width: 44px;
  height: 24px;
  margin: 0;
  opacity: 0;
  cursor: inherit;
}

.track {
  position: relative;
  flex: none;
  width: 44px;
  height: 24px;
  border-radius: var(--radius-full);
  background: var(--surface-3);
  border: 1px solid var(--border);
  transition:
    background var(--dur) var(--ease),
    border-color var(--dur) var(--ease);
}

.thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--text-muted);
  box-shadow: var(--shadow-1);
  transition:
    transform var(--dur) var(--ease),
    background var(--dur) var(--ease);
}

.input:checked ~ .track {
  background: var(--accent);
  border-color: var(--accent);
}

.input:checked ~ .track .thumb {
  transform: translateX(20px);
  background: #fff;
}

.input:focus-visible ~ .track {
  box-shadow: var(--focus-ring);
}

.input:disabled ~ .track {
  cursor: not-allowed;
}

/* 触摸设备：整行至少 48px */
@media (pointer: coarse) {
  .toggle {
    min-height: var(--tap-min);
  }
}
</style>
