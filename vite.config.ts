/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives the dev server on a fixed port and expects a fixed public dir.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
    // Only used when the frontend is opened in a plain browser instead of the
    // Tauri window; the desktop app routes queries through the Rust backend.
    proxy: {
      "/helix": {
        target: process.env.HELIX_URL || "http://localhost:6969",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/helix/, ""),
      },
    },
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
