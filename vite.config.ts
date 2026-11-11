import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 期望一个固定端口的开发服务器，且不能在 src-tauri 变化时重载
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "chrome110",
    minify: true,
    sourcemap: false,
    chunkSizeWarningLimit: 2000,
    // 主窗口 + 独立文件管理器窗口（两个 HTML 入口）
    rollupOptions: {
      input: {
        main: "index.html",
        files: "files.html",
      },
    },
  },
});
