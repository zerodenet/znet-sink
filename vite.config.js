import { defineConfig } from "vite";
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from "@sveltejs/kit/vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    tailwindcss(),
    sveltekit()
  ],

  // Tabs and settings panels are lazy-loaded. Pre-bundle their shared
  // dependencies before Tauri opens the WebView so Vite does not discover a
  // new dependency mid-import, invalidate OverviewTab, and force a reload
  // while Tauri invoke callbacks are still pending.
  optimizeDeps: {
    include: [
      '@lucide/svelte',
      '@tauri-apps/api/app',
      '@tauri-apps/api/core',
      '@tauri-apps/api/event',
      '@tauri-apps/api/window',
      '@tauri-apps/plugin-dialog',
      '@tauri-apps/plugin-opener',
      '@tauri-apps/plugin-updater',
      'bits-ui',
      'tailwind-merge',
      'tailwind-variants',
    ],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
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
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
