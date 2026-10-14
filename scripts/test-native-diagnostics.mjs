import assert from 'node:assert/strict';
import {test} from 'node:test';
import {validateCaptureBudget,captureReason,captureHistoryReason,previewNativeDiagnostics,deliverNativeDiagnosticSnapshot,startNativeCapture,stopNativeCapture,readNativeCapture} from '../src/lib/services/native-diagnostics.ts';
const status={detailed:false,state:'stopped',endReason:'capacity'};
test('capture accepts only integer product budgets including numeric shared Input values',()=>{
 for(const pair of [['0','1'],['301','1'],['1','0'],['1','11'],['1.5','1'],['NaN','1'],['','1'],['1e2','1']])assert.equal(validateCaptureBudget(...pair),null);
 assert.deepEqual(validateCaptureBudget('300','10'),{durationSeconds:300,capacityMiB:10});assert.deepEqual(validateCaptureBudget(30,1),{durationSeconds:30,capacityMiB:1});
});
test('authoritative capture end reasons remain distinct and no missing state means successful sampling',()=>{
 assert.match(captureReason(status),/容量/);assert.match(captureReason({...status,endReason:'duration'}),/时长/);assert.match(captureReason({...status,endReason:'storage_failure'}),/失败/);assert.match(captureReason({...status,endReason:'scope_changed'}),/失效/);assert.equal(captureReason({...status,state:'idle',endReason:null}),'尚未捕获');
});
test('actual IPC sends only original prepared plan ID, explicit session stop and bounded start inputs',async()=>{
 const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return {planId:'fixed-original'};}}};
 try{const preview=await previewNativeDiagnostics();await deliverNativeDiagnosticSnapshot(preview.planId,'share');await readNativeCapture();await startNativeCapture(3,2);await stopNativeCapture('original-session');
 assert.deepEqual(calls,[{command:'native_document_preview_diagnostics',args:{}},{command:'native_document_export_diagnostics',args:{planId:'fixed-original',delivery:'share'}},{command:'native_diagnostic_capture',args:{}},{command:'native_diagnostic_capture',args:{request:{action:'start',durationSeconds:3,capacityMiB:2}}},{command:'native_diagnostic_capture',args:{request:{action:'stop',expectedSessionId:'original-session'}}}]);
 }finally{delete globalThis.window;}
});

test('recovered interrupted and journal failures never become confirmed user completion',()=>{
 assert.match(captureReason({...status,state:'interrupted',endReason:'process_restart'}),/结果未确认/);
 assert.match(captureReason({...status,state:'interrupted',endReason:'journal_failure'}),/存盘未确认/);
 assert.match(captureReason({...status,endReason:null}),/未提供/);
 assert.match(captureHistoryReason({phase:'interrupted',confirmation:'result_unknown',endReason:'process_restart'}),/不会恢复或重放/);
 assert.match(captureHistoryReason({phase:'capturing',confirmation:'confirmed',endReason:null}),/不能据此恢复/);
 assert.equal(captureHistoryReason({phase:'stopped',confirmation:'confirmed',endReason:'user'}),'用户已停止');
});
