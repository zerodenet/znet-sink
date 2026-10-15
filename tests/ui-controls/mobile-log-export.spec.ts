import {test,expect,type Page} from '@playwright/test';
async function open(page:Page,status='cancelled'){
 await page.setViewportSize({width:320,height:740});
 await page.addInitScript((status)=>{
  const win=window as unknown as {__logExportCalls:{command:string;args:Record<string,unknown>}[];__TAURI_INTERNALS__:{invoke:(command:string,args:Record<string,unknown>)=>Promise<unknown>}};win.__logExportCalls=[];
  win.__TAURI_INTERNALS__={invoke:async(command,args)=>{win.__logExportCalls.push({command,args});if(command==='log_export_preview'){const request=args.request as {items:unknown[]};return {planId:'original-selected-plan',createdAtUnixMs:String(Date.now()),expiresAtUnixMs:String(Date.now()+300000),bytes:512,count:request.items.length,classifiedCount:1,unclassifiedCount:request.items.length-1,partial:true,scope:{profileId:'p',configRevision:'1',clientInstanceId:'2'}};}if(command==='log_export_discard')return; if(command==='log_export_deliver'){if(status==='copied')return {status:'copied',document:null};if(status==='stale')throw {code:'conflict',message:'配置或实例已变化，请重新预览'};return {status:'system-receipt',document:{operationId:'original-system',status,content:null,bytesRead:null,reason:null}};}throw new Error('unexpected RPC');}};
 },status);
 await page.goto('/mobile-log-export.html');await expect(page.locator('.log-row').first()).toBeVisible();
}
test('current visible selection preview contains only identities; cancelled system save preserves search/list',async({page})=>{
 await open(page);const search=page.getByRole('textbox',{name:'搜索已加载日志'});await search.fill('日志 4800 ');await expect(page.locator('.log-row')).toHaveCount(1);
 await page.getByRole('button',{name:'脱敏复制 / 导出',exact:true}).click();await expect(page.getByRole('article',{name:'固定脱敏日志预览'})).toContainText('1 条日志');await expect(page.getByRole('dialog')).toContainText('原日志正文');
 await page.getByRole('button',{name:'保存原快照',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'已取消'})).toBeVisible();await expect(page.getByRole('button',{name:'保存原快照'})).toBeDisabled();
 await page.getByRole('button',{name:'返回日志'}).click();await expect(search).toHaveValue('日志 4800 ');await expect(page.locator('.log-row')).toHaveCount(1);
 const calls=await page.evaluate(()=>(window as unknown as {__logExportCalls:{command:string;args:Record<string,unknown>}[]}).__logExportCalls);const request=calls.find(c=>c.command==='log_export_preview')?.args.request as {items:{id:string;occurredAtUnixMs:string}[];searchApplied:boolean};expect(request.items).toHaveLength(1);expect(request.items[0].id).toBe('4800');expect(request.searchApplied).toBe(true);expect(JSON.stringify(request)).not.toContain('日志');expect(calls.filter(c=>c.command==='log_export_deliver')).toEqual([{command:'log_export_deliver',args:{planId:'original-selected-plan',delivery:'save'}}]);
});
test('unknown result cannot replay; stale scope error preserves list and original selection',async({page})=>{
 for(const status of ['result-unknown','stale']){await open(page,status);await page.getByRole('button',{name:'脱敏复制 / 导出',exact:true}).click();await expect(page.getByRole('article',{name:'固定脱敏日志预览'})).toBeVisible();await page.getByRole('button',{name:'复制原脱敏快照'}).click();await expect(page.getByRole('dialog')).toContainText(status==='stale'?'实例已变化':'结果未确认');await expect(page.getByRole('button',{name:'复制原脱敏快照'})).toBeDisabled();await expect(page.getByRole('button',{name:'保存原快照'})).toBeDisabled();}
});
test('system share uses fixed plan and every delivery control has a44px touch target',async({page})=>{
 await open(page,'share-accepted');await page.getByRole('button',{name:'脱敏复制 / 导出',exact:true}).click();for(const name of ['返回日志','复制原脱敏快照','系统分享原快照','保存原快照'])expect((await page.getByRole('button',{name,exact:true}).boundingBox())?.height).toBeGreaterThanOrEqual(44);await page.getByRole('button',{name:'系统分享原快照'}).click();await expect(page.getByRole('status').filter({hasText:'接收方保存结果未确认'})).toBeVisible();
});

test('single log copy uses the original host plan and dismissed previews release only that plan',async({page})=>{
 await open(page,'copied');const single=page.getByRole('button',{name:'复制日志 #5000',exact:true});await single.click();await expect(page.getByRole('article',{name:'固定脱敏日志预览'})).toContainText('1 条日志');await page.getByRole('button',{name:'返回日志'}).click();await single.click();await page.getByRole('button',{name:'复制原脱敏快照'}).click();await expect(page.getByRole('dialog')).toContainText('脱敏日志已复制');
 const calls=await page.evaluate(()=>(window as unknown as {__logExportCalls:{command:string;args:Record<string,unknown>}[]}).__logExportCalls);const preview=calls.filter(c=>c.command==='log_export_preview');expect(preview).toHaveLength(2);expect((preview[0].args.request as {items:unknown[]}).items).toHaveLength(1);expect(calls.some(c=>c.command==='log_export_discard')).toBe(true);expect(calls.filter(c=>c.command==='log_export_deliver')).toEqual([{command:'log_export_deliver',args:{planId:'original-selected-plan',delivery:'copy'}}]);expect(calls.some(c=>c.command==='platform_clipboard_write')).toBe(false);
});
