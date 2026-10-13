import { test, expect, type Page } from '@playwright/test';
async function openVersions(page: Page, scenario = '', kernel = false) {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.addInitScript(({ scenario }) => {
    const target = window as unknown as {
      __releaseCalls: { command: string; args?: Record<string, unknown> }[];
      __resolveOldCatalog?: () => void;
      __TAURI_INTERNALS__: { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> };
      __notesExecuted?: boolean;
    };
    target.__releaseCalls = [];
    let preferences = { clientAutoCheck: false, clientChannel: 'stable', kernelChannel: 'stable' };
    let revision = 'revision-before';
    const snapshot = () => ({ preferences: { ...preferences }, expectedRevision: revision, schedulerState: 'not-adapted' });
    const catalog = (source: string, channel: string) => ({
      source, channel, platform: 'android', checkedAtUnixMs: '1788697000000', catalogLimit: 30,
      targetStatus: source === 'kernel' ? 'bundled-via-client' : 'installation-not-adapted',
      targetReason: '目标安装适配尚未完成；目录版本不是兼容性确认。',
      releases: channel === 'test' ? [] : [{
        tagName: channel === 'stable' ? 'v1.0.1' : 'v1.1.0-rc.2', version: channel === 'stable' ? '1.0.1' : '1.1.0-rc.2', channel,
        kernelChannel: source === 'kernel' ? channel === 'stable' ? 'stable' : 'beta' : null,
        publishedAt: null, notes: '<img src=x onerror="window.__notesExecuted=true">原始发行说明', notesTruncated: false,
        releaseUrl: `https://github.com/zerodenet/${source === 'kernel' ? 'core' : 'znet-sink'}/releases/tag/v1.0.1`,
      }],
    });
    target.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      target.__releaseCalls.push({ command, args });
      if (command === 'plugin:app|version') return '1.0.0';
      if (command === 'mobile_release_preferences') return snapshot();
      if (command === 'mobile_release_preferences_save') {
        preferences = args!.preferences as typeof preferences; revision = 'revision-after';
        if (scenario === 'unknown-save') throw { code: 'connection_closed', message: 'raw-token-secret' };
        return snapshot();
      }
      if (command === 'mobile_release_catalog') {
        const source = args!.source as string; const channel = args!.channel as string;
        if (scenario === 'stale-channel' && channel === 'stable') return new Promise(resolve => { target.__resolveOldCatalog = () => resolve(catalog(source, channel)); });
        if (scenario === 'catalog-failed') throw new Error('https://secret.test/token');
        return catalog(source, channel);
      }
      return null;
    } };
  }, { scenario });
  await page.goto(`/mobile-versions.html${kernel ? '?kernel' : ''}`);
  await expect(page.getByText('1.0.0', { exact: true })).toBeVisible();
}
test('real version page checks channel metadata, renders notes as text and persists only ordinary intent', async ({ page }) => {
  await openVersions(page);
  await expect(page.getByRole('status').filter({ hasText: '发现较新的项目版本' })).toBeVisible();
  await expect(page.locator('.release-notes')).toContainText('<img src=x');
  await expect(page.locator('.release-notes img')).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { __notesExecuted?: boolean }).__notesExecuted)).toBeUndefined();
  await page.getByRole('button', { name: '自动检查意图：关闭' }).click();
  expect(await page.evaluate(() => (window as unknown as { __releaseCalls: { command: string }[] }).__releaseCalls.filter(call => call.command.endsWith('_save')).length)).toBe(0);
  await page.getByRole('button', { name: '保存发行偏好', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: '此开关只保存意图' })).toBeVisible();
  const writes = await page.evaluate(() => (window as unknown as { __releaseCalls: { command: string; args?: Record<string, unknown> }[] }).__releaseCalls.filter(call => call.command.endsWith('_save')));
  expect(writes).toEqual([{ command: 'mobile_release_preferences_save', args: { expectedRevision: 'revision-before', preferences: { clientAutoCheck: true, clientChannel: 'stable', kernelChannel: 'stable' } } }]);
  await page.getByRole('radio', { name: '开发版', exact: true }).click(); await page.getByRole('button', { name: '检查当前渠道' }).click();
  await expect(page.getByRole('status').filter({ hasText: '暂无发行记录' })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBe(0);
  const buttons = await page.locator('button').all(); for (const button of buttons) { const box = await button.boundingBox(); if (box) expect(box.height).toBeGreaterThanOrEqual(44); }
  expect(await page.evaluate(() => (window as unknown as { __releaseCalls: { command: string }[] }).__releaseCalls.some(call => /download|install|updater|core_start/.test(call.command)))).toBe(false);
});
test('old pending channel cannot overwrite candidate result and failed catalog stays explicitly unknown', async ({ page }) => {
  await openVersions(page, 'stale-channel');
  await page.getByRole('radio', { name: '候选版', exact: true }).click(); await page.getByRole('button', { name: '检查当前渠道' }).click();
  await expect(page.getByRole('heading', { name: '1.1.0-rc.2 发行说明' })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __resolveOldCatalog?: () => void }).__resolveOldCatalog?.());
  await expect(page.getByRole('heading', { name: '1.1.0-rc.2 发行说明' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '1.0.1 发行说明' })).toHaveCount(0);
  const failed = await page.context().newPage();
  await openVersions(failed, 'catalog-failed');
  await expect(failed.getByRole('alert')).toContainText('无法确认是否有更新'); await expect(failed.getByRole('alert')).not.toContainText('secret');
  await expect(failed.getByRole('status').filter({ hasText: '未发现更新版本' })).toHaveCount(0);
  await failed.close();
});
test('unknown preference save only rechecks current values and kernel version remains unknown without a bundle', async ({ page }) => {
  await openVersions(page, 'unknown-save');
  await page.getByRole('button', { name: '自动检查意图：关闭' }).click(); await page.getByRole('button', { name: '保存发行偏好', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('保存结果未确认');
  await expect(page.getByRole('button', { name: '保存发行偏好', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '核对当前保存值' }).click();
  await expect(page.getByRole('status').filter({ hasText: '原保存回执仍未确认' })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __releaseCalls: { command: string }[] }).__releaseCalls.filter(call => call.command.endsWith('_save')).length)).toBe(1);
  await openVersions(page, '', true);
  await expect(page.getByText('随包内核产物未接入', { exact: true })).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: '当前版本未确认' })).toBeVisible();
  await expect(page.getByRole('button', { name: '通过客户端升级更新内核' })).toBeVisible();
  await expect(page.getByText('候选版对应内核 beta', { exact: false })).toBeVisible();
});
