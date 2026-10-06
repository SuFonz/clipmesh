<script setup lang="ts">
import { RouterView } from "vue-router";

import { ToastHost, androidApi, toMessage, useCoreEvents } from "@clipmesh/ui-core";

import MobileLayout from "./layouts/MobileLayout.vue";

/**
 * Android 端根组件。挂一次 `useCoreEvents()`，其余交给 MobileLayout。
 */
useCoreEvents();

/**
 * 启动时申请一次通知权限（Android 13 及以上），并让前台服务跟权限保持一致。
 *
 * 只在**还没拿到**权限时才申请：已经允许过的用户不该每次打开都被问一遍，
 * 系统那边也不会再弹框。设置页的常驻开关只**查询**权限状态、不主动申请，
 * 所以这里是启动路径上唯一的申请点，不会和设置页重复打扰用户。
 *
 * 权限在 ⇒ 服务在（`startService` 幂等，Rust 侧启动时通常已经拉起来了）。
 *
 * 权限被拒时**不动服务**：Android 允许前台服务在没有 POST_NOTIFICATIONS 的情况下
 * 运行，只是那条常驻通知不显示。用户拒绝通知往往只是嫌通知烦，不是不想同步 ——
 * 拿它顺手把后台同步一起关掉是过度反应。设置页那个开关会显示成「关」（它反映的是
 * 权限），但同步照常。
 */
async function ensureNotificationsOnce(): Promise<void> {
  try {
    const granted = await androidApi.isNotificationPermissionGranted();
    if (!granted) {
      await androidApi.requestNotificationPermission();
      return;
    }

    await androidApi.startService();
  } catch (cause) {
    // 非 Android 构建没有这些命令 —— 不是错误，记一行日志就够。
    console.info("[clipmesh] 通知权限不可用（非 Android 构建）：", toMessage(cause));
  }
}

void ensureNotificationsOnce();
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
