import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
const source=readFileSync(new URL('../src-tauri/apple/ios-host-smoke.js',import.meta.url),'utf8');
const execute=new (Object.getPrototypeOf(async function(){}).constructor)('window',source);
function diagnostic() {
 const skipped={attempted:false,osStatus:null};
 return {schemaVersion:1,isolated:true,protectedDataAvailable:false,lastHostAdmission:{protectedDataAvailable:false,probe:skipped,apiAvailable:false},availabilityProbe:skipped,readBeforeCreate:skipped,random:skipped,create:skipped,readAfterCreate:skipped,cleanup:skipped,readBackMatches:null,stopStage:'protectedDataGate',roundTrip:false};
}
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
  window.__TAURI_INTERNALS__.invoke=async command=>command==='plugins_native_vm_smoke'?{...vm,secureStorage:secureStorage?{...secureStorage,diagnostics:diagnostic()}:undefined}:invoke(command);
  const result=await execute(window);
  assert.equal(result.failedCheck,accepted?'ui-smoke-licenses-ipc':'ui-smoke-native-secure-storage-contract');
  assert.equal(result.nativeSecureStorageAccepted,undefined);
 }
});
test('isolated Keychain facts survive a later failure without keys or production acceptance',async()=>{
 const step=osStatus=>({attempted:true,osStatus});
 const details={...diagnostic(),protectedDataAvailable:false,lastHostAdmission:{protectedDataAvailable:false,probe:{attempted:false,osStatus:null},apiAvailable:false,keyBase64:'secret-canary'},availabilityProbe:step(-50),readBeforeCreate:step(-25300),random:step(0),create:step(0),readAfterCreate:step(0),cleanup:step(0),readBackMatches:true,stopStage:'completed',roundTrip:true,keyBase64:'secret-canary',account:'secret-canary'};
 const window=host(Error('must stop before UI'),'app_license_inventory');
 const invoke=window.__TAURI_INTERNALS__.invoke;
 window.__TAURI_INTERNALS__.invoke=async command=>command==='plugins_native_vm_smoke'?{executed:true,arithmetic:true,identity:true,ungrantedRejected:true,deadlineEnforced:true,revocationEnforced:true,target:{os:'ios'},secureStorage:{state:'unavailable',permissionsGated:true,roundTrip:false,diagnostics:details}}:invoke(command);
 const result=await execute(window);
 assert.equal(result.failedCheck,'ui-smoke-licenses-ipc');
 assert.equal(result.nativeKeychainDiagnostics.lastHostAdmission.apiAvailable,false);
 assert.equal(result.nativeKeychainDiagnostics.roundTrip,true);
 assert.equal(result.nativeKeychainDiagnostics.availabilityProbe.osStatus,-50);
 assert.equal(result.nativeSecureStorageAccepted,undefined);
 assert.equal(JSON.stringify(result).includes('secret-canary'),false);
});
test('a skipped probe cannot report an OSStatus or fabricated isolated success',async()=>{
 for(const patch of [{availabilityProbe:{attempted:false,osStatus:-34018}},{roundTrip:true},{stopStage:'secret-token'}]) {
  const window=host(Error('must stop before UI'),'app_license_inventory'),invoke=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async command=>command==='plugins_native_vm_smoke'?{executed:true,arithmetic:true,identity:true,ungrantedRejected:true,deadlineEnforced:true,revocationEnforced:true,target:{os:'ios'},secureStorage:{state:'unavailable',permissionsGated:true,roundTrip:false,diagnostics:{...diagnostic(),...patch}}}:invoke(command);
  const result=await execute(window);
  assert.equal(result.failedCheck,'ui-smoke-native-keychain-diagnostics');
  assert.equal(result.nativeKeychainDiagnostics,undefined);
 }
});
