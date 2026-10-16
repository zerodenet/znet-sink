import assert from 'node:assert/strict';
import {test} from 'node:test';
import {serializeDebugFrameForClipboard,serializeDebugFramesForClipboard,serializeLogForClipboard,serializeLogsForClipboard} from '../src/lib/services/diagnostic-copy.ts';
import {fixedLogSelection} from '../src/lib/types/log-export.ts';
import {previewLogExport,deliverLogExport} from '../src/lib/services/log-export.ts';
const secret='secret-canary-password';
const log={id:10,source:'plugin',level:'error',message:secret,occurredAtUnixMs:123,fields:{message:secret,raw_line:secret,rx_bytes:123456789,__znetHostDiagnosticV1:{action:'run',outcome:'ok'},diagnosticEvent:{kind:'process',event:'started'}}};
test('all legacy single/filter log and frame copies omit raw bodies, markers, errors and search',()=>{
 const frame={id:272,atMs:123,direction:'rx',frameType:secret,error:secret,payload:{data:{message:secret,rx_bytes:123456789}}};
 for(const copied of [serializeLogForClipboard(log),serializeLogsForClipboard([log],{source:'all',level:'all',search:secret,hasMore:true,copiedAtUnixMs:1}),serializeDebugFrameForClipboard(frame),serializeDebugFramesForClipboard([frame],{frameType:secret,hasMore:false,copiedAtUnixMs:1})]){
  assert(!copied.includes(secret));assert(!copied.includes('123456789'));assert(!copied.includes('__znet'));assert(!copied.includes('started'));
 }
 assert.equal(JSON.parse(serializeLogForClipboard(log)).rawFieldsOmitted,true);assert.equal(JSON.parse(serializeDebugFrameForClipboard(frame)).payloadOmitted,true);
});
test('fixed actual ID/time selection never carries original text, arbitrary fields or search term',()=>{
 const request=fixedLogSelection([log],'plugin','error',secret,true);assert.deepEqual(request,{items:[{id:'10',occurredAtUnixMs:'123'}],source:'plugin',level:'error',searchApplied:true,fromUnixMs:null,untilUnixMs:null,partial:true});assert(!JSON.stringify(request).includes(secret));
 for(const entries of [[],Array(2001).fill(log),[{...log,id:Number.MAX_SAFE_INTEGER+1}],[{...log,occurredAtUnixMs:-1}]])assert.throws(()=>fixedLogSelection(entries,'all','all','',false),/1–2000/);
});
test('real frontend RPCs deliver only original host plan and explicit action, never caller bytes',async()=>{
 const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return {planId:'fixed-host-plan'};}}};
 try{const request=fixedLogSelection([log],'plugin','error',secret,true);const preview=await previewLogExport(request);for(const delivery of ['copy','save','share'])await deliverLogExport(preview.planId,delivery);assert.deepEqual(calls,[{command:'log_export_preview',args:{request}},...['copy','save','share'].map(delivery=>({command:'log_export_deliver',args:{planId:'fixed-host-plan',delivery}}))]);assert(!JSON.stringify(calls).includes(secret));}finally{delete globalThis.window;}
});
test('unknown metadata shapes never serialize through enum or integer slots',()=>{
 const malformed={...log,id:'secret-canary-password',source:{value:'secret-canary-password'},level:'secret-canary-password',occurredAtUnixMs:NaN};const copied=JSON.parse(serializeLogForClipboard(malformed));assert.equal(copied.id,null);assert.equal(copied.source,null);assert.equal(copied.level,null);assert.equal(copied.occurredAtUnixMs,null);assert(!JSON.stringify(copied).includes(secret));
});
