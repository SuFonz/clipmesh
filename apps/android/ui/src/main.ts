import { createPinia } from "pinia";
import { createApp } from "vue";

import { configureMock, isMock } from "@clipmesh/ui-core";

import App from "./App.vue";
import { router } from "./router";

const app = createApp(App);

app.use(createPinia());
app.use(router);

/**
 * 告诉 mock 后端"我是 Android"——这样 `android_*` 系列命令在浏览器里也能用，
 * 状态里的 platform 也是 android。跑在真正的 Tauri 里时这行是空操作。
 */
configureMock({ platform: "android" });

app.mount("#app");

if (isMock()) {
  console.info(
    "[clipmesh] 未检测到 Tauri 运行时，已切换到内存 mock 后端（Android 模式，端口 1421）。",
  );
}
