import { test, expect } from '@playwright/test';
test('module diagnostics preserves partial results and the last snapshot after refresh failure', async ({ page }) => {
  test.setTimeout(90000);
  await page.goto('/?panel=modules', {waitUntil:'domcontentloaded', timeout:60000});
  const panel = page.getByRole('region', {name:'模块状态'});
  await expect(panel.getByRole('heading',{name:'内核托管'})).toBeVisible();
  await expect(panel.getByText('配置状态暂不可用', {exact:false})).toBeVisible();
  await expect(panel.locator('.state-badge', {hasText:'进行中'})).toBeVisible();
  await expect(panel.getByLabel('模块状态汇总')).toContainText('需要关注');
  for (const width of [1920, 900, 390]) {
    await page.setViewportSize({width,height:1000});
    expect(await panel.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
    expect(await panel.locator('.module-grid').evaluate(element => getComputedStyle(element).gridTemplateColumns.split(' ').length)).toBeLessThanOrEqual(3);
    await page.screenshot({path:`/tmp/znet-modules-${width}.png`,fullPage:true});
  }
  await page.setViewportSize({width:1280,height:1000});
  await page.getByRole('button',{name:'切换主题',exact:true}).click();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.screenshot({path:'/tmp/znet-modules-dark.png',fullPage:true});
  await panel.getByRole('heading',{name:'路由追踪',exact:true}).scrollIntoViewIfNeeded();
  await expect(panel.getByRole('heading',{name:'路由追踪',exact:true})).toBeVisible();
  await panel.getByRole('button',{name:'刷新',exact:true}).click();
  await expect(panel.getByRole('alert')).toContainText('仍显示上次快照');
  await expect(panel.getByRole('heading',{name:'内核托管'})).toBeVisible();
  await page.setViewportSize({width:390,height:844});
  expect(await panel.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  await page.screenshot({path:'/tmp/znet-module-diagnostics.png', fullPage:true});
});
