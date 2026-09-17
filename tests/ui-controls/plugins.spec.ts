import { test, expect, type Page } from '@playwright/test';
async function openInstalledPlugin(page: Page) {
  const entry = page.getByRole('button', { name: '打开 示例插件 详情', exact: true });
  await expect(entry).toBeVisible();
  await entry.click();
}
async function fixture(page: Page, discover = false, count = 1, configurable = false, duplicateCatalog = false, initialInstalled = true, installedVersion = '1.0.0', foundation = false, componentCount = 1, permissionExpansion = false, managementPage = false) {
  await page.addInitScript(({ count, configurable, duplicateCatalog, initialInstalled, installedVersion, foundation, componentCount, permissionExpansion, managementPage }) => {
    const component = {
      plugin_id: 'org.example.plugin', name: '示例插件', description: '这是用于验证独立插件详情页的说明。', component_id: foundation ? 'foundation' : 'identity', version: installedVersion, publisher: 'example',
      repository: 'https://github.com/example/plugin', documentation: 'https://github.com/example/plugin/blob/main/README.md', homepage: null, license: 'MIT', surfaces: [],
      review: { key: 'org.example.plugin/identity', identity: 'digest', registration: 1, revision: 0 },
      permissions: foundation ? [] : [
        { request: { capability: 'plugin.self.read', scope: 'self' }, required: true, supported: true, granted: false },
        { request: { capability: 'records.summary.read', scope: 'selection:test' }, required: false, supported: false, granted: false },
      ], enabled: false, running: false, blocked: null,
      configuration: configurable ? {
        schema: { title: '来源配置', description: '配置兼容的服务来源。', fields: [
          { id: 'name', label: '来源名称', kind: 'text', required: true, options: [] },
          { id: 'origin', label: '服务地址', kind: 'https_origin', required: true, options: [] },
          { id: 'path', label: '通信路径', kind: 'select', required: true, default: 'direct', options: [{ value: 'direct', label: '直连' }, { value: 'core', label: '通过内核' }] },
        ] }, values: { path: 'direct' }, configured: false,
      } : null,
    };
    const components = [component, ...Array.from({ length: Math.max(0, componentCount - 1) }, (_, index) => ({
      ...structuredClone(component),
      component_id: `worker-${index + 1}`,
      review: { ...component.review, key: `${component.plugin_id}/worker-${index + 1}` },
      configuration: null,
    }))];
    let installed = initialInstalled;
    let pending: ((value: unknown) => void) | null = null;
    let snapshotCalls = 0;
    let pageLoads = 0;
    const calls: Array<{ command: string; args: any }> = [];
    const state = () => structuredClone({ checked: true, components: installed ? components : [], pages: installed && managementPage ? [{ plugin_id: component.plugin_id, id: 'manage', title: 'Connect 管理', kind: 'management' }] : [], notices: [] });
    Object.assign(window, {
      __pluginFixture: {
        revoke: () => { component.review.revision++; component.enabled = false; },
        late: () => { pending?.({ result: 'late-canary' }); },
        snapshotCalls: () => snapshotCalls,
        pageLoads: () => pageLoads,
        calls,
      },
      __TAURI_INTERNALS__: { invoke: async (command: string, args: any) => {
        if (command === 'plugins_catalog') {
          const catalog = Array.from({ length: count }, (_, index) => ({ id: index === 0 ? component.plugin_id : `org.example.plugin${index}`, name: index === 0 ? '在线示例' : `插件 ${index + 1}`, description: 'Connects compatible subscription providers and clients for device authorization, managed subscription synchronization, and read-only provider messages.', publisher: { id: 'example' }, repository: 'https://github.com/example/plugin', surfaces: configurable ? ['settings'] : [] }));
          return duplicateCatalog ? [...catalog, catalog[0]] : catalog;
        }
        if (command === 'plugins_releases') return [{ tag_name: 'v2.0.0-beta', channel: 'dev', prerelease: true, body: '预发布说明' }, { tag_name: 'v1.1.0', channel: 'stable', prerelease: false, body: '稳定版本说明', html_url: 'https://github.com/example/plugin/releases/tag/v1.1.0' }];
        if (command === 'plugin:dialog|open') return '/tmp/example.zspkg';
        if (command === 'plugins_preview_install') return {
          plugin_id: component.plugin_id,
          current_version: installed ? component.version : null,
          candidate_version: '1.0.0-local',
          candidate_digest: 'local-candidate-digest',
          publisher: 'example',
          publisher_fingerprint: 'SHA256:11:22:33:44',
          first_install: !installed,
          local_trust: true,
          requested_surfaces: ['znet-sink.ui.management.v1'],
          requires_approval: true,
          added_permissions: [{ component_id: 'identity', request: { capability: 'plugin.self.read', scope: 'self' }, required: true }],
          removed_permissions: [],
        };
        if (command === 'plugins_install') {
          if (args.path !== '/tmp/example.zspkg' || args.approvalDigest !== 'local-candidate-digest') throw new Error('缺少本地安装确认');
          installed = true;
          component.version = '1.0.0-local';
          component.enabled = false;
          return state();
        }
        if (command === 'plugins_preview_release') return { plugin_id: component.plugin_id, current_version: installed ? component.version : null, candidate_version: args.tag.replace(/^v/, ''), candidate_digest: 'candidate-digest', added_permissions: permissionExpansion ? [{ component_id: 'identity', request: { capability: 'network.request', scope: 'https://panel.example.com' }, required: true }] : [], removed_permissions: [] };
        if (command === 'plugins_install_release') {
          if (args.id !== component.plugin_id || args.tag !== 'v1.1.0') throw new Error('错误的安装选择');
          if (permissionExpansion && args.approvalDigest !== 'candidate-digest') throw new Error('缺少权限变化确认');
          installed = true; component.version = '1.1.0'; component.enabled = false;
          return state();
        }
        if (command === 'plugin:opener|open_url') { (window as any).__openedPluginUrl = args.url; return; }
        if (command === 'plugins_supported') return true;
        if (command === 'plugins_snapshot' || command === 'plugins_refresh') {
          if (command === 'plugins_snapshot') snapshotCalls++;
          return state();
        }
        if (command === 'plugins_page') {
          pageLoads++;
          return `<!doctype html><html><body><main data-znet-layout="settings"><nav data-znet-settings-nav><div data-znet-nav-title>Connect</div><button data-znet-settings-item aria-current="step">来源</button></nav><section data-znet-settings-content><header data-znet-page-header><h1>Connect 管理</h1></header><div data-znet-panel><output id="config">正在读取配置</output><label data-znet-field><span>来源名称</span><input aria-label="来源名称" /></label><button id="login" data-variant="primary">登录并同步</button><output id="result"></output></div></section></main><script>(async()=>{const value=await znetPlugin.configuration.get('identity');document.getElementById('config').textContent='配置 '+value.path})().catch(error=>{document.getElementById('config').textContent=error.message});document.getElementById('login').onclick=async()=>{await znetPlugin.storage.putJson('identity','state','device/session',{credential:'remote-issued'});const value=await znetPlugin.invoke('identity','login',{account:'demo'});document.getElementById('result').textContent='订阅 '+value.subscriptions.length}</script></body></html>`;
        }
        if (command === 'plugins_storage_put' || command === 'plugins_invoke') {
          if (args.pluginId !== component.plugin_id) throw new Error('插件身份越界');
          calls.push({ command, args: structuredClone(args) });
          if (command === 'plugins_invoke') return { subscriptions: ['a', 'b'] };
          return;
        }
        if (command === 'plugins_authorize') {
          if (args.review.revision !== component.review.revision) throw { message: '插件状态或授权已变化，请重新检查并确认权限' };
          if (foundation ? args.grants.length !== 0 : args.grants.length !== 1 || args.grants[0].capability !== 'plugin.self.read') throw { message: '权限越界' };
          component.enabled = true; component.review.revision++; if (!foundation) component.permissions[0].granted = true;
          return state();
        }
        if (command === 'plugins_configure') {
          if (!component.configuration || args.key !== component.review.key || args.values.origin !== 'https://panel.example.com') throw { message: '配置无效' };
          component.configuration.values = structuredClone(args.values); component.configuration.configured = true;
          component.enabled = false; component.review.revision++;
          return state();
        }
        if (command === 'plugins_run') {
          if (foundation) return { status: 'configured-foundation', implemented: ['package-verification', 'component-startup', 'declarative-configuration'], unavailable: ['authorization', 'subscription-sync', 'messages'] };
          component.running = true; return new Promise(resolve => { pending = resolve; });
        }
        if (command === 'plugins_stop') { component.enabled = false; component.running = false; component.review.revision++; return state(); }
        if (command === 'plugins_revoke_permissions') { component.enabled = false; component.permissions.forEach(permission => { permission.granted = false; }); component.review.revision++; return state(); }
        if (command === 'plugins_uninstall') { installed = false; return state(); }
        throw new Error(`Unexpected command: ${command}`);
      } },
    });
  }, { count, configurable, duplicateCatalog, initialInstalled, installedVersion, foundation, componentCount, permissionExpansion, managementPage });
  await page.goto('/?panel=plugins', { waitUntil: 'domcontentloaded' });
  if (initialInstalled) await expect(page.getByText('示例插件', { exact: true })).toBeVisible();
}
test('permissions are explicit and unsupported access cannot be selected; stopping discards a late result', async ({ page }) => {
  await fixture(page);
  await openInstalledPlugin(page);
  await page.locator('.plugin-diagnostics summary').click();
  await expect(page.getByRole('button', { name: '运行诊断', exact: true })).toBeDisabled();
  await page.getByRole('radio', { name: '权限', exact: true }).click();
  const permissions = page.locator('.permission-row').getByRole('checkbox');
  await expect(page.getByRole('button', { name: '允许并启用' })).toBeDisabled();
  await expect(permissions.nth(1)).toBeDisabled();
  await permissions.first().check();
  await page.screenshot({ path: '/tmp/znet-plugin-permissions.png' });
  await page.getByRole('button', { name: '允许并启用' }).click();
  await page.getByRole('radio', { name: '管理', exact: true }).click();
  await page.locator('.plugin-diagnostics summary').click();
  await page.getByRole('button', { name: '运行诊断', exact: true }).click();
  await page.getByRole('button', { name: '停用组件', exact: true }).click();
  await page.evaluate(() => (window as any).__pluginFixture.late());
  await expect(page.getByText('组件已停用；已批准权限仍然保留', { exact: true })).toBeVisible();
  await expect(page.getByText('late-canary', { exact: false })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '运行诊断', exact: true })).toBeDisabled();
  await page.getByRole('radio', { name: '关于', exact: true }).click();
  await page.getByRole('button', { name: '卸载', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '卸载', exact: true }).click();
  await expect(page.getByText('插件已卸载', { exact: true })).toBeVisible();
});
test('a stale permission review displays rejection instead of enabling the component', async ({ page }) => {
  await fixture(page);
  await openInstalledPlugin(page);
  await page.getByRole('radio', { name: '权限', exact: true }).click();
  await page.locator('.permission-row').getByRole('checkbox').first().check();
  await page.evaluate(() => (window as any).__pluginFixture.revoke());
  await page.getByRole('button', { name: '允许并启用' }).click();
  await expect(page.getByText('插件状态或授权已变化，请重新检查并确认权限', { exact: true })).toBeVisible();
  await page.getByRole('radio', { name: '管理', exact: true }).click();
  await page.locator('.plugin-diagnostics summary').click();
  await expect(page.getByRole('button', { name: '运行诊断', exact: true })).toBeDisabled();
});

