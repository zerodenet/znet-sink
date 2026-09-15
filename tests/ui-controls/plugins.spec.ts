import { test, expect, type Page } from '@playwright/test';
async function fixture(page: Page, discover = false, count = 1) {
  await page.addInitScript(({ count }) => {
    const component = {
      plugin_id: 'org.example.plugin', name: '示例插件', component_id: 'identity', version: '1.0.0', publisher: 'example',
      review: { key: 'org.example.plugin/identity', identity: 'digest', registration: 1, revision: 0 },
      permissions: [
        { request: { capability: 'plugin.self.read', scope: 'self' }, required: true, supported: true, granted: false },
        { request: { capability: 'records.summary.read', scope: 'selection:test' }, required: false, supported: false, granted: false },
      ], enabled: false, running: false, blocked: null,
    };
    let installed = true;
    let pending: ((value: unknown) => void) | null = null;
    const state = () => structuredClone({ checked: true, components: installed ? [component] : [], notices: [] });
    Object.assign(window, {
      __pluginFixture: {
        revoke: () => { component.review.revision++; component.enabled = false; },
        late: () => { pending?.({ result: 'late-canary' }); },
      },
      __TAURI_INTERNALS__: { invoke: async (command: string, args: any) => {
        if (command === 'plugins_catalog') return Array.from({ length: count }, (_, index) => ({ id: index === 0 ? component.plugin_id : `org.example.plugin${index}`, name: index === 0 ? '在线示例' : `插件 ${index + 1}`, description: 'Connects compatible subscription providers and clients for device authorization, managed subscription synchronization, and read-only provider messages.', publisher: { id: 'example' }, repository: 'https://github.com/example/plugin' }));
        if (command === 'plugins_releases') return [{ tag_name: 'v2.0.0-beta', channel: 'dev', prerelease: true, body: '预发布说明' }, { tag_name: 'v1.1.0', channel: 'stable', prerelease: false, body: '稳定版本说明', html_url: 'https://github.com/example/plugin/releases/tag/v1.1.0' }];
        if (command === 'plugins_install_release') {
          if (args.id !== component.plugin_id || args.tag !== 'v1.1.0') throw new Error('错误的安装选择');
          installed = true; component.version = '1.1.0'; component.enabled = false;
          return state();
        }
        if (command === 'plugin:opener|open_url') { (window as any).__openedPluginUrl = args.url; return; }
        if (command === 'plugins_supported') return true;
        if (command === 'plugins_snapshot' || command === 'plugins_refresh') return state();
        if (command === 'plugins_authorize') {
          if (args.review.revision !== component.review.revision) throw { message: '插件状态或授权已变化，请重新检查并确认权限' };
          if (args.grants.length !== 1 || args.grants[0].capability !== 'plugin.self.read') throw { message: '权限越界' };
          component.enabled = true; component.review.revision++; component.permissions[0].granted = true;
          return state();
        }
        if (command === 'plugins_run') { component.running = true; return new Promise(resolve => { pending = resolve; }); }
        if (command === 'plugins_stop') { component.enabled = false; component.running = false; component.permissions[0].granted = false; component.review.revision++; return state(); }
        if (command === 'plugins_uninstall') { installed = false; return state(); }
        throw new Error(`Unexpected command: ${command}`);
      } },
    });
  }, { count });
  await page.goto('/?panel=plugins', { waitUntil: 'domcontentloaded' });
  if (!discover) { await page.getByRole('radio', { name: '已安装', exact: true }).click(); await expect(page.getByText('示例插件', { exact: true })).toBeVisible(); }
}
test('permissions are explicit and unsupported access cannot be selected; stopping discards a late result', async ({ page }) => {
  await fixture(page);
  await expect(page.getByRole('button', { name: '运行', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '查看权限' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('button', { name: '允许并启用' })).toBeDisabled();
  await expect(dialog.getByRole('checkbox').nth(1)).toBeDisabled();
  await dialog.getByRole('checkbox').first().check();
  await page.screenshot({ path: '/tmp/znet-plugin-permissions.png' });
  await dialog.getByRole('button', { name: '允许并启用' }).click();
  await page.getByRole('button', { name: '运行', exact: true }).click();
  await page.getByRole('button', { name: '停用', exact: true }).click();
  await page.evaluate(() => (window as any).__pluginFixture.late());
  await expect(page.getByText('权限已撤销', { exact: true })).toBeVisible();
  await expect(page.getByText('late-canary', { exact: false })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '运行', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '卸载', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '卸载', exact: true }).click();
  await expect(page.getByText('插件已卸载', { exact: true })).toBeVisible();
});
test('an old permission dialog displays rejection instead of enabling the component', async ({ page }) => {
  await fixture(page);
  await page.getByRole('button', { name: '查看权限' }).click();
  await page.getByRole('dialog').getByRole('checkbox').first().check();
  await page.evaluate(() => (window as any).__pluginFixture.revoke());
  await page.getByRole('dialog').getByRole('button', { name: '允许并启用' }).click();
  await expect(page.getByText('插件状态或授权已变化，请重新检查并确认权限', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '运行', exact: true })).toBeDisabled();
});

