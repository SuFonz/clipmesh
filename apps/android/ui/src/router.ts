import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

import type { MessageKey } from "@clipmesh/ui-core";

import DevicesView from "./views/DevicesView.vue";
import HistoryView from "./views/HistoryView.vue";
import HomeView from "./views/HomeView.vue";
import SettingsView from "./views/SettingsView.vue";

declare module "vue-router" {
  interface RouteMeta {
    /**
     * 页面标题的**文案键**（`t()` 的入参），不是成品字符串。
     *
     * 放键而不是放句子：vue-router 合并嵌套路由的 meta 时会把值拍平，
     * 放一个算好的字符串就永远停在模块加载那一刻，切语言不会跟着变。
     * 底部标签栏用的是同一批键，标题栏与标签栏永远不会对不上。
     */
    title?: MessageKey;
  }
}

/** Android 端 4 个标签页。用 hash 模式，Tauri 打包后无需服务端 rewrite。 */
export const routes: RouteRecordRaw[] = [
  { path: "/", name: "home", component: HomeView, meta: { title: "android.nav.home" } },
  { path: "/devices", name: "devices", component: DevicesView, meta: { title: "android.nav.devices" } },
  { path: "/history", name: "history", component: HistoryView, meta: { title: "android.nav.history" } },
  {
    path: "/settings",
    name: "settings",
    component: SettingsView,
    meta: { title: "android.nav.settings" },
  },
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
