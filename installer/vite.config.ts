import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: __dirname,
  plugins: [react()],
  base: "./",
  build: {
    outDir: "dist",
    // 由构建脚本预先清理目录，避免环境的批量删除守卫拦截 vite 的 emptyDir
    emptyOutDir: false,
    target: "chrome105",
    minify: true,
  },
});
