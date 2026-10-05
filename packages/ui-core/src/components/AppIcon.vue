<script setup lang="ts">
import { computed } from "vue";

import { ICONS, type IconName } from "./icons";

/**
 * 内联 SVG 图标。没有图标包，全部是 `icons.ts` 里的手写路径。
 * 颜色跟随 `currentColor`，尺寸默认跟随字号（1.15em）。
 */
const props = withDefaults(
  defineProps<{
    name: IconName;
    /** 像素尺寸；不传就跟随字号。 */
    size?: number | string;
    strokeWidth?: number;
  }>(),
  { size: undefined, strokeWidth: 1.7 },
);

const icon = computed(() => ICONS[props.name] ?? ICONS.unknown);

const dimension = computed<string>(() => {
  if (props.size === undefined) return "1.15em";
  return typeof props.size === "number" ? `${props.size}px` : props.size;
});
</script>

<template>
  <svg
    class="cm-icon"
    :class="{ 'is-filled': icon.filled }"
    viewBox="0 0 24 24"
    :width="dimension"
    :height="dimension"
    :fill="icon.filled ? 'currentColor' : 'none'"
    :stroke="icon.filled ? 'none' : 'currentColor'"
    :stroke-width="strokeWidth"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
    v-html="icon.body"
  />
</template>

<style scoped>
.cm-icon {
  display: block;
  flex: none;
  overflow: visible;
}
</style>
