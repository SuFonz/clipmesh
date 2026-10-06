<script setup lang="ts">
import {
  AppCard,
  AppIcon,
  StatusPill,
  isMock,
  t,
  useAppVersion,
  useStatusStore,
} from "@clipmesh/ui-core";

/**
 * 桌面「关于」页。
 *
 * 与 Android 设置页的「关于」卡片逐条对应：应用名 + 版本、运行时指示、
 * 无中心服务器 / 直连 TLS、当前监听端口。
 *
 * 版本号来自 Tauri 包信息（`useAppVersion()`），页面里不写死任何数字 ——
 * 浏览器 mock 下没有 Tauri 宿主，读不到就显示占位符。
 */
const statusStore = useStatusStore();
const version = useAppVersion();

const mock = isMock();
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">{{ t("desktop.nav.about") }}</h1>
        <p class="cm-page-sub">{{ t("desktop.about.subtitle") }}</p>
      </div>
      <div class="cm-page-actions">
        <StatusPill
          :label="statusStore.running ? t('desktop.status.running') : t('desktop.status.stopped')"
          :tone="statusStore.running ? 'ok' : 'idle'"
          :pulse="statusStore.running"
        />
      </div>
    </header>

    <div class="page-stack">
      <AppCard :title="t('desktop.nav.about')" icon="info">
        <div class="about">
          <span class="logo"><AppIcon name="link" :size="20" /></span>
          <div class="about-text">
            <span class="about-name">ClipMesh</span>
            <span class="about-version cm-mono">{{
              t("desktop.about.version", { version: version ?? "—" })
            }}</span>
          </div>
          <StatusPill
            :label="mock ? t('settings.about.mock') : t('settings.about.tauri')"
            :tone="mock ? 'warn' : 'ok'"
            size="sm"
          />
        </div>
        <p class="cm-help mt">
          {{ t("settings.about.body", { port: statusStore.listenPort }) }}
        </p>
        <p v-if="mock" class="cm-help mt-sm">
          {{ t("desktop.about.mockVersionNote") }}
        </p>
        <p class="cm-help mt-sm">
          {{ t("settings.about.licenseBefore") }}
          <span class="cm-mono">LICENSE</span>{{ t("settings.about.licenseAfter") }}
        </p>
      </AppCard>
    </div>
  </div>
</template>

<style scoped>
.view {
  display: block;
}

.mt {
  margin-top: 12px;
}

.mt-sm {
  margin-top: 6px;
}

/* ---------- 关于 ---------- */

.about {
  display: flex;
  align-items: center;
  gap: 10px;
}

.logo {
  display: grid;
  place-items: center;
  width: 38px;
  height: 38px;
  flex: none;
  border-radius: var(--radius-sm);
  background: var(--accent);
  color: var(--accent-fg);
  box-shadow: var(--shadow-1);
}

.about-text {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  line-height: 1.3;
}

.about-name {
  font-size: 15px;
  font-weight: 680;
  letter-spacing: -0.01em;
}

.about-version {
  color: var(--text-muted);
  font-size: 12px;
}
</style>
