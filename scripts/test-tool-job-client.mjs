import assert from 'node:assert/strict';
import { test } from 'node:test';
import { subscribeToolScope, startToolJob, cancelToolJob } from '../src/lib/features/tool-jobs/client.ts';
const flush=()=>new Promise(resolve=>setImmediate(resolve));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};};
function host(read) {
 let callback;const calls=[];
 const previousWindow=globalThis.window,previousDocument=globalThis.document;
 globalThis.window=Object.assign(new EventTarget(),{
  __TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},
  __TAURI_INTERNALS__:{transformCallback:handler=>{callback=handler;return 1;},invoke:async(command,args)=>{calls.push({command,args});if(command==='gui_client_core_snapshot')return read();if(command==='plugin:event|listen')return 1;return undefined;}},
 });
 globalThis.document=Object.assign(new EventTarget(),{hidden:false});
 return {calls,emit:payload=>callback({event:'client-core:updated',id:1,payload}),focus:()=>window.dispatchEvent(new Event('focus')),restore:()=>{if(previousWindow===undefined)delete globalThis.window;else globalThis.window=previousWindow;if(previousDocument===undefined)delete globalThis.document;else globalThis.document=previousDocument;}};
}
test('scope observation suppresses an old read failure or success after a newer native event',async()=>{
 for(const failure of [false,true]){
  const pending=deferred(),h=host(()=>pending.promise),seen=[];let stop;
  try{
   stop=await subscribeToolScope(value=>seen.push(value));h.focus();
   const latest={revision:3,scopeGeneration:3,scope:{profileId:'A',configRevision:'7',coreInstanceId:1},sourceStatus:'ready'};
   h.emit(latest);
   if(failure)pending.reject(new Error('old failure'));else pending.resolve({...latest,revision:1,scopeGeneration:1});
   await flush();assert.deepEqual(seen,[latest]);
  }finally{stop?.();await flush();h.restore();}
 }
});
test('scope observer disposal releases its own listener and suppresses pending reads',async()=>{
 const pending=deferred(),h=host(()=>pending.promise),seen=[];let stop;
 try{
  stop=await subscribeToolScope(value=>seen.push(value));h.focus();stop();stop=undefined;
  pending.resolve({revision:1,scopeGeneration:1});await flush();assert.deepEqual(seen,[]);
  assert.equal(h.calls.filter(call=>call.command==='plugin:event|unlisten').length,1);
 }finally{stop?.();h.restore();}
});
test('actual Tauri wrappers transmit the fixed scope and client revision without adding retries',async()=>{
 const h=host(async()=>{});
 try{
  const scope={profileId:'A',configRevision:'18446744073709551615',coreInstanceId:1};
  await startToolJob({kind:'fake_ip_clear',params:{},expectedScope:scope,expectedClientRevision:9});
  await cancelToolJob(42,scope,9);
  assert.deepEqual(h.calls,[
   {command:'gui_tool_job_start',args:{request:{kind:'fake_ip_clear',params:{},expectedScope:scope,expectedClientRevision:9}}},
   {command:'gui_tool_job_cancel',args:{jobId:42,expectedScope:scope,expectedClientRevision:9}},
  ]);
 }finally{h.restore();}
});
