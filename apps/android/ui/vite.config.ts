import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// vite.config.ts 由 Node 执行，但本项目没有装 @types/node —— 这里只声明用到的那一点。
declare const process: { env: Record<string, string | undefined> };

// Android 真机 / 模拟器调试时，`tauri android dev` 会把 dev server 暴露到局域网
const host = process.env.TAURI_DEV_HOST;

/**
 * Android 端固定 **1421**（桌面端占 1420），与 `apps/android/src-tauri/tauri.conf.json`
 * 里的 devUrl 保持一致。
 */
export default defineConfig({
  plugins: [vue()],

  envPrefix: ["VITE_", "TAURI_ENV_"],

  clearScreen: false,

  server: {
    port: 1421,
    strictPort: true,
    host: host || false,
    // HMR 用 1422，避免和 dev server 端口撞车
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1422,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  build: {
    // Android WebView 是常青的 Chromium
    target: "chrome105",
    sourcemap: false,
  },
});
