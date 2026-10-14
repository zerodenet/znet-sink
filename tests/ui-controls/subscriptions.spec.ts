import { expect, test } from '@playwright/test';

test('managed subscription card shows aggregate quota, ratio, expiry and ownership', async ({ page }) => {
  await page.goto('/?panel=subscriptions&mode=managed-usage');

  await expect(page.getByText('狗梯 / Developer Licenses', { exact: true })).toBeVisible();
  await expect(page.getByText('由 Connect 托管', { exact: true })).toBeVisible();
  await expect(page.getByText('来源: 狗梯', { exact: true })).toBeVisible();
  await expect(page.getByText(/已用 375 B \/ 总量 1000 B/)).toBeVisible();
  await expect(page.getByText('37.5%', { exact: true })).toBeVisible();
  await expect(page.getByText(/到期:/)).toBeVisible();
  await expect(page.getByText(/剩 \d+ 天/)).toBeVisible();
  await expect(page.getByRole('button', { name: '由来源插件刷新' })).toBeDisabled();
});

test('mounted subscription page refreshes the sync time on background completion', async ({ page }) => {
  await page.goto('/?panel=subscriptions&mode=managed-usage');
  const syncTime = page.getByText(/^最近成功检查:/);
  await expect(syncTime).toBeVisible();
  const before = await syncTime.textContent();
  await page.evaluate(() => {
    window.dispatchEvent(new CustomEvent('fixture-subscription-synced', { detail: 1790568600000 }));
    window.dispatchEvent(new CustomEvent('subscriptions:updated', { detail: { subscriptionId:'managed-connect' } }));
  });
  await expect(syncTime).not.toHaveText(before!);
  await expect(page.getByText(/已用 375 B \/ 总量 1000 B/)).toBeVisible();
});

test('320px subscription quota stays unknown for missing and unsafe usage, while real zero stays zero',async({page})=>{
 await page.setViewportSize({width:320,height:800});await page.goto('/?panel=mobile-configuration&mode=quota-facts');await page.getByRole('radio',{name:'订阅',exact:true}).click();
 const row=(name:string)=>page.locator('.list-row').filter({has:page.getByText(name,{exact:true})});
 for(const name of ['额度已提供但用量未知','只有上传已采样','数值无法确认']){const card=row(name);await expect(card).toContainText('来源已用 — / 总量 100 B');await expect(card.locator('.traffic-bar-track')).toHaveCount(0);await expect(card.locator('.traffic-percent')).toHaveCount(0);}
 await expect(row('只有上传已采样')).toContainText('↑10 B ↓—');await expect(row('明确的零用量').locator('.traffic-percent')).toHaveText('0.0%');
 await expect(row('数值无法确认')).toContainText('来源数值无法确认');await expect(page.getByText('private-canary',{exact:false})).toHaveCount(0);
 await expect(row('自动暂停仍可手动')).toContainText('自动同步已暂停');await expect(row('自动暂停仍可手动').getByRole('button',{name:'同步订阅',exact:true})).toBeEnabled();
 await expect(row('额度已提供但用量未知')).toContainText('额度采样时间未提供');expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});

test('explicit manual synchronization remains available when automatic participation is paused',async({page})=>{
 await page.addInitScript(()=>{(window as unknown as {manualSyncIds:string[]}).manualSyncIds=[];window.addEventListener('fixture-subscription-manual-sync',event=>(window as unknown as {manualSyncIds:string[]}).manualSyncIds.push((event as CustomEvent<string>).detail));});
 await page.goto('/?panel=subscriptions&mode=quota-facts');const paused=page.locator('.list-row').filter({has:page.getByText('自动暂停仍可手动',{exact:true})});
 await paused.getByRole('button',{name:'同步订阅',exact:true}).click();await expect.poll(()=>page.evaluate(()=>(window as unknown as {manualSyncIds:string[]}).manualSyncIds)).toEqual(['paused']);
 await expect(paused.getByRole('button',{name:'同步订阅',exact:true})).toBeEnabled();
});

test('cache recovery confirms only original source metadata, without synchronization or clearing unknown results',async({page})=>{
 await page.addInitScript(()=>{(window as unknown as {cacheResets:unknown[];manualSyncIds:string[]}).cacheResets=[];(window as unknown as {manualSyncIds:string[]}).manualSyncIds=[];window.addEventListener('fixture-subscription-cache-reset',event=>(window as unknown as {cacheResets:unknown[]}).cacheResets.push((event as CustomEvent).detail));window.addEventListener('fixture-subscription-manual-sync',event=>(window as unknown as {manualSyncIds:string[]}).manualSyncIds.push((event as CustomEvent).detail));});
 await page.goto('/?panel=subscriptions&mode=quota-facts');await page.getByRole('button',{name:'重置条件请求记录',exact:true}).click();
 await expect(page.getByRole('dialog')).toContainText('保留配置、覆盖、同步错误和未知结果');await expect(page.getByRole('dialog')).toContainText('条件记录损坏');
 await expect.poll(()=>page.evaluate(()=>(window as unknown as {cacheResets:unknown[]}).cacheResets)).toEqual([]);
 await page.getByRole('button',{name:'清理原订阅条件记录',exact:true}).click();await expect(page.getByRole('dialog').getByRole('status')).toContainText('该订阅条件记录已清理，配置材料保留。');
 await expect(page.getByRole('button',{name:'清理原订阅条件记录',exact:true})).toBeDisabled();
 const calls=await page.evaluate(()=>{const win=window as unknown as {cacheResets:unknown[];manualSyncIds:string[]};return {cacheResets:win.cacheResets,manualSyncIds:win.manualSyncIds};});expect(calls.cacheResets).toEqual([{id:'cache',version:1}]);expect(calls.manualSyncIds).toEqual([]);
});
