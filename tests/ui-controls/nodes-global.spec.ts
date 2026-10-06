import {test,expect} from '@playwright/test';
const card = (page:import('@playwright/test').Page,tag:string) => page.locator('.grid-card').filter({has:page.locator('.grid-card-name').getByText(tag,{exact:true})});

test('special outbound cards retain distinct tags in grid, list and global selection',async({page})=>{
  await page.goto('/?panel=nodes&layout=global&special=1');
  await page.locator('.group-sidebar').getByRole('button',{name:/全部节点/}).click();
  const names = ['direct','block','DIRECT','REJECT','node-a','node-b','wg-independent'];
  await expect(page.locator('.grid-card-name')).toHaveText(names);
  await expect(card(page,'direct').locator('.proto-label')).toHaveText('direct');
  await expect(card(page,'DIRECT').locator('.proto-label')).toHaveText('direct');
  await expect(card(page,'block').locator('.proto-label')).toHaveText('block');
  await expect(card(page,'REJECT').locator('.proto-label')).toHaveText('block');
  await page.getByRole('radio',{name:'列表视图'}).click();
  await expect(page.locator('.node-name')).toHaveText(names);
  await page.evaluate(()=>window.dispatchEvent(new CustomEvent('fixture-mode-changed',{detail:'global'})));
  const reject = page.locator('.node-main').filter({has:page.locator('.node-name').getByText('REJECT',{exact:true})});
  await reject.click();
  await expect(page.getByLabel('保存结果')).toContainText('"globalOutbound":"REJECT"');
  await expect(page.locator('.node-row.active .node-name')).toHaveText('REJECT');
});

test('global mode flattens all targets, distinguishes groups and blocks invalid references',async({page})=>{
  await page.goto('/?panel=nodes&layout=global&route=global');
  await expect(page.locator('.node-title')).toHaveText('全局出口');
  await expect(page.locator('.grid-card-name')).toHaveText(['node-a','node-b','wg-independent','proxy','ai','auto','standalone','bad-a','bad-b','empty','broken']);
  await expect(page.locator('.section-header')).toHaveCount(0);
  await expect(card(page,'node-a')).toHaveClass(/active/);
  await expect(card(page,'proxy')).toBeDisabled();
  await expect(card(page,'proxy').locator('.proto-label')).toHaveText('Selector');
  await expect(card(page,'auto')).toBeEnabled();
  await expect(card(page,'standalone')).toBeEnabled();
  await expect(card(page,'bad-a')).toBeDisabled();
  await expect(card(page,'bad-a')).toHaveAttribute('title',/循环引用/);
  await expect(card(page,'empty')).toBeDisabled();
  await expect(card(page,'broken')).toBeDisabled();
  await expect(card(page,'wg-independent')).toBeEnabled();
  await card(page,'node-b').click();
  await expect(page.getByLabel('保存结果')).toContainText('"globalOutbound":"node-b"');
  await expect(page.getByLabel('保存结果')).not.toContainText('"policy"');
  await expect(card(page,'node-b')).toHaveClass(/active/);
  await expect(page.locator('.group-item').filter({has:page.locator('.group-name').getByText('proxy',{exact:true})}).locator('.group-selected-name')).toHaveText('auto');
  await page.screenshot({path:test.info().outputPath('global-targets.png'),fullPage:true});
});

test('global automatic group selection is a mode target and list view shows the same active target',async({page})=>{
  await page.goto('/?panel=nodes&layout=global&route=global');
  await card(page,'standalone').click();
  await expect(page.getByLabel('保存结果')).toContainText('"globalOutbound":"standalone"');
  await expect(card(page,'standalone')).toHaveClass(/active/);
  await page.getByRole('radio',{name:'列表视图'}).click();
  await expect(page.locator('.node-row.active')).toContainText('standalone');
  const auto=page.locator('.node-main').filter({has:page.locator('.node-name').getByText('auto',{exact:true})});
  await auto.click();
  await expect(page.getByLabel('保存结果')).toContainText('"globalOutbound":"auto"');
  await expect(page.locator('.node-row.active')).toContainText('auto');
});

test('rule inventory is flat and readonly, explicit selectors alone receive member changes',async({page})=>{
  await page.goto('/?panel=nodes&layout=global');
  await page.locator('.group-sidebar').getByRole('button',{name:/全部节点/}).click();
  await expect(page.locator('.grid-card-name')).toHaveText(['node-a','node-b','wg-independent']);
  await expect(page.locator('.grid-card.active')).toHaveCount(0);
  await expect(card(page,'node-a')).toBeDisabled();
  await expect(page.getByLabel('保存结果')).toBeEmpty();
  await page.locator('.group-item').filter({has:page.locator('.group-name').getByText('ai',{exact:true})}).click();
  await card(page,'node-a').click();
  await expect(page.getByLabel('保存结果')).toContainText('"policy":"ai"');
  await expect(page.getByLabel('保存结果')).not.toContainText('"globalOutbound"');
  await page.evaluate(()=>window.dispatchEvent(new CustomEvent('fixture-mode-changed',{detail:'global'})));
  await expect(page.locator('.node-title')).toHaveText('全局出口');
  await expect(page.locator('.grid-card-name')).toHaveCount(11);
  await page.evaluate(()=>window.dispatchEvent(new CustomEvent('fixture-mode-changed',{detail:'rule'})));
  await expect(page.locator('.node-title')).toHaveText('proxy');
  await expect(page.locator('.grid-card-name')).toHaveText(['auto','node-a','node-b']);
});

test('rejected global target keeps the original highlight and reports failure',async({page})=>{
  await page.goto('/?panel=nodes&layout=global&route=global&global-failure=1');
  await card(page,'node-b').click();
  await expect(page.locator('.node-action-error')).toContainText('全局出口应用失败');
  await expect(card(page,'node-a')).toHaveClass(/active/);
  await expect(card(page,'node-b')).not.toHaveClass(/active/);
  await expect(page.getByLabel('保存结果')).toBeEmpty();
});
