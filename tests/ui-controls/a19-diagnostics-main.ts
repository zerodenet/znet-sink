import './styles.css';
import { mount } from 'svelte';
import Fixture from './A19DiagnosticsFixture.svelte';
import type { ClientCoreSnapshot, StartToolJobRequest, ToolJobSnapshot } from '../../src/lib/types/gui-api';
let current: ClientCoreSnapshot = {revision:1,scopeGeneration:1,scope:{profileId:'actual-fixture',configRevision:'18446744073709551615',coreInstanceId:1},sourceStatus:'ready',activeProbeJobs:[]};
let id=100;
const requests: StartToolJobRequest[]=[];
const jobs:ToolJobSnapshot[]=[];
const copies:string[]=[];
let denied=false;
const emit=(name:string,payload:unknown)=>window.dispatchEvent(new CustomEvent(name,{detail:payload}));
Object.defineProperty(window,'__TAURI_INTERNALS__',{value:{invoke:async(command:string,args:Record<string,unknown>)=>{
 if(command==='gui_client_core_snapshot')return structuredClone(current);
 if(command==='gui_tool_job_list')return structuredClone(jobs);
 if(command==='platform_clipboard_write'){copies.push(String(args.text));return;}
 if(command==='gui_tool_job_start'){
   const request=args.request as StartToolJobRequest;requests.push(structuredClone(request));
   if(denied)throw new Error('permission_denied：当前内核没有 Admin 诊断权限');
   const job:ToolJobSnapshot={id:++id,kind:request.kind,state:'queued',scope:structuredClone(request.expectedScope!),subject:'fixture.test',params:structuredClone(request.params??{}),createdAtUnixMs:10,updatedAtUnixMs:10,deadlineAtUnixMs:1000};jobs.push(job);return structuredClone(job);
 }
 if(command==='gui_tool_job_cancel'){const job=jobs.find(job=>job.id===args.jobId)!;return {...job,state:'cancelling',updatedAtUnixMs:11};}
 throw new Error('Unexpected A19 fixture command');
}}});
Object.assign(window,{a19Fixture:{
 requests,copies,
 deny(value:boolean){denied=value;},
 complete(kind:string,result:unknown){const job=jobs.findLast(job=>job.kind===kind);if(!job)throw new Error('No actual started fixture job');job.state='completed';job.result=result;job.updatedAtUnixMs++;emit('client-core:tool-job-updated',structuredClone(job));},
 change(){current={...current,revision:current.revision+1,scopeGeneration:current.scopeGeneration!+1,scope:{...current.scope,profileId:'replacement'}};emit('client-core:updated',structuredClone(current));},
}});
mount(Fixture,{target:document.getElementById('app')!});
