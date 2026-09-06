import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const absolute = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const root = absolute('../../');
const legacyId = absolute('./LegacyOverview.svelte');
export default defineConfig({
  root: absolute('./'), publicDir: absolute('../../static'),
  plugins: [{
    name: 'original-overview-from-git',
    resolveId(id) { if (id === 'virtual:original-overview') return legacyId; },
    load(id) {
      if (id === legacyId) return execFileSync('git', ['show', '1f3f6d6:src/lib/components/tabs/OverviewTab.svelte'], { cwd: root, encoding: 'utf8' });
    },
  }, tailwindcss(), svelte({ configFile: false })],
  resolve: { alias: [
    ...['store.svelte', 'gui-state.svelte', 'core-events.svelte', 'updater.svelte', 'proxy-config-signal.svelte', 'traffic-ball-preference.svelte'].map(name => ({ find: `$lib/services/${name}`, replacement: absolute('./state.svelte.ts') })),
    ...['core', 'config', 'kernel-version', 'runtime-performance', 'traffic-ball', 'toast.svelte'].map(name => ({ find: `$lib/services/${name}`, replacement: absolute('./api.ts') })),
    { find: '@tauri-apps/api/app', replacement: absolute('./native.ts') },
    { find: '@tauri-apps/api/window', replacement: absolute('./native.ts') },
    { find: '$lib', replacement: absolute('../../src/lib') },
  ] },
  server: { host: '127.0.0.1', port: 4178, strictPort: true },
  build: { outDir: absolute('../../build/client-preview'), emptyOutDir: true, rollupOptions: { input: absolute('./overview-preview.html') } },
});
