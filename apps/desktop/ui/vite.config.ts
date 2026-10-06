import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// vite.config.ts 由 Node 执行，但本项目没有装 @types/node —— 这里只声明用到的那一点。
declare const process: { env: Record<string, string | undefined> };

// `tauri dev` 在真机/模拟器上调试时会把 dev server 暴露到局域网
const host = process.env.TAURI_DEV_HOST;
const isWindows = process.env.TAURI_ENV_PLATFORM === "windows";

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],

  // 让 vite 认识 TAURI_ENV_* 前缀的环境变量
  envPrefix: ["VITE_", "TAURI_ENV_"],

  // 1. 不要把 rust 的报错刷掉
  clearScreen: false,

  // 2. tauri 的 devUrl 固定是 http://localhost:1420，端口被占就直接失败
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. 不要盯着 src-tauri 看
      ignored: ["**/src-tauri/**"],
    },
  },

  build: {
    // Tauri 的 WebView 版本是已知的，不用为老浏览器降级太多
    target: isWindows ? "chrome105" : "safari13",
    sourcemap: false,
  },
});
