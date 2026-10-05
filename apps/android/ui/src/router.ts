import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

import DevicesView from "./views/DevicesView.vue";
import HistoryView from "./views/HistoryView.vue";
import HomeView from "./views/HomeView.vue";
import SettingsView from "./views/SettingsView.vue";

/** Android 端 4 个标签页。用 hash 模式，Tauri 打包后无需服务端 rewrite。 */
export const routes: RouteRecordRaw[] = [
  { path: "/", name: "home", component: HomeView, meta: { title: "ClipMesh" } },
  { path: "/devices", name: "devices", component: DevicesView, meta: { title: "设备" } },
  { path: "/history", name: "history", component: HistoryView, meta: { title: "历史" } },
  { path: "/settings", name: "settings", component: SettingsView, meta: { title: "设置" } },
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
