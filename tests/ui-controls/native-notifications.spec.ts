import { test, expect, type Page } from '@playwright/test';
async function open(page:Page,fail=false){
  await page.setViewportSize({width:320,height:740});
  await page.addInitScript(({fail})=>{
    const w=window as unknown as {__noticeCalls:{command:string,args?:Record<string,unknown>}[];__TAURI_INTERNALS__:{invoke:(command:string,args?:Record<string,unknown>)=>Promise<unknown>}};
    w.__noticeCalls=[];
    w.__TAURI_INTERNALS__={invoke:async(command,args)=>{
      w.__noticeCalls.push({command,args});
      if(command==='mobile_notification_preference'&&fail)throw new Error('保存结果未确认，请重新核对');
      return {enabled:command==='mobile_notification_preference',expectedRevision:'a'.repeat(64),permission:command==='mobile_notification_request'?'granted':'prompt',reason:null,presentation:'unconfirmed'};
    }};
  },{fail});
  await page.goto('/native-notifications.html');
  await expect(page.getByText(/应用偏好：关闭；系统授权：未请求授权/)).toBeVisible();
}
test('viewing and saving optional notification intent never requests permission',async({page})=>{
  await open(page);
  expect(await page.evaluate(()=>(window as unknown as {__noticeCalls:{command:string}[]}).__noticeCalls.map(x=>x.command))).toEqual(['mobile_notification_status']);
  await page.getByRole('button',{name:'保存可选通知偏好为开启'}).click();
  await expect(page.getByText(/应用偏好：开启；系统授权：未请求授权/)).toBeVisible();
  expect(await page.evaluate(()=>(window as unknown as {__noticeCalls:{command:string}[]}).__noticeCalls.map(x=>x.command))).toEqual(['mobile_notification_status','mobile_notification_preference']);
  for(const button of await page.getByRole('button').all())expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);
});
test('only explicit permission action requests it and unknown preference outcomes do not replay',async({page})=>{
  await open(page,true);
  await page.getByRole('button',{name:'保存可选通知偏好为开启'}).click();
  await expect(page.getByRole('status')).toHaveText('保存结果未确认，请重新核对');
  await expect(page.getByRole('button',{name:'保存可选通知偏好为开启'})).toBeDisabled();
  await page.getByRole('button',{name:'请求系统通知授权'}).click();
  await expect(page.getByText(/应用偏好：关闭；系统授权：已授权/)).toBeVisible();
  await expect(page.getByRole('button',{name:'请求系统通知授权'})).toBeDisabled();
  const calls=await page.evaluate(()=>(window as unknown as {__noticeCalls:{command:string}[]}).__noticeCalls.map(x=>x.command));
  expect(calls).toEqual(['mobile_notification_status','mobile_notification_preference','mobile_notification_request']);
});
