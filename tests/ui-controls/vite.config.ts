import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath } from 'node:url';
const absolute = (path: string) => fileURLToPath(new URL(path, import.meta.url));
export default defineConfig({
  root: absolute('./'),
  plugins: [tailwindcss(), svelte({ configFile: false })],
  resolve: { alias: [
    { find: '$lib/services/config', replacement: absolute('./config.ts') },
    { find: '$lib/services/core', replacement: absolute('./config.ts') },
    { find: '$lib', replacement: absolute('../../src/lib') },
  ] },
  server: { host: '127.0.0.1', port: 4177, strictPort: true },
  build: { outDir: absolute('../../build/ui-controls'), emptyOutDir: true },
});
