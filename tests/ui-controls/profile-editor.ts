// Raw host facts only; never build source text from legacy listing.content.
import type { ProfileEditorInput, ProfileEditorSnapshot, ProfileEditorSave } from '../../src/lib/features/profile-editor/types';
import type {ProxyConfigProfile} from '../../src/lib/types/domain';
import { proxyConfigSignal } from '../../src/lib/services/proxy-config-signal.svelte';
export const rawProfileText='{\n  "route": {"final": {"type": "direct"}},\n  "runtime": {"udp_upstream_idle_timeout_seconds": 18446744073709551615}\n}';
const controls={listingNumber:'',filePath:'/fixture/raw-profile.json',calls:[] as {kind:string;input?:unknown}[],holdRead:false,holdValidate:false,holdSave:false,failSave:false,invalid:false,readOnly:'',releaseRead:(()=>{}) as ()=>void,releaseValidate:(()=>{}) as ()=>void,releaseSave:(()=>{}) as ()=>void};
Object.assign(window,{profileEditorFixture:controls});
proxyConfigSignal.onActiveChanged(()=>controls.calls.push({kind:'activate'}));
const saved=new Map<string,ProfileEditorSnapshot>();
function value(id?:string):ProfileEditorSnapshot{return saved.get(id??'')??{targetId:id??null,createTargetId:id?null:'manual-fixture-create',binding:`binding-${id??'create'}`,name:id==='main'?'日常网络配置':id==='work'?'工作配置':'',sourceJson:id?rawProfileText:null,readOnlyReason:null,sourceUpdatedAtUnixMs:id?'1':null,validationScope:'native Zero schema; dependencies not executed'};}
export async function getProfileEditorSnapshot(id?:string){controls.calls.push({kind:'snapshot',input:id});if(controls.holdRead)await new Promise<void>(r=>controls.releaseRead=r);const result={...value(id)};if(controls.readOnly){result.binding=null;result.readOnlyReason=controls.readOnly==='managed'?'插件托管来源，正文不提供；请通过来源插件管理':'普通订阅来源正文只读，请通过订阅来源同步管理';if(controls.readOnly==='managed')result.sourceJson=null;}return result;}
export async function validateProfileEditor(input:ProfileEditorInput){controls.calls.push({kind:'validate',input});if(controls.holdValidate)await new Promise<void>(r=>controls.releaseValidate=r);if(controls.invalid)throw {code:'native_validation_failed',message:'SECRET_EDITOR_CANARY',details:{errors:[{field_path:'runtime',reasonCode:'invalid_value',message:'SECRET_EDITOR_CANARY',line:3,column:4}]}};return {state:'validated' as const,sourceJson:input.sourceJson.trim(),validationScope:'native Zero schema; dependencies not executed'};}
export async function saveProfileEditor(input:ProfileEditorInput & {name:string}):Promise<ProfileEditorSave>{controls.calls.push({kind:'save',input});if(controls.holdSave)await new Promise<void>(r=>controls.releaseSave=r);if(controls.invalid)await validateProfileEditor(input);const id=input.id??'manual-fixture-create';const snapshot={...value(id),name:input.name,sourceJson:input.sourceJson,binding:`saved-${id}`};saved.set(id,snapshot);if(controls.failSave)throw {code:'io_error',message:'SECRET_EDITOR_CANARY unknown storage'};return {state:'saved_pending_apply',snapshot};}
export async function readProfileEditorFile(path:string){controls.calls.push({kind:'read-file',input:path});return rawProfileText;}

/** Deliberately corrupted legacy Value demonstrates that the editor reads raw host text. */
export function profileEditorListing(items:ProxyConfigProfile[]):ProxyConfigProfile[]{
 if(new URLSearchParams(location.search).get('fixture')!=='profile-editor')return items;
 const corrupted=18446744073709551615;controls.listingNumber=String(corrupted);
 const rows:ProxyConfigProfile[]=items.map(item=>({...item,content:{runtime:{udp_upstream_idle_timeout_seconds:corrupted}}}));
 for(const [id,item] of saved){const existing=rows.find(row=>row.id===id);if(existing){existing.name=item.name;}else rows.push({id,name:item.name,kernel:'zero',format:'json',content:{},active:false,updatedAtUnixMs:2,capabilities:{hasProxyNodes:false,hasProxyGroups:false,hasRouteRules:false,hasRuleSets:false,hasSelector:false,hasUrlTest:false,featureKeys:[]}});}
 return rows;
}
// This is a chooser mock only; the file bytes still go through the production raw-read port.
if(new URLSearchParams(location.search).get('fixture')==='profile-editor'){
 const tauri=(window as unknown as {__TAURI_INTERNALS__?:{invoke?:(command:string,args?:unknown)=>Promise<unknown>}}).__TAURI_INTERNALS__??{};
 const previous=tauri.invoke?.bind(tauri);
 tauri.invoke=(command,args)=>command==='plugin:dialog|open'?Promise.resolve(controls.filePath):previous?previous(command,args):Promise.reject(Error('unexpected fixture command'));
 Object.assign(window,{__TAURI_INTERNALS__:tauri});
}
