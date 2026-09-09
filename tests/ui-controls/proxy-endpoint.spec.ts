import { test, expect } from '@playwright/test';
test.setTimeout(90_000);

test('client endpoint remains editable when a legacy profile source marker exists', async ({ page }) => {
  await page.goto('/?panel=endpoint&custom=1', {waitUntil:'domcontentloaded', timeout:60_000});
  await expect(page.getByRole('textbox', { name: '代理监听地址' })).toHaveValue('127.0.0.2');
  await expect(page.getByRole('textbox', { name: '代理监听端口' })).toHaveValue('8899');
  await expect(page.getByRole('textbox', { name: '代理监听端口' })).toBeEnabled();
  await expect(page.getByRole('button', { name: '保存', exact: true })).toBeEnabled();
  await expect(page.getByText('客户端设置覆盖订阅和配置文件中的主代理入口', {exact:false})).toBeVisible();
});

test('managed endpoint rejects partial numbers and saves an exact numeric port', async ({ page }) => {
  await page.goto('/?panel=endpoint', {waitUntil:'domcontentloaded', timeout:60_000});
  const port = page.getByRole('textbox', { name: '代理监听端口' });
  const save = page.getByRole('button', { name: '保存', exact: true });
  await port.fill('9000bad');
  await save.click();
  await expect(page.getByRole('alert')).toContainText('监听端口必须');
  await expect(page.getByLabel('保存结果')).toBeEmpty();
  await port.fill('9000');
  await save.click();
  await expect(page.getByLabel('保存结果')).toContainText('"port":9000');
  await expect(page.getByRole('button', { name: '已保存', exact: true })).toBeVisible();
});
