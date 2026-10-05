import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

import DashboardView from "./views/DashboardView.vue";
import DevicesView from "./views/DevicesView.vue";
import HistoryView from "./views/HistoryView.vue";
import PairingView from "./views/PairingView.vue";
import SettingsView from "./views/SettingsView.vue";

/**
 * 桌面端路由。用 hash 模式：Tauri 打包后是自定义协议 + 静态文件，
 * hash 模式不需要服务端 rewrite，最稳。
 */
export const routes: RouteRecordRaw[] = [
  { path: "/", name: "dashboard", component: DashboardView, meta: { title: "仪表盘" } },
  { path: "/devices", name: "devices", component: DevicesView, meta: { title: "设备" } },
  { path: "/history", name: "history", component: HistoryView, meta: { title: "历史" } },
  { path: "/pairing", name: "pairing", component: PairingView, meta: { title: "配对" } },
  { path: "/settings", name: "settings", component: SettingsView, meta: { title: "设置" } },
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
  scrollBehavior: () => ({ top: 0 }),
});
