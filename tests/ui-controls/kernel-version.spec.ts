import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (error) => { throw error; });
  await page.route('**/*', (route) => {
    if (new URL(route.request().url()).hostname === '127.0.0.1') return route.continue();
    throw new Error(`Unexpected external request: ${route.request().url()}`);
  });
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string) => {
      if (command === 'logs_append') return;
      throw new Error(`Unexpected Tauri command: ${command}`);
    } } });
  });
});

test('older stable is not advertised as an upgrade to a newer RC', async ({ page }) => {
  await page.goto('/?panel=kernel-card');
  await expect(page.getByText('v0.0.17-rc.1', { exact:true })).toBeVisible();
  await expect(page.getByText('暂无稳定版更新', { exact:true })).toBeVisible();
  await expect(page.getByText('v0.0.16 可用')).toHaveCount(0);
});

test('download completion stays pending and rollback details remain visible', async ({ page }) => {
  await page.goto('/?panel=kernel');
  await page.getByRole('button', { name:'版本管理', exact:true }).click();
  await page.getByRole('button', { name:'安装', exact:true }).click();
  await expect(page.getByRole('dialog').getByRole('status')).toContainText('下载完成，正在校验并安装');
  await expect(page.getByRole('button', { name:'取消', exact:true })).toBeDisabled();
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('fixture-install-stage', { detail:'rolling_back' })));
  await expect(page.getByRole('dialog').getByRole('status')).toContainText('正在恢复原版本');
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('fixture-install-finish', { detail:'failure' })));
  await expect(page.getByRole('alert')).toContainText('已恢复原版本和连接');
  await expect(page.getByRole('alert')).toContainText('/fixture/backup');
  await expect(page.getByRole('button', { name:'安装', exact:true })).toBeEnabled();
  await expect(page.getByText('安装成功', { exact:true })).toHaveCount(0);
});

test('successful install does not submit a second configuration write', async ({ page }) => {
  await page.goto('/?panel=kernel');
  await page.evaluate(() => {
    document.body.dataset.configWrites = '0';
    window.addEventListener('fixture-unexpected-config-write', () => { document.body.dataset.configWrites = '1'; });
  });
  await page.getByRole('button', { name:'版本管理', exact:true }).click();
  await page.getByRole('button', { name:'安装', exact:true }).click();
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('fixture-install-finish', { detail:'success' })));
  await expect(page.getByText('安装成功', { exact:true })).toBeVisible();
  await expect(page.getByRole('button', { name:'关闭', exact:true }).filter({ hasText:'关闭' })).toBeEnabled();
  await expect(page.locator('body')).toHaveAttribute('data-config-writes', '0');
});

test('release list failure is distinct from an empty channel', async ({ page }) => {
  await page.goto('/?panel=kernel&scenario=list-error');
  await page.getByRole('button', { name:'版本管理', exact:true }).click();
  await expect(page.getByRole('alert')).toContainText('发布服务器暂不可用');
  await expect(page.getByText('该渠道暂无可用版本')).toHaveCount(0);
});