test('online discovery selects a stable release and installs without automatically authorizing', async ({ page }) => {
  await fixture(page, true);
  await expect(page.getByText('在线示例', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '选择版本' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('button', { name: '发布版本', exact: true })).toHaveText('v1.1.0（stable）');
  await dialog.getByRole('button', { name: '发布版本', exact: true }).click();
  await page.getByRole('option', { name: 'v2.0.0-beta（dev）', exact: true }).click();
  await expect(dialog.getByText('这是预发布版本，可能尚不稳定。')).toBeVisible();
  await dialog.getByRole('button', { name: '发布版本', exact: true }).click();
  await page.getByRole('option', { name: 'v1.1.0（stable）', exact: true }).click();
  await dialog.getByRole('button', { name: '查看发行说明' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__openedPluginUrl)).toBe('https://github.com/example/plugin/releases/tag/v1.1.0');
  await dialog.getByRole('button', { name: '下载并安装' }).click();
  await expect(page.getByText('1.1.0', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '运行', exact: true })).toBeDisabled();
});
test('catalog errors can be retried and an empty catalog has an honest empty state', async ({ page }) => {
  await page.addInitScript(() => {
    let attempts = 0;
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string) => {
      if (command === 'plugins_supported') return true;
      if (command === 'plugins_snapshot') return { checked: false, components: [], notices: [] };
      if (command === 'plugins_catalog') { if (++attempts === 1) throw { message: '网络暂不可用' }; return []; }
      throw new Error(command);
    } } });
  });
  await page.goto('/?panel=plugins');
  await expect(page.getByRole('alert')).toContainText('网络暂不可用');
  await page.getByRole('button', { name: '重试', exact: true }).click();
  await expect(page.getByText('暂时没有已登记的插件', { exact: false })).toBeVisible();
});

for (const mode of ['lite', 'pro']) {
  test(`plugins has a top-level menu in ${mode} mode`, async ({ page }) => {
    await fixture(page, true);
    await page.goto(`/?panel=plugins-shell&mode=${mode}`);
    await expect(page.getByRole('navigation', { name: '主导航' }).getByRole('tab', { name: '插件', exact: true })).toHaveAttribute('aria-selected', 'true');
    await expect(page.getByRole('radio', { name: '发现插件', exact: true })).toBeVisible();
    await expect(page.getByText('在线示例', { exact: true })).toBeVisible();
    await page.screenshot({ path: `/tmp/znet-plugins-${mode}.png` });
  });
}

for (const theme of ['light', 'dark']) {
  test(`plugin workspace uses the shared panel and keeps long catalogs scrollable in ${theme}`, async ({ page }) => {
    await fixture(page, true, 18);
    await page.goto(`/?panel=plugins-shell&mode=pro&theme=${theme}`);
    const panel = page.locator('.plugins-panel');
    await expect(panel).toHaveClass(/desk-card/);
    await expect(panel.locator('[data-slot="segmented-root"]')).toBeVisible();
    await expect(panel.getByRole('article')).toHaveCount(18);
    const scroll = panel.locator('.plugins-scroll');
    await expect.poll(() => scroll.evaluate(el => el.scrollHeight > el.clientHeight)).toBe(true);
    await expect.poll(() => panel.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
    await scroll.evaluate(el => { el.scrollTop = el.scrollHeight; });
    await expect(panel.getByRole('button', { name: '刷新目录' })).toBeInViewport();
    await expect(panel.getByText('插件 18', { exact: true })).toBeInViewport();
    await scroll.evaluate(el => { el.scrollTop = 0; });
    await page.screenshot({ path: `/tmp/znet-plugins-unified-${theme}.png` });
    await panel.getByRole('radio', { name: '已安装', exact: true }).click();
    await expect(panel.getByText('示例插件', { exact: true })).toBeVisible();
    await panel.getByRole('textbox', { name: '搜索已安装插件' }).fill('unmatched');
    await expect(panel.getByText('没有找到匹配的插件。')).toBeVisible();
    await panel.getByRole('button', { name: '清除搜索' }).click();
    await page.screenshot({ path: `/tmp/znet-plugins-installed-${theme}.png` });
  });
}

test('small plugin windows keep catalog and permission actions reachable', async ({ page }) => {
  await page.setViewportSize({ width: 520, height: 430 });
  await fixture(page, true, 8);
  await page.goto('/?panel=plugins-shell&mode=lite');
  const panel = page.locator('.plugins-panel');
  await expect(panel.getByRole('article')).toHaveCount(8);
  await expect.poll(() => panel.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await expect(panel.getByRole('button', { name: '刷新目录' })).toBeInViewport();
  await panel.getByRole('button', { name: '选择版本' }).first().click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('button', { name: '下载并安装' })).toBeInViewport();
  await expect.poll(() => dialog.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await page.screenshot({ path: '/tmp/znet-plugins-small-release.png' });
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await panel.getByRole('radio', { name: '已安装', exact: true }).click();
  await panel.getByRole('button', { name: '查看权限' }).click();
  await expect(dialog.getByRole('button', { name: '允许并启用' })).toBeInViewport();
  await expect.poll(() => dialog.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await page.screenshot({ path: '/tmp/znet-plugins-small-permissions.png' });
});
