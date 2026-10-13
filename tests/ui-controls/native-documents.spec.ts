import { expect, test } from '@playwright/test';
test('native file read places ordinary content in the draft without a desktop path', async ({page}) => {
  await page.setViewportSize({width:320,height:740});
  await page.goto('/?panel=native-documents&document=read');
  await page.getByRole('button',{name:'新建',exact:true}).click();
  await page.getByRole('button',{name:'选择',exact:true}).click();
  await expect(page.getByPlaceholder('粘贴代理配置 JSON...')).toHaveValue('{"outbounds":[],"route":{"final":"direct"}}');
  await expect(page.getByText('文件已读取，请校验后保存',{exact:true})).toBeVisible();
  await expect(page.getByRole('button',{name:'保存配置',exact:true})).toBeDisabled();
});
for (const mode of ['cancelled','denied']) {
  test(`native ${mode} preserves the current editor and does not save`, async({page}) => {
    await page.goto(`/?panel=native-documents&document=${mode}`);
    await page.getByRole('button',{name:'新建',exact:true}).click();
    await page.getByRole('button',{name:'选择',exact:true}).click();
    await expect(page.getByPlaceholder('请选择本地配置文件')).toHaveValue('');
    await expect(page.getByRole('button',{name:'导入并保存',exact:true})).toBeDisabled();
    await expect(page.getByText(mode==='cancelled'?'已取消，原有草稿和数据保留':'系统权限拒绝；草稿保留',{exact:true})).toBeVisible();
  });
}
test('late native document does not overwrite a changed draft',async({page}) => {
  await page.goto('/?panel=native-documents&document=delayed');
  await page.getByRole('button',{name:'新建',exact:true}).click();
  await page.getByRole('button',{name:'选择',exact:true}).click();
  await page.getByRole('radio',{name:'粘贴 JSON',exact:true}).click();
  const content=page.getByPlaceholder('粘贴代理配置 JSON...');
  await content.fill('{"draft":"keep"}');
  await expect(page.getByText('草稿已变化，文件结果未写入；请保留修改后重新选择文件。',{exact:true})).toBeVisible();
  await expect(content).toHaveValue('{"draft":"keep"}');
});
