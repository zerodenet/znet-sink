import { test, expect } from '@playwright/test';
test('node page stops a backend-owned job and prevents a duplicate stop click', async ({ page }) => {
  test.setTimeout(90000);
  await page.addInitScript(() => {
    let job: Record<string, unknown>;
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string, args: any) => {
      if (command === 'gui_probe_job_start') {
        job = {id:1,scope:{profileId:'fixture',configRevision:1,coreInstanceId:1},kind:args.request.kind,state:'running',targetTags:args.request.targetTags,results:[],completed:0,succeeded:0,failed:0,startedAtUnixMs:1,updatedAtUnixMs:1,deadlineAtUnixMs:60000};
        return job;
      }
      if (command === 'gui_probe_job_cancel') {
        window.dispatchEvent(new CustomEvent('fixture-save',{detail:{jobId:args.jobId}}));
        await new Promise(resolve=>setTimeout(resolve,200));
        return {...job,state:'cancelled',updatedAtUnixMs:2};
      }
      throw new Error(`Unexpected command: ${command}`);
    }}});
  });
  await page.goto('/?panel=nodes', {waitUntil:'domcontentloaded', timeout:60000});
  await page.getByRole('button',{name:'测速',exact:true}).click();
  const stop = page.getByRole('button',{name:'停止测速',exact:true});
  await expect(stop).toBeVisible();
  await expect(stop).toHaveAttribute('title', /已发送请求等待内核返回或超时/);
  await stop.click();
  await expect(page.getByRole('button',{name:'正在停止…'})).toBeDisabled();
  await expect(page.getByLabel('保存结果')).toContainText('"jobId":1');
  await expect(stop).toHaveCount(0);
  await expect(page.getByRole('button',{name:'测速',exact:true})).toBeEnabled();
});
