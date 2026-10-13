import { test, expect } from '@playwright/test';

for (const uiMode of ['lite','pro']) {
  test(`global rescue remains available in ${uiMode} without a kernel or readable config`, async ({page}) => {
    await page.goto(`/?panel=settings&section=network&mode=network-rescue&uiMode=${uiMode}&config-unavailable=1&status-unavailable=1`);
    const rescue=page.getByRole('region',{name:'网络救援'});
    await expect(rescue).toContainText('待确认');
    await rescue.getByRole('button',{name:'处理系统代理',exact:true}).click();
    const dialog=page.getByRole('dialog');
    await expect(dialog).toHaveCount(1);
    await expect(dialog.getByRole('button',{name:/强制接管/})).toBeDisabled();
    await dialog.getByRole('button',{name:/清除系统代理/}).click();
    await expect(dialog.getByRole('button',{name:/清除系统代理/})).toBeDisabled();
    await page.evaluate(()=>window.dispatchEvent(new Event('fixture-finish-reset')));
    await expect(dialog).toHaveCount(0);
    await expect(rescue).toContainText('已关闭');
    await expect(rescue.getByRole('button',{name:'处理系统代理',exact:true})).toBeEnabled();
  });
}

test('global rescue shows reset failure and allows retry without leaving settings', async ({page}) => {
  await page.goto('/?panel=settings&section=network&mode=network-rescue&uiMode=lite&reset-failure=1');
  await page.getByRole('region',{name:'网络救援'}).getByRole('button',{name:'处理系统代理',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await dialog.getByRole('button',{name:/清除系统代理/}).click();
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-finish-reset')));
  await expect(dialog.getByRole('alert')).toHaveText('系统拒绝修改代理设置');
  await expect(dialog.getByRole('button',{name:/清除系统代理/})).toBeEnabled();
  await dialog.getByRole('button',{name:'取消',exact:true}).click();
  await expect(page.getByRole('region',{name:'网络救援'})).toContainText('未接管');
});
