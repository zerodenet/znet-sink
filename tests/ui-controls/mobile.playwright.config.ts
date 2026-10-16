import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import base from '../../playwright.config';
export default defineConfig({
  ...base,
  testDir: fileURLToPath(new URL('./', import.meta.url)),
  testMatch: 'mobile-*.spec.ts',
  webServer: { command: 'pnpm exec svelte-kit sync && pnpm exec vite --config tests/ui-controls/vite.config.ts', url: 'http://127.0.0.1:4177', timeout: 120000, cwd: fileURLToPath(new URL('../../', import.meta.url)), reuseExistingServer: !process.env.CI },
  projects: [{ name: 'chromium', use: {
    browserName: 'chromium',
    launchOptions: { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE },
  } }],
});