test('local package review stays inside the install window and returns to the catalog', async ({ page }) => {
  await fixture(page, false, 1, false, false, false);
  await page.getByRole('button', { name: '安装插件', exact: true }).first().click();
  await expect(page.getByRole('dialog')).toHaveCount(1);
  await page.getByRole('dialog').getByRole('button', { name: '从本地安装', exact: true }).click();

  const localReview = page.getByRole('dialog').filter({ hasText: '确认本地插件来源' });
  await expect(localReview).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(1);
  await expect(page.getByText('线上插件市场', { exact: true })).toBeHidden();
  await expect(localReview.getByText('SHA256:11:22:33:44', { exact: true })).toBeVisible();
  await expect(localReview.getByRole('button', { name: '全屏', exact: true })).toBeVisible();

  await localReview.getByRole('button', { name: '返回', exact: true }).click();
  const returnedMarket = page.getByRole('dialog').filter({ hasText: '线上插件市场' });
  await expect(returnedMarket).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(1);

  await returnedMarket.getByRole('button', { name: '从本地安装', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '信任并安装', exact: true }).click();
  await expect(page.getByText('1.0.0-local', { exact: true })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('a fresh market install uses the latest stable compatible release without a version picker', async ({ page }) => {
  await fixture(page, true, 1, false, false, false);
  await page.getByRole('button', { name: '安装插件', exact: true }).first().click();
  const installWindow = page.getByRole('dialog');
  const market = installWindow.filter({ hasText: '线上插件市场' });
  await expect(market.getByText('在线示例', { exact: true })).toBeVisible();
  await expect(market.getByRole('button', { name: '从本地安装', exact: true })).toBeVisible();
  await expect(market.getByRole('button', { name: '全屏', exact: true })).toBeVisible();
  await market.getByRole('button', { name: '安装最新版' }).click();
  await expect(installWindow.getByText('v1.1.0（正式版）', { exact: true })).toBeVisible();
  await expect(installWindow.getByText('最新兼容版本', { exact: true })).toBeVisible();
  await expect(installWindow.getByRole('button', { name: '目标版本', exact: true })).toHaveCount(0);
  await expect(page.getByRole('dialog')).toHaveCount(1);
  await installWindow.getByRole('button', { name: '查看发行说明' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__openedPluginUrl)).toBe('https://github.com/example/plugin/releases/tag/v1.1.0');
  await installWindow.getByRole('button', { name: '安装 v1.1.0' }).click();
  await expect(page.getByText('1.1.0', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '打开 示例插件 详情', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '详情', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '安装插件', exact: true }).click();
  const reopenedMarket = page.getByRole('dialog').filter({ hasText: '线上插件市场' });
  await expect(reopenedMarket.getByText('已安装 1.1.0', { exact: true })).toBeVisible();
  await expect(reopenedMarket.getByRole('button', { name: '管理插件', exact: true })).toBeVisible();
  await expect(reopenedMarket.getByRole('button', { name: '版本管理', exact: true })).toBeVisible();
});

test('permission-expanding updates show an exact review before installation', async ({ page }) => {
  await fixture(page, true, 1, false, false, true, '1.0.0', false, 1, true);
  await page.getByRole('button', { name: '安装插件', exact: true }).click();
  const market = page.getByRole('dialog');
  await market.getByRole('button', { name: '版本管理', exact: true }).click();
  await expect(market.getByRole('button', { name: '目标版本', exact: true })).toBeVisible();
  await market.getByRole('button', { name: '下载并更新' }).click();
  await expect(market.getByText('此更新申请新增或扩大权限', { exact: true })).toBeVisible();
  await expect(market.getByText('https://panel.example.com', { exact: true })).toBeVisible();
  await market.getByRole('button', { name: '确认权限变化并更新' }).click();
  await expect(page.getByText('1.1.0', { exact: true })).toBeVisible();
});

test('catalog entries are deduplicated and declarative configuration gates enabling', async ({ page }) => {
  await fixture(page, true, 1, true, true);
  await page.getByRole('button', { name: '安装插件', exact: true }).click();
  const market = page.getByRole('dialog').filter({ hasText: '线上插件市场' });
  await expect(market.getByText('在线示例', { exact: true })).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(page.getByText('待配置', { exact: true })).toBeVisible();
  await openInstalledPlugin(page);
  await page.getByRole('textbox', { name: '来源名称', exact: true }).fill('我的面板');
  await page.getByRole('textbox', { name: '服务地址', exact: true }).fill('https://panel.example.com');
  await page.getByRole('button', { name: '保存配置', exact: true }).click();
  await expect(page.getByText('配置已保存', { exact: true })).toBeVisible();
  await page.getByRole('radio', { name: '权限', exact: true }).click();
  await page.locator('.permission-row').getByRole('checkbox').first().check();
  await expect(page.getByRole('button', { name: '允许并启用', exact: true })).toBeEnabled();
});
test('multiple components stay grouped under one installed plugin card', async ({ page }) => {
  await fixture(page, false, 1, false, false, true, '1.0.0', false, 2);
  await expect(page.getByRole('article')).toHaveCount(1);
  const entry = page.getByRole('button', { name: '打开 示例插件 详情', exact: true });
  await entry.focus();
  await page.keyboard.press('Enter');
  await page.getByRole('radio', { name: '关于', exact: true }).click();
  await expect(page.getByText('identity、worker-1', { exact: true })).toBeVisible();
});

test('a signed plugin management page owns its workflow through the bound host sdk', async ({ page }) => {
  await fixture(page, false, 1, true, false, true, '1.0.0', false, 1, false, true);
  await openInstalledPlugin(page);
  await expect(page.locator('.plugin-detail-hero')).toHaveCount(0);
  const detailToolbar = page.locator('.plugin-detail-toolbar');
  await expect(detailToolbar.getByRole('radio', { name: '管理' })).toBeVisible();
  await expect(detailToolbar.getByRole('switch')).toBeVisible();
  const frame = page.frameLocator('iframe[title="Connect 管理"]');
  await expect(frame.getByRole('heading', { name: 'Connect 管理' })).toBeVisible();
  await expect(frame.getByText('配置 direct', { exact: true })).toBeVisible();
  await expect(frame.locator('style[data-znet-plugin-ui="1"]')).toHaveCount(1);
  await expect(frame.locator('[data-znet-panel]')).toHaveCSS('border-radius', '10px');
  const snapshotCalls = await page.evaluate(() => (window as any).__pluginFixture.snapshotCalls());
  await frame.getByRole('textbox', { name: '来源名称' }).fill('正在编辑，不能丢失');
  await page.waitForTimeout(3300);
  await expect(frame.getByRole('textbox', { name: '来源名称' })).toHaveValue('正在编辑，不能丢失');
  expect(await page.evaluate(() => (window as any).__pluginFixture.snapshotCalls())).toBe(snapshotCalls);
  expect(await page.evaluate(() => (window as any).__pluginFixture.pageLoads())).toBe(1);
  await frame.getByRole('button', { name: '登录并同步' }).click();
  await expect(frame.getByText('订阅 2', { exact: true })).toBeVisible();
  const calls = await page.evaluate(() => (window as any).__pluginFixture.calls);
  expect(calls.map((call: any) => call.command)).toEqual(['plugins_storage_put', 'plugins_invoke']);
  expect(calls.every((call: any) => call.args.pluginId === 'org.example.plugin')).toBe(true);

  for (const size of [{ width: 900, height: 650 }, { width: 1680, height: 960 }]) {
    await page.setViewportSize(size);
    const layout = await page.locator('.plugins-detail-scroll').evaluate(container => {
      const surface = container.querySelector('.plugin-page-frame');
      if (!(surface instanceof HTMLElement)) throw new Error('plugin management surface is missing');
      const containerRect = container.getBoundingClientRect();
      const surfaceRect = surface.getBoundingClientRect();
      return {
        bottomGap: Math.round(containerRect.bottom - surfaceRect.bottom),
        scrolls: container.scrollHeight > container.clientHeight + 1,
      };
    });
    expect(layout.bottomGap).toBeLessThanOrEqual(13);
    expect(layout.scrolls).toBe(false);
  }
  await page.screenshot({ path: '/tmp/znet-plugin-management-fill.png' });
});
test('an installed local build cannot be mistaken for an older remote update', async ({ page }) => {
  await fixture(page, true, 1, false, false, true, '2.0.0');
  await page.getByRole('button', { name: '安装插件', exact: true }).click();
  const market = page.getByRole('dialog');
  await market.getByRole('button', { name: '版本管理', exact: true }).click();
  await expect(market.getByText('当前已安装 2.0.0，所选远端版本 1.1.0 更旧。', { exact: false })).toBeVisible();
  await expect(market.getByRole('button', { name: '不能安装旧版本', exact: true })).toBeDisabled();
});
test('foundation execution explains the self-check and keeps technical JSON collapsed', async ({ page }) => {
  await fixture(page, false, 1, true, false, true, '1.0.0', true);
  await openInstalledPlugin(page);
  await expect(page.getByText('基础验证组件', { exact: true })).toBeVisible();
  await page.getByRole('textbox', { name: '来源名称', exact: true }).fill('我的面板');
  await page.getByRole('textbox', { name: '服务地址', exact: true }).fill('https://panel.example.com');
  await page.getByRole('button', { name: '保存配置', exact: true }).click();
  await page.getByRole('radio', { name: '权限', exact: true }).click();
  await page.getByRole('button', { name: '允许并启用', exact: true }).click();
  await page.getByRole('radio', { name: '管理', exact: true }).click();
  await page.locator('.plugin-diagnostics summary').click();
  await page.getByRole('button', { name: '运行诊断', exact: true }).click();
  await expect(page.getByText('基础组件自检完成', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('这个基础组件不会执行设备授权、订阅同步或消息读取。', { exact: false })).toBeVisible();
  await expect(page.getByText('configured-foundation', { exact: false })).not.toBeVisible();
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
  await page.getByRole('button', { name: '安装插件', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('网络暂不可用');
  await page.getByRole('button', { name: '重试', exact: true }).click();
  await expect(page.getByText('暂时没有已登记的插件', { exact: false })).toBeVisible();
});

for (const mode of ['lite', 'pro']) {
  test(`plugins has a top-level menu in ${mode} mode`, async ({ page }) => {
    await fixture(page, true);
    await page.goto(`/?panel=plugins-shell&mode=${mode}`);
    await expect(page.getByRole('navigation', { name: '主导航' }).getByRole('tab', { name: '插件', exact: true })).toHaveAttribute('aria-selected', 'true');
    await expect(page.getByText('已安装插件', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: '安装插件', exact: true }).click();
    const market = page.getByRole('dialog').filter({ hasText: '线上插件市场' });
    await expect(market.getByText('在线示例', { exact: true })).toBeVisible();
    await expect(market.getByRole('button', { name: '从本地安装', exact: true })).toBeVisible();
    await expect(market.getByRole('button', { name: '全屏', exact: true })).toBeVisible();
    await page.screenshot({ path: `/tmp/znet-plugins-${mode}.png` });
  });
}

for (const theme of ['light', 'dark']) {
  test(`plugin workspace uses the shared panel and keeps long catalogs scrollable in ${theme}`, async ({ page }) => {
    await fixture(page, true, 18);
    await page.goto(`/?panel=plugins-shell&mode=pro&theme=${theme}`);
    const panel = page.locator('.plugins-panel');
    await expect(panel).toHaveClass(/desk-card/);
    await expect(panel.getByText('已安装插件', { exact: true })).toBeVisible();
    await panel.getByRole('button', { name: '安装插件', exact: true }).click();
    const market = page.getByRole('dialog').filter({ hasText: '线上插件市场' });
    await expect(market.getByRole('article')).toHaveCount(18);
    const marketCollection = market.locator('.plugins-collection');
    await expect(marketCollection).toHaveClass(/list-view/);
    await market.getByRole('radio', { name: '卡片视图', exact: true }).click();
    await expect(marketCollection).toHaveClass(/card-view/);
    await market.getByRole('radio', { name: '列表视图', exact: true }).click();
    const scroll = market.locator('.plugin-catalog-scroll');
    await expect.poll(() => scroll.evaluate(el => el.scrollHeight > el.clientHeight)).toBe(true);
    await expect.poll(() => market.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
    await scroll.evaluate(el => { el.scrollTop = el.scrollHeight; });
    await expect(market.getByRole('button', { name: '刷新目录' })).toBeInViewport();
    await expect(market.getByText('插件 18', { exact: true })).toBeInViewport();
    await scroll.evaluate(el => { el.scrollTop = 0; });
    await page.screenshot({ path: `/tmp/znet-plugins-unified-${theme}.png` });
    await expect.poll(() => page.evaluate(() => ({
      html: document.documentElement.scrollHeight <= innerHeight,
      body: document.body.scrollHeight <= innerHeight,
    }))).toEqual({ html: true, body: true });
    await page.keyboard.press('Escape');
    await expect(panel.getByText('示例插件', { exact: true })).toBeVisible();
    const installedCollection = panel.locator('.installed-plugins');
    await expect(installedCollection).toHaveClass(/list-view/);
    await panel.getByRole('radio', { name: '卡片视图', exact: true }).click();
    await expect(installedCollection).toHaveClass(/card-view/);
    await panel.getByRole('radio', { name: '列表视图', exact: true }).click();
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
  await panel.getByRole('button', { name: '安装插件', exact: true }).click();
  const market = page.getByRole('dialog');
  await expect(market.getByRole('article')).toHaveCount(8);
  await expect.poll(() => panel.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await expect(market.getByRole('button', { name: '刷新目录' })).toBeInViewport();
  await market.getByRole('button', { name: '版本管理' }).first().click();
  await expect(market.getByRole('button', { name: '下载并更新' })).toBeInViewport();
  await expect.poll(() => market.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await page.screenshot({ path: '/tmp/znet-plugins-small-release.png' });
  await market.getByRole('button', { name: '返回插件市场', exact: true }).click();
  await expect(market.getByText('线上插件市场', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(market).not.toBeVisible();
  await panel.getByRole('button', { name: '打开 示例插件 详情', exact: true }).click();
  await panel.getByRole('radio', { name: '权限', exact: true }).click();
  await panel.getByRole('button', { name: '允许并启用' }).scrollIntoViewIfNeeded();
  await expect(panel.getByRole('button', { name: '允许并启用' })).toBeInViewport();
  await expect.poll(() => panel.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await page.screenshot({ path: '/tmp/znet-plugins-small-permissions.png' });
});
