import { test, expect } from '@playwright/test';
test('public latency URL is validated and saved with client-owned tolerance', async ({page}) => {
  test.setTimeout(90_000);
  await page.goto('/?panel=url-test', {waitUntil:'domcontentloaded', timeout:60_000});
  const url = page.getByRole('textbox', {name:'公共测速地址'});
  await expect(url).toHaveValue('http://www.gstatic.com/generate_204');
  await url.fill('socks5://example.test');
  await page.getByRole('button', {name:'应用', exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('HTTP(S)');
  await expect(page.getByLabel('保存结果')).toBeEmpty();
  await url.fill('https://probe.example/204');
  await page.getByRole('spinbutton', {name:'URLTest 延迟容差'}).fill('0');
  await page.getByRole('button', {name:'应用', exact:true}).click();
  await expect(page.getByLabel('保存结果')).toContainText('"urlTest.url":"https://probe.example/204","urlTest.toleranceMs":0');
  await expect(page.getByRole('button', {name:'已应用', exact:true})).toBeVisible();
});
