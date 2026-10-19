import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
const source=readFileSync(new URL('../src-tauri/apple/ios-host-smoke.js',import.meta.url),'utf8');
const execute=new (Object.getPrototypeOf(async function(){}).constructor)('window',source);
function host(failure,at='plugins_native_vm_smoke') {
 return {__TAURI_INTERNALS__:{invoke:async(command)=>{
  if(command===at)throw failure;
  if(command==='gui_mobile_platform_snapshot')return {mobile:true,platform:'ios',vpnSupported:false,bundledKernelAvailable:false,adaptationState:'not-adapted'};
  if(command==='plugin:app|identifier')return 'org.zerodenet.znetsink';
  if(command==='plugins_snapshot')return {platform:{state:'ready',executionAllowed:true,target:{os:'ios'}}};
  throw Error('fixture must stop before any UI, network or user-data access');
 }}};
}
test('fixed driver reports the failing IPC stage and bounded native VM facts',async()=>{
 const result=await execute(host({code:'native_vm_smoke_failed',message:'sensitive raw exception',details:{fixture:'identity',actual:'GuestException',payload:'secret'}}));
 assert.deepEqual(result,{failedCheck:'ui-smoke-plugin-vm-ipc',ipcCode:'native_vm_smoke_failed',actual:'GuestException',fixture:'identity'});
 assert.ok(!JSON.stringify(result).includes('secret'));
});
test('unknown codes, error messages and payloads cannot enter the native receipt',async()=>{
 for(const message of ['https://private.invalid/?token=secret','ui-smoke-secret-token']) {
  const result=await execute(host({code:'secret-token',message,details:{actual:'secret-token',fixture:'secret-token'}}));
  assert.deepEqual(result,{failedCheck:'ui-smoke-plugin-vm-ipc',ipcCode:'unclassified'});
  assert.equal(JSON.stringify(result).includes('secret'),false);
 }
});
test('a profile read failure is distinguished from VM execution failure',async()=>{
 const result=await execute(host({code:'plugin_platform_unavailable',message:'native exception'},'plugins_snapshot'));
 assert.deepEqual(result,{failedCheck:'ui-smoke-plugin-profile-ipc',ipcCode:'plugin_platform_unavailable'});
});
test('secure-store results require actual success facts or explicitly gated unavailability',async()=>{
 const vm={executed:true,arithmetic:true,identity:true,ungrantedRejected:true,deadlineEnforced:true,revocationEnforced:true,target:{os:'ios'}};
 for(const [secureStorage,accepted] of [
  [{state:'available',roundTrip:true,lostKeyRejected:true,plaintextFallbackRejected:true},true],
  [{state:'unavailable',permissionsGated:true,roundTrip:false},true],
  [{state:'available',roundTrip:false,lostKeyRejected:true,plaintextFallbackRejected:true},false],
  [{state:'unavailable',permissionsGated:false,roundTrip:false},false],
  [{state:'unavailable',permissionsGated:true,roundTrip:true},false],
  [undefined,false],
 ]) {
  const window=host(Error('must stop before UI'),'app_license_inventory');
  const invoke=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async command=>command==='plugins_native_vm_smoke'?{...vm,secureStorage}:invoke(command);
  const result=await execute(window);
  assert.equal(result.failedCheck,accepted?'ui-smoke-licenses-ipc':'ui-smoke-native-secure-storage-contract');
  assert.equal(result.nativeSecureStorageAccepted,undefined);
 }
});
