import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { decodeInspection, inspectionTime, scopeFingerprint, workspaceReasons, packetReasons } from '../src/lib/features/storage-inspection/model.ts';
import { InspectionSession } from '../src/lib/features/storage-inspection/session.ts';
const workspace = () => ({hostSessionId:'host',sampledAtUnixMs:'1000',readOnly:true,barriersRetained:true,state:'metadata-readable',reasonCode:'metadata-readable',summary:'SECRET_CANARY',evidence:{databasePresent:true,databaseBytes:'18446744073709551615',applicationId:1,schemaVersion:5,metadataRows:2,pendingRows:1,candidateBodiesRead:false,snapshotStable:true}});
const packet = () => ({hostSessionId:null,sampledAtUnixMs:'1000',readOnly:true,barriersRetained:true,state:'blocked',reasonCode:'entropy-unavailable',summary:'SECRET_CANARY',newCloseBlocked:true,inMemoryBarrierCount:128,publicationUnconfirmed:true,evidence:{journalPresent:true,markerPresent:true,markerState:'initialized',lockState:'acquired',journalBytes:'18446744073709551615',journalSha256:'a'.repeat(64),schemaVersion:1,verifiedRecordCount:128,snapshotStable:true}});
const deferred = () => { let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b});return {resolve,reject,promise}; };
test('workspace projection keeps decimal u64 and omits all raw/secret material, even host summary',()=>{
 const raw={...workspace(),candidateBody:'SECRET_CANARY',operations:[{id:'SECRET_CANARY'}]};raw.evidence.path='SECRET_CANARY';const result=decodeInspection('workspace',raw);assert.equal(result.evidence.databaseBytes,'18446744073709551615');assert.ok(!JSON.stringify(result).includes('SECRET_CANARY'));assert.equal(result.evidence.candidateBodiesRead,false);
});
test('packet projection preserves nullable host identity, bounded counts/hash and never invents authorization',()=>{
 const result=decodeInspection('packet',packet());assert.equal(result.hostSessionId,null);assert.equal(result.newCloseBlocked,true);assert.equal(result.evidence.journalBytes,'18446744073709551615');assert.equal(result.inMemoryBarrierCount,128);assert.ok(!JSON.stringify(result).includes('SECRET_CANARY'));
});
test('every closed reason is accepted only with its corresponding state',()=>{
 for(const [kind,base,reasons,good] of [['workspace',workspace,workspaceReasons,'metadata-readable'],['packet',packet,packetReasons,'ready']]) for(const code of Object.keys(reasons)){ const raw={...base(),reasonCode:code,state:code===good?good:code==='not-initialized'?'not-initialized':'blocked'};assert.doesNotThrow(()=>decodeInspection(kind,raw)); }
});
test('unknown reason/state and false trust flags reject without trusting free text',()=>{
 for(const patch of [{reasonCode:'SECRET_CANARY'},{state:'success'},{readOnly:false},{barriersRetained:false},{reasonCode:'permission-denied'}])assert.throws(()=>decodeInspection('workspace',{...workspace(),...patch}));
});
test('invalid metrics, candidate body access and unsafe integer values fail validation',()=>{
 for(const patch of [{metadataRows:65},{pendingRows:3},{databaseBytes:'18446744073709551616'},{databaseBytes:10},{candidateBodiesRead:true},{snapshotStable:'yes'},{applicationId:2147483648}])assert.throws(()=>decodeInspection('workspace',{...workspace(),evidence:{...workspace().evidence,...patch}}));
 for(const patch of [{verifiedRecordCount:129},{schemaVersion:-1},{journalSha256:'SECRET_CANARY'},{lockState:'granted'}])assert.throws(()=>decodeInspection('packet',{...packet(),evidence:{...packet().evidence,...patch}}));
});
test('session does not read until explicit click, deduplicates and exposes pending',async()=>{
 const d=deferred();let calls=0;const s=new InspectionSession('workspace',()=>{calls++;return d.promise},()=>{});assert.equal(calls,0);const read=s.inspect();await s.inspect();assert.equal(calls,1);assert.equal(s.view.busy,true);d.resolve(workspace());await read;assert.equal(s.view.busy,false);assert.equal(s.view.result.state,'metadata-readable');
});
test('scope change during inspection drops old response and permits only a later explicit read',async()=>{
 const d=deferred();let calls=0;const s=new InspectionSession('workspace',()=>{calls++;return calls===1?d.promise:Promise.resolve(workspace())},()=>{});const read=s.inspect();s.invalidate();d.resolve(workspace());await read;assert.equal(s.view.result,null);assert.equal(calls,1);await s.inspect();assert.equal(calls,2);assert.ok(s.view.result);
});
test('read failure keeps old sampled metadata as history and never exposes error contents',async()=>{
 let fail=false;const s=new InspectionSession('workspace',async()=>{if(fail)throw {message:'SECRET_CANARY'};return workspace()},()=>{});await s.inspect();fail=true;await s.inspect();assert.equal(s.view.result.evidence.metadataRows,2);assert.equal(s.view.historical,true);assert.ok(!JSON.stringify(s.view).includes('SECRET_CANARY'));assert.match(s.view.error,/屏障保留/);
});
test('scope invalidation and later failure do not erase or upgrade the old check',async()=>{
 let fail=false;const s=new InspectionSession('workspace',async()=>{if(fail)throw Error('secret');return workspace()},()=>{});await s.inspect();s.invalidate();assert.equal(s.view.historical,true);fail=true;await s.inspect();assert.equal(s.view.historical,true);assert.ok(s.view.result);
});
test('disposed UI ignores late delivery without pretending host read was cancelled',async()=>{
 const d=deferred();let publications=0,calls=0;const s=new InspectionSession('workspace',()=>{calls++;return d.promise},()=>publications++);const read=s.inspect();s.dispose();const before=publications;d.resolve(workspace());await read;assert.equal(calls,1);assert.equal(publications,before);assert.equal(s.view.result,null);
});
test('unknown malformed host reply leaves fixed failure, never a metadata success',async()=>{
 const s=new InspectionSession('workspace',async()=>({...workspace(),reasonCode:'secret'}),()=>{});await s.inspect();assert.equal(s.view.result,null);assert.ok(s.view.error);
});
test('not initialized is separate from failure and blocked, without filling unknown counts with zero',async()=>{
 const raw={...workspace(),state:'not-initialized',reasonCode:'not-initialized',evidence:{...workspace().evidence,databasePresent:false,metadataRows:null,pendingRows:null,snapshotStable:false}};const s=new InspectionSession('workspace',async()=>raw,()=>{});await s.inspect();assert.equal(s.view.result.evidence.metadataRows,null);assert.equal(s.view.error,null);assert.equal(s.view.historical,false);
});
test('ordinary revision does not expire evidence but scopeGeneration detects unobserved ABA',()=>{
 const scope={profileId:'a',configRevision:1,coreInstanceId:1};assert.equal(scopeFingerprint({scope,scopeGeneration:1,revision:1}),scopeFingerprint({scope,scopeGeneration:1,revision:2}));assert.notEqual(scopeFingerprint({scope,scopeGeneration:1}),scopeFingerprint({scope,scopeGeneration:3}));assert.equal(scopeFingerprint({scope}),null);assert.equal(scopeFingerprint({scope,scopeGeneration:Number.MAX_SAFE_INTEGER+1}),null);
});
test('unrepresentable timestamps remain unknown without rounding into a date',()=>{assert.equal(inspectionTime('18446744073709551615'),'时间未提供');assert.ok(!inspectionTime('1000').includes('未提供'));});
test('production RPC wrappers are separate parameterless readonly commands, with no operation/close calls',()=>{
 const source=readFileSync(new URL('../src/lib/services/core.ts',import.meta.url),'utf8');for(const [fn,rpc]of [['inspectWorkspaceStorage','gui_workspace_storage_inspect'],['inspectPacketLedger','gui_packet_ledger_inspect']]){const body=source.match(new RegExp(`export async function ${fn}\\(\\)[\\s\\S]*?\\n}`))?.[0];assert.ok(body);assert.ok(body.includes(`return invoke('${rpc}');`));assert.equal((body.match(/invoke\(/g)||[]).length,1);}
});

test('WAL-active metadata is blocked rather than a valid empty main database or candidate confirmation',()=>{
 const raw={...workspace(),state:'blocked',reasonCode:'wal-active',evidence:{...workspace().evidence,metadataRows:null,pendingRows:null,snapshotStable:false}};const result=decodeInspection('workspace',raw);assert.equal(result.state,'blocked');assert.equal(result.evidence.metadataRows,null);assert.equal(result.evidence.pendingRows,null);assert.match(result.summary,/不会忽略/);assert.throws(()=>decodeInspection('workspace',{...raw,state:'metadata-readable'}));
});
