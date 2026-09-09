import { test, expect } from '@playwright/test';
test('module diagnostics preserves partial results and the last snapshot after refresh failure', async ({ page }) => {
  test.setTimeout(90000);
  await page.goto('/?panel=modules', {waitUntil:'domcontentloaded', timeout:60000});
  const panel = page.getByRole('region', {name:'模块状态'});
  await expect(panel.getByRole('heading',{name:'内核托管'})).toBeVisible();
  await expect(panel.getByText('配置状态暂不可用', {exact:false})).toBeVisible();
  await expect(panel.getByText('进行中', {exact:true})).toBeVisible();
  await panel.getByRole('button',{name:'刷新',exact:true}).click();
  await expect(panel.getByRole('alert')).toContainText('仍显示上次快照');
  await expect(panel.getByRole('heading',{name:'内核托管'})).toBeVisible();
  await page.setViewportSize({width:390,height:844});
  expect(await panel.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  await page.screenshot({path:'/tmp/znet-module-diagnostics.png', fullPage:true});
});
