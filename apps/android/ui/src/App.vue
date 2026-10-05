<script setup lang="ts">
import { RouterView } from "vue-router";

import { ToastHost, useCoreEvents } from "@clipmesh/ui-core";

import MobileLayout from "./layouts/MobileLayout.vue";

/**
 * Android 端根组件。挂一次 `useCoreEvents()`，其余交给 MobileLayout。
 */
useCoreEvents();
</script>

<template>
  <MobileLayout>
    <RouterView v-slot="{ Component }">
      <Transition name="page" mode="out-in">
        <component :is="Component" />
      </Transition>
    </RouterView>
  </MobileLayout>

  <ToastHost placement="bottom-center" />
</template>

<style>
/* 移动端：整页不滚动，滚动交给布局里的内容区，避免地址栏抖动 */
html,
body {
  overflow: hidden;
  overscroll-behavior: none;
}

.page-enter-active,
.page-leave-active {
  transition:
    opacity 130ms var(--ease),
    transform 130ms var(--ease);
}

.page-enter-from {
  opacity: 0;
  transform: translateY(8px);
}

.page-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
