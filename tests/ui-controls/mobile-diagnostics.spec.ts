import {test,expect,type Page} from '@playwright/test';
async function open(page:Page){await page.setViewportSize({width:320,height:740});await page.addInitScript(()=>{
 const win=window as unknown as {__diagnosticCalls:{command:string;args:Record<string,unknown>}[];__TAURI_INTERNALS__:{invoke:(command:string,args:Record<string,unknown>)=>Promise<unknown>}};win.__diagnosticCalls=[];
 let status={detailed:false,expiresAtMs:0,sessionId:null as string|null,startedAtMs:0,endedAtMs:null,durationSeconds:0,maximumBytes:0,storedBytes:0,storedFrames:0,state:'idle',endReason:null};
 win.__TAURI_INTERNALS__={invoke:async(command,args)=>{win.__diagnosticCalls.push({command,args});
  if(command==='native_diagnostic_capture'){const request=args?.request as {action:string;expectedSessionId?:string;durationSeconds?:number;capacityMiB?:number}|undefined;
   if(request?.action==='start')status={...status,detailed:true,sessionId:'session-original',state:'capturing',durationSeconds:request.durationSeconds!,maximumBytes:request.capacityMiB!*1024*1024};
   if(request?.action==='stop')status={...status,detailed:false,state:'stopped'};return structuredClone(status);
  }
  if(command==='native_document_preview_diagnostics')return {planId:'fixed-host-snapshot',createdAtUnixMs:String(Date.now()),expiresAtUnixMs:String(Date.now()+300000),bytes:256,scope:{profileId:'original-profile',configRevision:'42',clientInstanceId:'7'},logCount:2,frameCount:3,taskCount:1,omissions:[{category:'rawMessages',count:2,reason:'不含自由日志秘密'}]};
  if(command==='native_document_export_diagnostics')return {operationId:'system-original',status:'result-unknown',content:null,bytesRead:null,reason:'lost receipt'};
  throw new Error('unsupported command');
 }};
 });await page.goto('/mobile-diagnostics.html');await expect(page.getByRole('status').filter({hasText:'尚未捕获'})).toBeVisible();}
test('invalid budget never dispatches; fixed session stop follows actual capture receipt',async({page})=>{
 await open(page);await page.getByLabel('捕获时长（秒）').fill('301');await page.getByRole('button',{name:'开始有界捕获'}).click();await expect(page.getByRole('alert')).toContainText('1–300');
 await page.getByLabel('捕获时长（秒）').fill('3');await page.getByLabel('捕获容量（MiB）').fill('2');await page.getByRole('button',{name:'开始有界捕获'}).click();await expect(page.getByRole('status').filter({hasText:'正在捕获'})).toBeVisible();await page.getByRole('button',{name:'停止原捕获会话'}).click();
 const calls=await page.evaluate(()=> (window as unknown as {__diagnosticCalls:{command:string;args:Record<string,unknown>}[]}).__diagnosticCalls.filter(c=>c.args?.request));
 expect(calls.map(c=>c.args.request)).toEqual([{action:'start',durationSeconds:3,capacityMiB:2},{action:'stop',expectedSessionId:'session-original'}]);
});
test('preview uses original host snapshot and unknown delivery disables replay without claiming saved',async({page})=>{
 await open(page);await page.getByRole('button',{name:'预览脱敏诊断材料'}).click();await expect(page.getByRole('article',{name:'固定诊断快照'})).toContainText('original-profile');
 await page.getByRole('button',{name:'保存原快照'}).click();await expect(page.getByRole('status').filter({hasText:'系统操作结果未确认'})).toBeVisible();await expect(page.getByRole('button',{name:'保存原快照'})).toBeDisabled();
 const calls=await page.evaluate(()=> (window as unknown as {__diagnosticCalls:{command:string;args:Record<string,unknown>}[]}).__diagnosticCalls.filter(c=>c.command==='native_document_export_diagnostics'));
 expect(calls).toHaveLength(1);expect(calls[0].args).toEqual({planId:'fixed-host-snapshot',delivery:'save'});
});
