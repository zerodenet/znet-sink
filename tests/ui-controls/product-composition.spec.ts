import { test, expect } from '@playwright/test';
import { resolveProduct } from '../../scripts/product-composition.mjs';
test('selected diagnostic tools render independently without starting work', async ({page}) => {
  test.setTimeout(90_000);
  const product = resolveProduct();
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/?panel=tools', {waitUntil: 'domcontentloaded', timeout: 60_000});
  await expect(page.getByRole('button', {name:'切换主题'})).toBeVisible();
  await expect(page.getByText('域名查询', {exact:true})).toHaveCount(product.features.includes('tool-dns') ? 1 : 0);
  await expect(page.getByText('路由追踪', {exact:true})).toHaveCount(product.features.includes('tool-route') ? 1 : 0);
  await page.setViewportSize({width:390,height:844});
  expect(await page.locator('.diag-panel').evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  expect(errors).toEqual([]);
});

test('node browsing and selection survive manual probe trimming', async ({page}) => {
  test.setTimeout(90_000);
  const enabled = resolveProduct().features.includes('tool-node-probe');
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/?panel=nodes', {waitUntil:'domcontentloaded',timeout:60_000});
  await expect(page.getByRole('button', {name:/^node-a VLESS 42 ms/})).toBeVisible();
  await expect(page.getByRole('button', {name:'测速',exact:true})).toHaveCount(enabled ? 1 : 0);
  await expect(page.getByRole('button', {name:'测试 node-a 延迟',exact:true})).toHaveCount(enabled ? 1 : 0);
  await page.getByText('node-b', {exact:true}).click();
  await expect(page.getByLabel('保存结果')).toContainText('"selected":"node-b"');
  expect(errors).toEqual([]);
});
