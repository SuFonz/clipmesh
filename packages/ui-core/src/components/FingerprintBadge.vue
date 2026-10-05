<script setup lang="ts">
import { computed, ref } from "vue";

import { useToast } from "../composables/useToast";
import { copyToClipboard } from "../utils/dom";
import { groupFingerprint } from "../utils/format";
import AppIcon from "./AppIcon.vue";

/**
 * 指纹展示：等宽字体 + 每 4 位分组，右边一个复制按钮。
 * 配对场景里用户需要**肉眼核对**这串字符，所以要够大够清楚。
 */
const props = withDefaults(
  defineProps<{
    fingerprint: string;
    label?: string;
    /** 只显示前面的组（列表里用）。 */
    groups?: number;
    copyable?: boolean;
    size?: "sm" | "md" | "lg";
  }>(),
  { label: undefined, groups: undefined, copyable: true, size: "md" },
);

const toast = useToast();
const copied = ref(false);
let resetTimer: ReturnType<typeof setTimeout> | null = null;

const full = computed<string>(() => groupFingerprint(props.fingerprint));
const chunks = computed<string[]>(() => {
  const all = full.value === "" ? [] : full.value.split(" ");
  return props.groups === undefined ? all : all.slice(0, props.groups);
});
const truncated = computed<boolean>(() => chunks.value.length < full.value.split(" ").length);

async function copy(): Promise<void> {
  if (full.value === "") return;
  const ok = await copyToClipboard(props.fingerprint.replace(/\s+/g, ""));
  if (ok) {
    copied.value = true;
    toast.success("指纹已复制", full.value);
    if (resetTimer !== null) clearTimeout(resetTimer);
    resetTimer = setTimeout(() => {
      copied.value = false;
    }, 1_600);
  } else {
    toast.error("复制失败", "当前环境不允许访问剪贴板");
  }
}
</script>

<template>
  <div class="fp" :class="`s-${size}`">
    <span v-if="label" class="fp-label">{{ label }}</span>
    <span class="fp-body">
      <code class="fp-value cm-mono">
        <span v-for="(chunk, index) in chunks" :key="index" class="chunk">{{ chunk }}</span>
        <span v-if="truncated" class="chunk more">…</span>
      </code>
      <button
        v-if="copyable"
        class="fp-copy"
        type="button"
        :title="copied ? '已复制' : '复制完整指纹'"
        :aria-label="copied ? '已复制' : '复制完整指纹'"
        @click.stop.prevent="copy"
      >
        <AppIcon :name="copied ? 'check' : 'copy'" :size="14" />
      </button>
    </span>
  </div>
</template>

<style scoped>
.fp {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.fp-label {
  color: var(--text-dim);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.fp-body {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.fp-value {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  min-width: 0;
  padding: 4px 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
  color: var(--text-muted);
  font-size: 12px;
  letter-spacing: 0.04em;
  line-height: 1.35;
}

.s-sm .fp-value {
  padding: 2px 6px;
  font-size: 11px;
}

.s-lg .fp-value {
  padding: 7px 10px;
  font-size: 14px;
  color: var(--text);
}

.chunk {
  font-variant-numeric: tabular-nums;
}

.more {
  color: var(--text-dim);
}

.fp-copy {
  display: grid;
  place-items: center;
  flex: none;
  width: 26px;
  height: 26px;
  border: 1px solid transparent;
  border-radius: var(--radius-xs);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  transition:
    background var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.fp-copy:hover {
  background: var(--surface-2);
  color: var(--text);
}
</style>
