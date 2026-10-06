<script setup lang="ts">
import { computed } from "vue";

import { useRelativeTime } from "../composables/useRelativeTime";
import { t } from "../i18n";
import type { Platform } from "../types";
import { platformLabel } from "../utils/format";
import AppIcon from "./AppIcon.vue";
import FingerprintBadge from "./FingerprintBadge.vue";
import StatusPill from "./StatusPill.vue";
import { PLATFORM_ICONS } from "./icons";

/**
 * 一台设备的行：平台图标 + 名字 + 状态 + 指纹 + 可选的地址/最后可见时间。
 * 纯展示组件 —— 操作按钮通过 `#actions` 插槽传进来，所以 peers / trusted /
 * pairing 三种列表都能复用它。
 */
const props = withDefaults(
  defineProps<{
    name: string;
    platform: Platform;
    fingerprint: string;
    /** 是否已建立 TLS 会话。 */
    online?: boolean;
    /** 是否在信任列表里。 */
    trusted?: boolean;
    /** 是否正在配对。 */
    pairing?: boolean;
    address?: string;
    lastSeen?: number;
    /** 正在执行操作（按钮转圈）。 */
    busy?: boolean;
    /** 灰掉整行（例如已断开的设备）。 */
    dimmed?: boolean;
    /** 隐藏指纹（移动端窄屏时用）。 */
    hideFingerprint?: boolean;
    /** 自定义副标题，给了就不显示地址行。 */
    subtitle?: string;
  }>(),
  {
    online: false,
    trusted: false,
    pairing: false,
    address: undefined,
    lastSeen: undefined,
    busy: false,
    dimmed: false,
    hideFingerprint: false,
    subtitle: undefined,
  },
);

const { format } = useRelativeTime();

const icon = computed(() => PLATFORM_ICONS[props.platform] ?? "unknown");
const platformText = computed<string>(() => platformLabel(props.platform));
const stateLabel = computed<string>(() => {
  if (props.pairing) return t("components.device.pairing");
  if (props.online) return t("components.device.connected");
  if (props.trusted) return t("components.device.offline");
  return t("components.device.discovered");
});
const stateTone = computed<"ok" | "warn" | "idle" | "accent">(() => {
  if (props.pairing) return "accent";
  if (props.online) return "ok";
  if (props.trusted) return "idle";
  return "warn";
});
const lastSeenText = computed<string>(() =>
  props.lastSeen === undefined ? "" : format(props.lastSeen),
);
</script>

<template>
  <div class="device" :class="{ dimmed, busy }">
    <span class="avatar" :class="{ online }">
      <AppIcon :name="icon" :size="20" />
      <span v-if="online" class="live" aria-hidden="true" />
    </span>

    <div class="main">
      <div class="line-1">
        <span class="name cm-truncate">{{ name }}</span>
        <StatusPill :label="stateLabel" :tone="stateTone" size="sm" />
        <StatusPill
          v-if="trusted"
          :label="t('components.device.trusted')"
          tone="ok"
          icon="shield"
          size="sm"
          class="trusted-pill"
        />
      </div>

      <p v-if="subtitle" class="line-2 cm-truncate">{{ subtitle }}</p>
      <p v-else class="line-2">
        <span class="platform">{{ platformText }}</span>
        <template v-if="address">
          <span class="sep">·</span>
          <span class="cm-mono addr">{{ address }}</span>
        </template>
        <template v-if="lastSeenText">
          <span class="sep">·</span>
          <span>{{ lastSeenText }}</span>
        </template>
      </p>

      <FingerprintBadge
        v-if="!hideFingerprint"
        class="fp"
        :fingerprint="fingerprint"
        :groups="4"
        size="sm"
      />
    </div>

    <div v-if="$slots.actions" class="actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<style scoped>
.device {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  transition:
    background var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease);
}

.device:hover {
  background: var(--surface-hover);
  border-color: var(--border-strong);
}

.device.dimmed {
  opacity: 0.68;
}

.avatar {
  position: relative;
  display: grid;
  place-items: center;
  flex: none;
  width: 40px;
  height: 40px;
  border-radius: var(--radius-sm);
  background: var(--surface-2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.avatar.online {
  background: var(--accent-soft);
  border-color: var(--border-accent);
  color: var(--accent);
}

.live {
  position: absolute;
  right: -3px;
  bottom: -3px;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--ok);
  border: 2px solid var(--surface);
}

.main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.line-1 {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
}

.name {
  font-size: 13.5px;
  font-weight: 600;
  min-width: 0;
}

.line-2 {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 5px;
  color: var(--text-muted);
  font-size: 12px;
}

.sep {
  color: var(--text-dim);
}

.addr {
  font-size: 11.5px;
}

.fp {
  margin-top: 2px;
}

.actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}

@media (max-width: 560px) {
  .trusted-pill {
    display: none;
  }

  .device {
    padding: 11px;
  }
}
</style>
