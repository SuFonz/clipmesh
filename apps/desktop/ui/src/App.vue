<script setup lang="ts">
import { RouterView } from "vue-router";

import { ToastHost, useCoreEvents } from "@clipmesh/ui-core";

import { useContextMenuGuard } from "./composables/useContextMenuGuard";
import DesktopLayout from "./layouts/DesktopLayout.vue";

/**
 * 桌面端根组件。
 *
 * 做三件事：挂一次 `useCoreEvents()`（初始填充 + 订阅全部 `clipmesh://` 事件）、
 * 挂一次 `useContextMenuGuard()`（屏蔽 WebView2 的默认右键菜单），
 * 以及把当前路由塞进 DesktopLayout 的默认插槽。
 */
useCoreEvents();
useContextMenuGuard();
</script>

<template>
  <DesktopLayout>
    <RouterView v-slot="{ Component }">
      <Transition name="page" mode="out-in">
        <component :is="Component" />
      </Transition>
    </RouterView>
  </DesktopLayout>

  <ToastHost placement="bottom-right" />
</template>

<style>
/* 桌面端：整个窗口不滚动，滚动交给内容区 */
html,
body {
  overflow: hidden;
}

.page-enter-active,
.page-leave-active {
  transition:
    opacity 140ms var(--ease),
    transform 140ms var(--ease);
}

.page-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.page-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
