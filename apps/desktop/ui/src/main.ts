import { createPinia } from "pinia";
import { createApp } from "vue";

import { isMock } from "@clipmesh/ui-core";

import App from "./App.vue";
import { router } from "./router";
import "./styles/layout.css";

const app = createApp(App);

app.use(createPinia());
app.use(router);

app.mount("#app");

// 浏览器里跑（没有 Tauri 运行时）时提示一下：数据全部来自内存 mock。
if (isMock()) {
  console.info(
    "[clipmesh] 未检测到 Tauri 运行时，已切换到内存 mock 后端（packages/ui-core/src/api/mock.ts）。",
  );
}
