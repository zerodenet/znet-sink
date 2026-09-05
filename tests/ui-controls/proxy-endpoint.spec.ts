import { test, expect } from '@playwright/test';

test('custom profile endpoint shows the actual address and explains where to edit', async ({ page }) => {
  await page.goto('/?panel=endpoint&custom=1');
  await expect(page.getByRole('textbox', { name: '代理监听地址' })).toHaveValue('127.0.0.2');
  await expect(page.getByRole('textbox', { name: '代理监听端口' })).toHaveValue('8899');
  await expect(page.getByRole('textbox', { name: '代理监听端口' })).toBeDisabled();
  await expect(page.getByRole('button', { name: '保存', exact: true })).toBeDisabled();
  await expect(page.getByText('当前地址和端口由配置文件定义，请在配置编辑器修改入站设置。')).toBeVisible();
});

test('managed endpoint rejects partial numbers and saves an exact numeric port', async ({ page }) => {
  await page.goto('/?panel=endpoint');
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
