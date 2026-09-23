/// <reference types="vitest/config" />
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// Tauri drives the dev server on a fixed port and expects a fixed public dir.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 14237,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 14238 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Tauri targets a known webview, so we can emit modern output.
  build: {
    target: "es2022",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // Debug builds stay unminified so a stack trace from the webview is usable.
    minify: !process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
  },
});
