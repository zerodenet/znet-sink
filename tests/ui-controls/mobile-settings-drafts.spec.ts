import { test, expect, type Page } from '@playwright/test';
test.use({ viewport: { width: 320, height: 720 } });
const cases = [
  { entry: 'VPN 与接管', kind: 'tun', field: 'TUN 入站标签', value: 'draft-tun', original: 'tun' },
  { entry: 'DNS', kind: 'dns', field: '', value: '', original: '' },
  { entry: '运行网络', kind: 'network', field: 'UDP 上游空闲时间', value: '45', original: '30' },
  { entry: '节点延迟测试', kind: 'probes', field: '公共测速地址', value: 'https://draft.example/204', original: 'http://www.gstatic.com/generate_204' },
] as const;
type Form = typeof cases[number];
async function openSettings(page: Page) {
  await page.goto('/mobile.html?fixture=settings-drafts');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '更多', exact: true }).click();
  await page.getByRole('button', { name: '设置', exact: true }).click();
}
async function openForm(page: Page, form: Form) {
  await page.getByRole('button', { name: form.entry, exact: true }).click();
  if (form.kind === 'dns') await expect(page.getByRole('radio', { name: 'Fake-IP', exact: true })).toBeVisible();
  else await expect(page.getByRole('textbox', { name: form.field, exact: true })).toHaveValue(form.original);
}
async function edit(page: Page, form: Form) {
  if (form.kind === 'dns') await page.getByRole('radio', { name: 'Fake-IP', exact: true }).click();
  else await page.getByRole('textbox', { name: form.field, exact: true }).fill(form.value);
}
async function retained(page: Page, form: Form) {
  if (form.kind === 'dns') await expect(page.getByRole('radio', { name: 'Fake-IP', exact: true })).toHaveAttribute('aria-checked', 'true');
  else await expect(page.getByRole('textbox', { name: form.field, exact: true })).toHaveValue(form.value);
}
const nativeBack = (page: Page) => page.evaluate(() => window.dispatchEvent(new CustomEvent('mobile:back', { cancelable: true })));
async function controls(page: Page, action: 'hold' | 'fail' | 'release' | 'source') {
  await page.evaluate(action => {
    const fixture = (window as unknown as { settingsDraftFixture: { holdWrites: boolean; failWrites: boolean; releaseWrite: () => void; replaceSource: () => void } }).settingsDraftFixture;
    if (action === 'hold') fixture.holdWrites = true;
    if (action === 'fail') fixture.failWrites = true;
    if (action === 'release') fixture.releaseWrite();
    if (action === 'source') fixture.replaceSource();
  }, action);
}
const writes = (page: Page) => page.evaluate(() => (window as unknown as { settingsDraftFixture: { writes: number } }).settingsDraftFixture.writes);
async function save(page:Page,form:Form){
  if(form.kind==='network'){
    await page.getByRole('button',{name:'预览本地网络设置',exact:true}).click();
    await page.getByRole('dialog').getByRole('button',{name:'确认仅保存本地',exact:true}).click();
  }else await page.getByRole('button',{name:form.kind==='dns'||form.kind==='tun'?'保存并应用':'应用',exact:true}).click();
}
for (const form of cases) {
  test(`${form.entry}: native return and bottom tab require explicit discard and cancellation keeps the draft`, async ({ page }) => {
    await openSettings(page); await openForm(page, form); await edit(page, form);
    expect(await nativeBack(page)).toBe(false);
    await expect(page.getByRole('heading', { name: '放弃未保存的设置？' })).toBeVisible();
    await page.getByRole('button', { name: '继续编辑', exact: true }).click(); await retained(page, form);
    await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '节点', exact: true }).click();
    await expect(page.getByRole('heading', { name: '放弃未保存的设置？' })).toBeVisible();
    await page.getByRole('button', { name: '继续编辑', exact: true }).click(); await retained(page, form);
    await page.getByRole('button', { name: '返回', exact: true }).click();
    await page.getByRole('button', { name: '放弃修改', exact: true }).click();
    await expect(page.locator('.secondary-pane:not(.inactive) .secondary-header h1')).toHaveText('设置');
    expect(await writes(page)).toBe(0);
    await openForm(page, form); // confirmed local values return, no hidden write
    expect(await nativeBack(page)).toBe(false);
    await expect(page.getByRole('heading', { name: '放弃未保存的设置？' })).toHaveCount(0);
    await openForm(page, form); await edit(page, form);
    const nav = page.getByRole('navigation', { name: '主导航' });
    await nav.getByRole('button', { name: '节点', exact: true }).click();
    await page.getByRole('button', { name: '放弃修改', exact: true }).click();
    await expect(nav.getByRole('button', { name: '节点', exact: true })).toHaveAttribute('aria-current', 'page');
    await nav.getByRole('button', { name: '更多', exact: true }).click();
    if (form.kind === 'dns') await expect(page.getByRole('radio', { name: 'Fake-IP', exact: true })).toHaveAttribute('aria-checked', 'false');
    else await expect(page.getByRole('textbox', { name: form.field, exact: true })).toHaveValue(form.original);
    expect(await writes(page)).toBe(0);
  });
  test(`${form.entry}: submission blocks navigation without cancellation/replay and confirmed offline save clears the guard`, async ({ page }) => {
    await openSettings(page); await openForm(page, form); await edit(page, form); await controls(page, 'hold');
    await save(page, form); await expect.poll(() => writes(page)).toBe(1);
    expect(await nativeBack(page)).toBe(false);
    if(form.kind==='network'){
      await expect(page.getByRole('dialog').getByRole('heading',{name:'确认仅保存本地网络设置'})).toBeVisible();
      await expect(page.getByRole('button',{name:'返回编辑网络设置',exact:true})).toBeDisabled();
      await page.getByRole('navigation',{name:'主导航'}).getByRole('button',{name:'节点',exact:true}).click({force:true});
      await expect(page.getByRole('dialog').getByRole('heading',{name:'确认仅保存本地网络设置'})).toBeVisible();
    }else{
      await expect(page.getByRole('heading', { name: '设置正在提交' })).toBeVisible();
      await expect(page.getByRole('button', { name: '放弃修改', exact: true })).toHaveCount(0);
      await page.getByRole('button', { name: '继续编辑', exact: true }).click();
      await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '节点', exact: true }).click();
      await expect(page.getByRole('heading', { name: '设置正在提交' })).toBeVisible();
    }
    await controls(page, 'release');
    await expect(page.getByRole('heading', { name: '设置正在提交' })).toHaveCount(0);
    await expect(page.locator('.secondary-pane:not(.inactive) .secondary-header h1')).toHaveText(form.entry);
    if(form.kind==='network')await expect(page.getByText('已确认本地网络覆盖，待应用；内核、监听、系统代理与 VPN 未改变。',{exact:true})).toBeVisible();
    else await expect(page.getByRole('button', { name: form.kind === 'dns' ? '已保存，待内核' : '已保存，启动后生效', exact: true })).toBeVisible();
    expect(await writes(page)).toBe(1);
    expect(await nativeBack(page)).toBe(false);
    await expect(page.getByRole('heading', { name: '放弃未保存的设置？' })).toHaveCount(0);
    await expect(page.locator('.secondary-pane:not(.inactive) .secondary-header h1')).toHaveText('设置');
  });
  test(`${form.entry}: failed/unknown save preserves input and guard, never automatically repeats`, async ({ page }) => {
    await openSettings(page); await openForm(page, form); await edit(page, form); await controls(page, 'fail');
    await save(page, form); if(form.kind==='network')await expect(page.getByText('原网络保存或保存后核对未确认；原操作保留，不会重复提交。',{exact:true})).toBeVisible();
    else await expect(page.getByRole('alert').filter({ hasText: '原设置提交结果未确认' })).toBeVisible();
    await retained(page, form); expect(await nativeBack(page)).toBe(false);
    await expect(page.getByRole('heading', { name: '放弃未保存的设置？' })).toBeVisible();
    await page.getByRole('button', { name: '继续编辑', exact: true }).click(); await retained(page, form);
    expect(await writes(page)).toBe(1);
  });
}
test('DNS dirty refresh and browser back keep the draft and original confirmed baseline', async ({ page }) => {
  const form=cases[1]; await openSettings(page); await openForm(page,form); await edit(page,form);
  await controls(page,'source'); await page.getByRole('button',{name:'重新加载 DNS 配置',exact:true}).click();
  await expect(page.getByText('来源已刷新，未覆盖未保存草稿；请继续编辑或明确放弃。')).toBeVisible(); await retained(page,form);
  await page.goBack(); await expect(page.getByRole('heading',{name:'放弃未保存的设置？'})).toBeVisible();
  await page.getByRole('button',{name:'继续编辑',exact:true}).click(); await retained(page,form);
  expect(await writes(page)).toBe(0);
});
test('clean DNS source refresh becomes a new baseline and never reports unsaved edits', async ({ page }) => {
  await openSettings(page); await openForm(page,cases[1]); await controls(page,'source');
  await page.getByRole('button',{name:'重新加载 DNS 配置',exact:true}).click();
  await expect(page.getByRole('radio',{name:'Fake-IP',exact:true})).toHaveAttribute('aria-checked','false');
  expect(await nativeBack(page)).toBe(false);
  await expect(page.getByRole('heading',{name:'放弃未保存的设置？'})).toHaveCount(0);
  await expect(page.locator('.secondary-pane:not(.inactive) .secondary-header h1')).toHaveText('设置');
});
test('inactive mounted forms never own another route and disposing a newer guard retains the older one', async ({ page }) => {
  await openSettings(page); await openForm(page,cases[1]);
  const nav=page.getByRole('navigation',{name:'主导航'});
  await nav.getByRole('button',{name:'概览',exact:true}).click();
  await page.getByRole('button',{name:'打开设置',exact:true}).click(); await openForm(page,cases[0]); await edit(page,cases[0]);
  await page.getByRole('button',{name:'返回',exact:true}).click(); await page.getByRole('button',{name:'放弃修改',exact:true}).click();
  await nav.getByRole('button',{name:'更多',exact:true}).click(); await edit(page,cases[1]);
  expect(await nativeBack(page)).toBe(false);
  await expect(page.getByRole('heading',{name:'放弃未保存的设置？'})).toBeVisible();
  await page.getByRole('button',{name:'继续编辑',exact:true}).click(); await retained(page,cases[1]);
});
