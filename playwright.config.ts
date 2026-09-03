import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/ui-controls',
  testMatch: '*.spec.ts',
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: { baseURL: 'http://127.0.0.1:4177', viewport: { width: 900, height: 650 }, trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  projects: [ { name: 'chromium', use: { browserName: 'chromium' } }, { name: 'webkit', use: { browserName: 'webkit' } } ],
  webServer: { command: 'pnpm exec svelte-kit sync && pnpm exec vite --config tests/ui-controls/vite.config.ts', url: 'http://127.0.0.1:4177', reuseExistingServer: false, timeout: 120000 },
});
