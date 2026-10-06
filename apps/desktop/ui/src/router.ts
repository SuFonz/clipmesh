import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

import type { MessageKey } from "@clipmesh/ui-core";

import AboutView from "./views/AboutView.vue";
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
     * 侧边导航用的是同一批键，标题栏与导航永远不会对不上。
     */
    title?: MessageKey;
  }
}

/**
 * 桌面端路由。用 hash 模式：Tauri 打包后是自定义协议 + 静态文件，
 * hash 模式不需要服务端 rewrite，最稳。
 */
export const routes: RouteRecordRaw[] = [
  { path: "/", name: "home", component: HomeView, meta: { title: "desktop.nav.home" } },
  { path: "/devices", name: "devices", component: DevicesView, meta: { title: "desktop.nav.devices" } },
  { path: "/history", name: "history", component: HistoryView, meta: { title: "desktop.nav.history" } },
  {
    path: "/settings",
    name: "settings",
    component: SettingsView,
    meta: { title: "desktop.nav.settings" },
  },
  { path: "/about", name: "about", component: AboutView, meta: { title: "desktop.nav.about" } },
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
  scrollBehavior: () => ({ top: 0 }),
});
