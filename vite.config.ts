import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite 配置：Tauri 要求固定端口 1420，且禁用 clearScreen 以兼容 Tauri 输出
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    // 不在构建前清空 dist：避免文件删除失败导致构建中断
    // （旧文件带 hash，不影响 index.html 引用）
    emptyOutDir: false,
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
