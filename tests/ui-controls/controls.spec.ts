import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (error) => console.error('UI fixture runtime error:', error));
  // The fixture has no reason to contact anything outside its static UI server.
  await page.route('**/*', (route) => {
    if (new URL(route.request().url()).hostname === '127.0.0.1') return route.continue();
    throw new Error(`Unexpected external UI-fixture request: ${route.request().url()}`);
  });
  await page.addInitScript(() => localStorage.setItem('znet-rules-view-mode', 'card'));
  await page.goto('/');
  await expect(page.locator('main')).toHaveCSS('display', 'flex');
});

test.afterEach(async ({ page }, testInfo) => {
  if (testInfo.status !== testInfo.expectedStatus) {
    console.error('UI failure geometry:', JSON.stringify(await page.locator(
      '[data-slot="select-content"], [data-select-viewport], [data-bits-floating-content-wrapper], [data-slot="dialog-content"], [aria-label="长菜单"]',
    ).evaluateAll((nodes) => nodes.map((node) => {
      const style = getComputedStyle(node);
      return {
        slot: node.getAttribute('data-slot') ?? node.getAttribute('data-select-viewport'),
        rect: node.getBoundingClientRect().toJSON(),
        maxHeight: style.maxHeight,
        available: style.getPropertyValue('--bits-floating-available-height'),
        position: style.position,
        transform: style.transform,
        translate: style.translate,
        inlineStyle: node.getAttribute('style'),
        scrollHeight: node.scrollHeight,
        scrollTop: node.scrollTop,
      };
    }))));
    console.error('UI failure accessibility snapshot:', await page.locator('body').ariaSnapshot());
  }
});

test('rule actions use styled controls and preserve numeric ordering', async ({ page }, testInfo) => {
  const action = page.getByRole('button', { name: '匹配动作' }).first();
  await expect(action).toHaveAttribute('aria-haspopup', 'listbox');
  await expect(action).toHaveText('直连');
  await expect(page.locator('select')).toHaveCount(0);
  await expect(action).toHaveCSS('height', '30px');
  await expect(page.getByRole('spinbutton', { name: '测试数字' })).toHaveCSS('height', '30px');
  await expect(action).toHaveCSS('border-radius', '7px');
  await action.click();
  await page.getByRole('option', { name: '代理', exact: true }).click();
  await expect(action).toHaveText('代理');
  const order = page.getByRole('spinbutton', { name: '公共规则顺序' }).first();
  await order.fill('25');
  await order.press('Tab');
  await expect(order).toHaveValue('25');
  await page.screenshot({ path: testInfo.outputPath('controls-light.png'), fullPage: true });
  await page.getByRole('button', { name: '切换主题' }).click();
  await expect(page.locator('html')).toHaveClass('dark');
  await page.screenshot({ path: testInfo.outputPath('controls-dark.png'), fullPage: true });
});

test('select inside draggable dialog stays above overlay and Escape closes only the menu', async ({ page }) => {
  await page.getByRole('button', { name: '新建', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  const trigger = page.getByRole('button', { name: '规则类型' });
  await trigger.click();
  await page.getByRole('option', { name: '精确域名', exact: true }).click();
  await expect(trigger).toHaveText('精确域名');
  await trigger.click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox')).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(trigger).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});

test('source interval selector saves a number instead of a string', async ({ page }) => {
  await page.getByRole('button', { name: '新建', exact: true }).click();
  await page.getByRole('radio', { name: '外部导入', exact: true }).click();
  await page.getByPlaceholder('例如：AI 服务、局域网直连').fill('UI fixture');
  await page.getByPlaceholder('https://example.com/rules.yaml').fill('https://example.invalid/rules');
  await page.getByRole('button', { name: '自动更新' }).click();
  await page.getByRole('option', { name: '每小时', exact: true }).click();
  await page.getByRole('button', { name: '导入并构建', exact: true }).click();
  await expect(page.getByLabel('保存结果')).toContainText('"updateIntervalSecs":3600');
});

test('long menus fit a short window and keyboard selection works', async ({ page }) => {
  await page.setViewportSize({ width: 650, height: 480 });
  await page.getByRole('button', { name: '更多选项' }).click();
  await expect(page.getByRole('dialog')).toHaveCSS('position', 'fixed');
  const trigger = page.getByRole('button', { name: '长菜单' });
  await trigger.click();
  const menu = page.getByRole('listbox');
  await expect(menu).toBeVisible();
  await expect(menu).not.toHaveCSS('max-height', 'none');
  await expect(menu).toHaveCSS('z-index', '1100');
  await expect.poll(async () => {
    const bounds = await menu.boundingBox();
    return {
      bounds,
      topInside: bounds !== null && bounds.y >= 0,
      bottomInside: bounds !== null && bounds.y + bounds.height <= 480,
      leftInside: bounds !== null && bounds.x >= 0,
      rightInside: bounds !== null && bounds.x + bounds.width <= 650,
    };
  }).toMatchObject({ topInside: true, bottomInside: true, leftInside: true, rightInside: true });
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await expect(trigger).toHaveText('选项 79');
  await expect(page.getByRole('dialog')).toBeVisible();
});

test('choice controls retain native keyboard semantics and disabled inputs', async ({ page }) => {
  const check = page.getByRole('checkbox', { name: '保留来源' });
  await check.focus();
  await page.keyboard.press('Space');
  await expect(check).toBeChecked();
  const radio = page.getByRole('radio', { name: '保留配置' });
  await radio.focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('radio', { name: '删除配置' })).toBeChecked();
  await expect(page.getByRole('button', { name: '禁用操作' })).toBeDisabled();
  await expect(page.getByRole('textbox', { name: '禁用输入' })).toBeDisabled();
});
