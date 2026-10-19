import { expect, test } from '@playwright/test';
for (const mode of ['unified-lite','unified-pro']) {
  test(`${mode}: shared sources have one configuration, unsynced sources remain actionable`, async ({page}) => {
    await page.goto(`/?panel=profiles&mode=${mode}`);
    await expect(page.getByRole('heading',{name:'配置管理'})).toBeVisible();
    await expect(page.getByRole('article',{name:'配置 工作配置',exact:true})).toHaveCount(1);
    const shared=page.getByRole('article',{name:'配置 工作配置',exact:true});
    await expect(shared.getByText('来源一',{exact:true})).toBeVisible();
    await expect(shared.getByText('来源二',{exact:true})).toBeVisible();
    await expect(page.getByRole('article',{name:'配置 待同步来源',exact:true})).toContainText('待更新');
    await page.getByRole('radio',{name:'订阅',exact:true}).click();
    await expect(page.getByRole('article')).toHaveCount(2);
    await page.getByRole('textbox',{name:'搜索配置'}).fill('来源二');
    await expect(page.getByRole('article')).toHaveCount(1);
  });
}
test('editing a source preserves its target and preferences, without association controls',async({page})=>{
  await page.goto('/?panel=profiles&mode=unified');
  await page.getByRole('region',{name:'更新来源 来源一',exact:true}).getByRole('button',{name:'编辑来源'}).click();
  const dialog=page.getByRole('dialog');
  await expect(dialog.getByLabel('名称',{exact:true})).toHaveValue('来源一');
  await expect(dialog.getByLabel('User-Agent',{exact:true})).toHaveValue('Custom-UA');
  await expect(dialog.getByText('关联配置',{exact:true})).toHaveCount(0);
  await dialog.getByLabel('名称',{exact:true}).fill('新来源名称');
  await dialog.getByRole('button',{name:'保存来源',exact:true}).click();
  const saved=page.getByLabel('保存结果');
  await expect(saved).toContainText('"targetProxyConfigId":"work"');
  await expect(saved).toContainText('"updateIntervalSecs":3600');
  await expect(page.getByRole('article',{name:'配置 工作配置',exact:true})).toContainText('新来源名称');
});
test('shared configuration cannot be deleted along with one of its sources',async({page})=>{
  await page.goto('/?panel=profiles&mode=unified');
  await page.getByRole('region',{name:'更新来源 来源一',exact:true}).getByRole('button',{name:'移除来源'}).click();
  const dialog=page.getByRole('dialog');
  await expect(dialog.getByRole('checkbox')).toBeDisabled();
  await dialog.getByRole('button',{name:'确认移除'}).click();
  await expect(page.getByRole('article',{name:'配置 工作配置',exact:true})).toContainText('来源二');
  await expect(page.getByRole('region',{name:'更新来源 来源一',exact:true})).toHaveCount(0);
  await expect(page.getByLabel('保存结果')).toContainText('"removeConfig":false');
});
test('failed initial update leaves one saved source and cannot create duplicates by resubmission',async({page})=>{
  await page.goto('/?panel=profiles&mode=unified');
  await page.getByRole('button',{name:'添加配置',exact:true}).click();
  await page.getByRole('button',{name:'URL 订阅',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await dialog.getByLabel('名称',{exact:true}).fill('新增来源');
  await dialog.getByLabel('订阅地址',{exact:true}).fill('https://new.test/sub');
  await dialog.getByRole('button',{name:'保存并更新'}).click();
  await expect(dialog.getByRole('alert')).toContainText('来源已保存');
  await expect(dialog.getByRole('button',{name:'保存并更新'})).toBeDisabled();
  await dialog.getByRole('button',{name:'关闭',exact:true}).first().click();
  await expect(page.getByRole('article',{name:'配置 新增来源',exact:true})).toHaveCount(1);
});
