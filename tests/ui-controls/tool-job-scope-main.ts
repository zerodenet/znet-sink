import '../../src/app.css';
import { mount } from 'svelte';
import ToolJobScopeFixture from './ToolJobScopeFixture.svelte';
import type { ClientCoreSnapshot, StartToolJobRequest, ToolJobSnapshot } from '../../src/lib/types/gui-api';
const original = {profileId:'original',configRevision:'18446744073709551614',coreInstanceId:4};
let current: ClientCoreSnapshot = {revision:1,scopeGeneration:1,scope:original,sourceStatus:'ready',activeProbeJobs:[]};
let nextId=100;
const requests: StartToolJobRequest[]=[];
const allJobs: ToolJobSnapshot[]=[
 {id:1,kind:'dns_lookup',state:'completed',scope:{...original,profileId:'old'},subject:'previous.example',params:{},result:{answers:[{value:'192.0.2.99'}]},createdAtUnixMs:1,updatedAtUnixMs:1,deadlineAtUnixMs:100},
 {id:2,kind:'fake_ip_clear',state:'completed',scope:original,subject:'old-clear',params:{},result:{enabled:true,removedMappings:4,removedAddresses:4,liveMappings:0,retiredAddresses:0},createdAtUnixMs:1,updatedAtUnixMs:1,deadlineAtUnixMs:100},
 {id:3,kind:'route_trace',state:'completed',scope:{...original,profileId:'old'},subject:'old-route',params:{},result:{hops:[],route:'old-only'},createdAtUnixMs:1,updatedAtUnixMs:1,deadlineAtUnixMs:100},
];
const emit=(name:string,payload:unknown)=>window.dispatchEvent(new CustomEvent(name,{detail:payload}));
const report=()=>emit('tool-fixture:requests',requests);
Object.defineProperty(window,'__TAURI_INTERNALS__',{value:{invoke:async(command:string,args:Record<string,unknown>)=>{
 if(command==='gui_client_core_snapshot')return structuredClone(current);
 if(command==='gui_tool_job_list')return structuredClone(allJobs);
 if(command==='gui_tool_job_start'){
   const request=args.request as StartToolJobRequest;requests.push(structuredClone(request));report();
   const job:ToolJobSnapshot={id:++nextId,kind:request.kind,state:'queued',scope:structuredClone(request.expectedScope!),subject:'current.example',params:request.params??{},createdAtUnixMs:10,updatedAtUnixMs:10,deadlineAtUnixMs:1000};allJobs.push(job);return structuredClone(job);
 }
 if(command==='gui_tool_job_cancel'){const job=allJobs.find(job=>job.id===args.jobId)!;return {...job,state:'cancelling',updatedAtUnixMs:12};}
 throw new Error(`Unexpected fixture command ${command}`);
}}});
window.addEventListener('tool-fixture:action',event=>{
 const action=(event as CustomEvent<string>).detail;
 if(action==='switch'){current={...current,revision:current.revision+1,scopeGeneration:current.scopeGeneration!+1,scope:{...current.scope,profileId:'replacement',configRevision:'18446744073709551615'}};emit('client-core:updated',current);return;}
 if(action==='restart'){current={...current,revision:current.revision+1,scopeGeneration:current.scopeGeneration!+1,scope:{...current.scope,coreInstanceId:current.scope.coreInstanceId+1}};emit('client-core:updated',current);return;}
 if(action==='unsafe-id'){emit('client-core:tool-job-updated',{...allJobs[0],id:JSON.parse('9007199254740993'),scope:current.scope,state:'running',updatedAtUnixMs:20});return;}
 if(action==='old-clear'){emit('client-core:tool-job-updated',{...allJobs[1],updatedAtUnixMs:20});return;}
 const kind=action==='dns'?'dns_lookup':'fake_ip_clear';
 const job=allJobs.findLast(job=>job.kind===kind&&job.id>100);
 if(job)emit('client-core:tool-job-updated',{...job,state:'completed',updatedAtUnixMs:20,result:kind==='dns_lookup'?{answers:[{name:'current.example',type:'A',value:'192.0.2.4'}]}:{enabled:true,removedMappings:1,removedAddresses:1,liveMappings:0,retiredAddresses:0}});
});
mount(ToolJobScopeFixture,{target:document.getElementById('app')!});
