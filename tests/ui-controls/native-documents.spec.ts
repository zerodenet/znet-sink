import { expect, test } from '@playwright/test';
test('native file read places ordinary content in the draft without a desktop path', async ({page}) => {
  await page.setViewportSize({width:320,height:740});
  await page.goto('/?panel=native-documents&document=read');
  await page.getByRole('button',{name:'新建',exact:true}).click();
  await page.getByRole('button',{name:'选择配置文件',exact:true}).click();
  await expect(page.getByPlaceholder('粘贴代理配置 JSON...')).toHaveValue('{"outbounds":[],"route":{"final":"direct"}}');
  await expect(page.getByText('文件原文已读取；请校验后保存，尚未应用。',{exact:true})).toBeVisible();
  await expect(page.getByRole('button',{name:'仅保存本地配置',exact:true})).toBeDisabled();
  expect(await page.evaluate(()=> (window as any).profileEditorFixture.calls.filter((item:any)=>item.kind==='save').length)).toBe(0);
});
for (const mode of ['cancelled','denied']) {
  test(`native ${mode} preserves the current editor and does not save`, async({page}) => {
    await page.goto(`/?panel=native-documents&document=${mode}`);
    await page.getByRole('button',{name:'新建',exact:true}).click();
    await page.getByRole('button',{name:'选择配置文件',exact:true}).click();
    await expect(page.getByPlaceholder('粘贴代理配置 JSON...')).toHaveValue('{}');
    await expect(page.getByRole('button',{name:'仅保存本地配置',exact:true})).toBeDisabled();
  expect(await page.evaluate(()=> (window as any).profileEditorFixture.calls.filter((item:any)=>item.kind==='save').length)).toBe(0);
    await expect(page.getByText(mode==='cancelled'?'已取消文件选择，草稿保留。':'系统文件操作被拒绝或失败，原草稿保留。',{exact:true})).toBeVisible();
  });
}
test('late native document does not overwrite a changed draft',async({page}) => {
  await page.goto('/?panel=native-documents&document=delayed');
  await page.getByRole('button',{name:'新建',exact:true}).click();
  await page.getByRole('button',{name:'选择配置文件',exact:true}).click();
  const content=page.getByPlaceholder('粘贴代理配置 JSON...');
  await content.fill('{"draft":"keep"}');
  await expect(page.getByText('草稿已变化，文件结果未写入；请保留修改后重新选择文件。',{exact:true})).toBeVisible();
  await expect(content).toHaveValue('{"draft":"keep"}');
  expect(await page.evaluate(()=> (window as any).profileEditorFixture.calls.filter((item:any)=>item.kind==='save').length)).toBe(0);
});
