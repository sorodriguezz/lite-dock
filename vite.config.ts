import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri injects this when developing against a physical device / LAN host.
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [svelte()],

  // Prevent Vite from obscuring Rust errors in the terminal.
  clearScreen: false,

  server: {
    // Tauri expects a fixed port and fails if it is not available.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      // Don't watch the Rust side — Cargo handles that.
      ignored: ["**/src-tauri/**"],
    },
  },

  // Only these env prefixes are exposed to the frontend.
  envPrefix: ["VITE_", "TAURI_ENV_"],

  build: {
    // WebView2 is evergreen Chromium → we can target modern JS.
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
    // Keep the bundle in a predictable place for tauri.conf.json.
    outDir: "dist",
    emptyOutDir: true,
  },
});
