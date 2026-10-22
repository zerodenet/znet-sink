import { expect, test, type Page } from '@playwright/test';
type Call = { command: string; args?: Record<string, unknown> };
type Fixture = { __calls: Call[]; __setUpdate: (patch: Record<string, unknown>) => void; __failInstall: boolean };
async function open(page: Page, initial = 'idle', empty = false) {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.addInitScript(({ initial }) => {
    const target = window as unknown as Fixture & { __TAURI_INTERNALS__: { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> } };
    target.__calls = []; target.__failInstall = false;
    let state = { state: initial, operationId: initial === 'idle' ? null : 'e2695293-5580-4eec-9437-a1e6b2621111', reason: null, packageName: 'org.zerodenet.znetsink.debug', installedVersion: '1.0.0', installedVersionCode: '1', targetVersion: initial === 'idle' ? null : '1.1.0', targetVersionCode: initial === 'idle' ? null : '2', downloadedBytes: initial === 'idle' ? '0' : '100', sizeBytes: initial === 'idle' ? null : '100', assetName: initial === 'idle' ? null : 'client-x86.apk', sha256: initial === 'idle' ? null : 'a'.repeat(64), installPermission: 'allowed', confirmationAvailable: false, installerStatus: null };
    target.__setUpdate = patch => { state = { ...state, ...patch } as typeof state; };
    target.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      target.__calls.push({ command, args });
      if (command === 'mobile_app_update_download') state = { ...state, state: 'queued', operationId: args!.operationId as string, sizeBytes: '100', downloadedBytes: '0', sha256: args!.expectedSha256 as string, targetVersion: '1.1.0', assetName: 'client-x86.apk' };
      if (command === 'mobile_app_update_action' && args?.action === 'install') {
        if (target.__failInstall) throw Error('lost-reply-token-must-not-render');
        state = { ...state, state: 'awaiting-user', confirmationAvailable: true };
      }
      if (command === 'mobile_app_update_action' && args?.action === 'confirm') state = { ...state, state: 'installing', confirmationAvailable: false };
      return structuredClone(state);
    } };
  }, { initial });
  await page.goto(`/mobile-app-update.html${empty ? '?empty' : ''}`);
  await expect(page.getByText('org.zerodenet.znetsink.debug', { exact: true })).toBeVisible();
}
async function calls(page: Page, command: string) { return page.evaluate(command => (window as unknown as Fixture).__calls.filter(c => c.command === command), command); }
test('320px download review fixes asset identity and requires explicit consent', async ({ page }) => {
  await open(page);
  const download = page.getByRole('button', { name: /下载 client-x86.apk/ });
  await download.click();
  expect(await calls(page, 'mobile_app_update_download')).toHaveLength(0);
  await expect(page.getByRole('dialog')).toContainText('a'.repeat(64));
  await page.getByRole('button', { name: '确认下载并检查', exact: true }).click();
  const requests = await calls(page, 'mobile_app_update_download');
  expect(requests).toHaveLength(1);
  expect(requests[0].args).toMatchObject({ tag: 'v1.1.0', assetId: '10', expectedSha256: 'a'.repeat(64), channel: 'stable' });
  expect(requests[0].args).not.toHaveProperty('url');
  await expect(page.getByRole('status')).toHaveText('等待系统调度下载');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy();
  const box = await page.getByRole('button', { name: '读取原更新状态及安装版本' }).boundingBox();
  expect(box!.height).toBeGreaterThanOrEqual(44);
});
test('lost installation reply is read-only until status check; system confirmation is another explicit tap', async ({ page }) => {
  await open(page, 'ready');
  await page.evaluate(() => { (window as unknown as Fixture).__failInstall = true; });
  await page.getByRole('button', { name: '审阅并交给系统安装器', exact: true }).click();
  expect(await calls(page, 'mobile_app_update_action')).toHaveLength(0);
  await page.getByRole('button', { name: '准备系统安装会话', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('操作回执未确认');
  await expect(page.getByRole('button', { name: '审阅并交给系统安装器', exact: true })).toBeDisabled();
  expect(await calls(page, 'mobile_app_update_action')).toHaveLength(1);
  await expect(page.locator('body')).not.toContainText('lost-reply-token');
  await page.evaluate(() => (window as unknown as Fixture).__setUpdate({ state: 'awaiting-user', confirmationAvailable: true }));
  await page.getByRole('button', { name: '读取原更新状态及安装版本', exact: true }).click();
  await expect(page.getByRole('button', { name: '打开系统安装确认', exact: true })).toBeEnabled();
  expect(await calls(page, 'mobile_app_update_action')).toHaveLength(1);
  await page.getByRole('button', { name: '打开系统安装确认', exact: true }).click();
  const actions = await calls(page, 'mobile_app_update_action');
  expect(actions.map(c => c.args?.action)).toEqual(['install', 'confirm']);
  expect(actions[1].args?.operationId).toBe('e2695293-5580-4eec-9437-a1e6b2621111');
});
test('a release without digest-pinned APK keeps the reason and cannot download', async ({ page }) => {
  await open(page, 'idle', true);
  await expect(page.getByText(/此版本没有带可信 GitHub SHA-256 摘要的 APK/)).toBeVisible();
  await expect(page.getByRole('button', { name: /^下载 / })).toHaveCount(0);
  expect(await calls(page, 'mobile_app_update_download')).toHaveLength(0);
});
