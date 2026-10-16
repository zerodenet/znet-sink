import {test,expect,type Page} from '@playwright/test';
async function open(page:Page,recovered=false,failed=false){await page.setViewportSize({width:320,height:740});await page.addInitScript(({recovered,failed})=>{
 const win=window as unknown as {__diagnosticCalls:{command:string;args:Record<string,unknown>}[];__TAURI_INTERNALS__:{invoke:(command:string,args:Record<string,unknown>)=>Promise<unknown>}};win.__diagnosticCalls=[];
 let status={detailed:false,expiresAtMs:0,sessionId:null as string|null,startedAtMs:0,endedAtMs:null,durationSeconds:0,maximumBytes:0,storedBytes:0,storedFrames:0,state:'idle',endReason:null as string|null,recoveryState:failed?'result_unknown':'confirmed',recoveryReason:failed?'恢复记录存盘未确认':null,recovered,recoveryHistory:[] as Record<string,unknown>[]};
 if(recovered)status={...status,sessionId:'capture-old-session',state:'interrupted',endReason:'process_restart',startedAtMs:1,expiresAtMs:2,storedBytes:100,storedFrames:1,maximumBytes:1048576,durationSeconds:1,recoveryHistory:[{sessionId:'capture-confirmed-old',phase:'stopped',confirmation:'confirmed',startedAtMs:1,expiresAtMs:2,endedAtMs:2,durationSeconds:1,maximumBytes:1048576,storedBytes:50,storedFrames:1,queueDropsObserved:0,endReason:'user'},{sessionId:'capture-old-session',phase:'interrupted',confirmation:'result_unknown',startedAtMs:1,expiresAtMs:2,endedAtMs:null,durationSeconds:1,maximumBytes:1048576,storedBytes:100,storedFrames:1,queueDropsObserved:0,endReason:'process_restart'}]};
 win.__TAURI_INTERNALS__={invoke:async(command,args)=>{win.__diagnosticCalls.push({command,args});
  if(command==='native_diagnostic_capture'){const request=args?.request as {action:string;expectedSessionId?:string;durationSeconds?:number;capacityMiB?:number}|undefined;
   if(request?.action==='start')status={...status,detailed:true,sessionId:'session-original',state:'capturing',durationSeconds:request.durationSeconds!,maximumBytes:request.capacityMiB!*1024*1024};
   if(request?.action==='stop')status={...status,detailed:false,state:'stopped',endReason:'user'};return structuredClone(status);
  }
  if(command==='native_document_preview_diagnostics')return {planId:'fixed-host-snapshot',createdAtUnixMs:String(Date.now()),expiresAtUnixMs:String(Date.now()+300000),bytes:256,scope:{profileId:'original-profile',configRevision:'42',clientInstanceId:'7'},logCount:2,frameCount:3,taskCount:1,omissions:[{category:'rawMessages',count:2,reason:'不含自由日志秘密'}]};
  if(command==='native_document_export_diagnostics')return {operationId:'system-original',status:'result-unknown',content:null,bytesRead:null,reason:'lost receipt'};
  throw new Error('unsupported command');
 }};
 },{recovered,failed});await page.goto('/mobile-diagnostics.html');await expect(page.getByRole('status').filter({hasText:recovered?'最终结果未确认':'尚未捕获'})).toBeVisible();}
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

test('process recovery exposes unknown old session and confirmed history without automatic replay',async({page})=>{
 await open(page,true);await expect(page.getByRole('button',{name:'停止原捕获会话'})).toBeDisabled();
 await page.getByText('捕获恢复记录（最近 2 项）',{exact:true}).click();await expect(page.getByLabel('捕获恢复历史')).toContainText('用户已停止');await expect(page.getByLabel('捕获恢复历史')).toContainText('最终结果未确认');await expect(page.getByLabel('捕获恢复历史')).toContainText('结束时间未确认');
 await page.getByRole('button',{name:'核对捕获状态'}).click();
 const actions=await page.evaluate(()=> (window as unknown as {__diagnosticCalls:{args:Record<string,unknown>}[]}).__diagnosticCalls.filter(call=>call.args?.request));expect(actions).toHaveLength(0);
 await expect(page.getByRole('status').filter({hasText:'最终结果未确认'})).toBeVisible();await expect(page.getByRole('button',{name:'开始有界捕获'})).toBeEnabled();
});
test('unconfirmed journal blocks new capture and gives only explicit capture-owned cleanup path',async({page})=>{
 await open(page,true,true);await expect(page.getByRole('alert')).toContainText('存盘未确认');await expect(page.getByRole('alert')).toContainText('本地数据');await expect(page.getByRole('button',{name:'开始有界捕获'})).toBeDisabled();
 const actions=await page.evaluate(()=> (window as unknown as {__diagnosticCalls:{args:Record<string,unknown>}[]}).__diagnosticCalls.filter(call=>call.args?.request));expect(actions).toHaveLength(0);
});

test('runtime inspection reports native unsupported without requesting VPN or claiming forwarding',async({page})=>{
 await open(page);
 await page.evaluate(()=>{const win=window as unknown as {__TAURI_INTERNALS__:{invoke:(command:string,args:Record<string,unknown>)=>Promise<unknown>}};const previous=win.__TAURI_INTERNALS__.invoke;win.__TAURI_INTERNALS__.invoke=async(command,args)=>{if(command==='gui_runtime_bridge_inspect')throw {code:'unsupported',message:'移动运行服务尚未适配，不能核对实际 Proxy。'};return previous(command,args);};});
 const button=page.getByRole('button',{name:'核对运行控制通道'});await expect(button).toBeVisible();expect((await button.boundingBox())?.height).toBeGreaterThanOrEqual(44);
 await button.click();await expect(page.getByRole('alert')).toContainText('移动运行服务尚未适配');await expect(page.getByLabel('控制通道核对回执')).toHaveCount(0);
 const calls=await page.evaluate(()=>(window as unknown as {__diagnosticCalls:{command:string}[]}).__diagnosticCalls.map(c=>c.command));expect(calls.some(c=>/vpn.*start|tun.*start/.test(c))).toBe(false);
});

test('only matching confirmed runtime receipt displays identity; unsafe large revision stays exact',async({page})=>{
 await open(page);
 await page.evaluate(()=>{const win=window as unknown as {__TAURI_INTERNALS__:{invoke:(command:string,args:Record<string,unknown>)=>Promise<unknown>}};const previous=win.__TAURI_INTERNALS__.invoke;let count=0;win.__TAURI_INTERNALS__.invoke=async(command,args)=>{if(command==='gui_runtime_bridge_inspect'){count++;return {status:'confirmed',transport:'pinned-ipc',coreInstanceId:'actual-instance',configRevision:'9007199254740993',vpnObservation:'unprovided',receipt:{operationId:'fixed-read',profileId:'actual-profile',coreInstanceId:count===1?'stale-instance':'actual-instance',configRevision:'9007199254740993',mutation:false,state:'confirmed'}};}return previous(command,args);};});
 await page.getByRole('button',{name:'核对运行控制通道'}).click();await expect(page.getByRole('alert')).toContainText('所属实例不一致');await expect(page.getByLabel('控制通道核对回执')).toHaveCount(0);
 await page.getByRole('button',{name:'核对运行控制通道'}).click();await expect(page.getByLabel('控制通道核对回执')).toContainText('9007199254740993');await expect(page.getByLabel('控制通道核对回执')).toContainText('VPN 接管状态未提供');
});
