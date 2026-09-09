import { productComposition } from '../../scripts/product-composition.mjs';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath } from 'node:url';
const absolute = (path: string) => fileURLToPath(new URL(path, import.meta.url));
export default defineConfig({
  root: absolute('./'),
  plugins: [productComposition(), tailwindcss(), svelte({ configFile: false })],
  resolve: { alias: [
    { find: '@tauri-apps/api/event', replacement: absolute('./tauri-events.ts') },
    { find: '$lib/services/theme.svelte', replacement: absolute('./presentation-state.svelte.ts') },
    { find: '$lib/services/traffic-ball-preference.svelte', replacement: absolute('./presentation-state.svelte.ts') },
    { find: '$lib/services/toast.svelte', replacement: absolute('./toast.ts') },
    { find: '$lib/services/core-events.svelte', replacement: absolute('./overview-events.ts') },
    { find: '$lib/services/kernel-version', replacement: absolute('./kernel-version.ts') },
    { find: '$lib/services/runtime-performance', replacement: absolute('./config.ts') },
    { find: '$lib/services/gui-state.svelte', replacement: absolute('./tun-state.svelte.ts') },
    { find: '$lib/services/store.svelte', replacement: absolute('./tun-state.svelte.ts') },
    { find: '$lib/services/tun', replacement: absolute('./tun-state.svelte.ts') },
    { find: '$lib/services/config', replacement: absolute('./config.ts') },
    { find: '$lib/services/core', replacement: absolute('./config.ts') },
    { find: '$lib', replacement: absolute('../../src/lib') },
  ] },
  server: { host: '127.0.0.1', port: 4177, strictPort: true },
  build: { outDir: absolute('../../build/ui-controls'), emptyOutDir: true },
});
