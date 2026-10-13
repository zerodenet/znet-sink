import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
const source=readFileSync(new URL('../src/lib/features/mobile/settings-migration.ts',import.meta.url),'utf8');
const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const api={};new Function('exports',js)(api);
const {SettingsMigration}=api;
const plan={planId:'plan-1',changedFields:['tun.mtu'],expectedRevision:'before',scope:{profileId:'a',configRevision:'1',coreInstanceId:1}};
const receipt={planId:'plan-1',localState:'saved',runtimeState:'pending-apply',savedRevision:'after',theme:'system',notice:'已保存，待应用'};
const make=(overrides={})=>{
 const calls=[],seen=[];
 const gateway={export:async()=>({}),preview:async content=>{calls.push(['preview',content]);return plan;},commit:async id=>{calls.push(['commit',id]);return receipt;},status:async id=>{calls.push(['status',id]);return receipt;},...overrides};
 return {migration:new SettingsMigration(gateway,state=>seen.push(state)),calls,seen};
};
test('editing and preview never save; confirmation commits only fixed host plan once',async()=>{
 const {migration,calls}=make();migration.edit('portable-draft');await migration.preview();
 assert.deepEqual(calls,[['preview','portable-draft']]);assert.equal(migration.state.phase,'ready');
 await migration.confirm();await migration.confirm();assert.deepEqual(calls,[['preview','portable-draft'],['commit','plan-1']]);assert.equal(migration.state.receipt.runtimeState,'pending-apply');
});
test('editing after preview invalidates it; stale preview cannot deliver after scope change',async()=>{
 let resolve;const {migration,calls}=make({preview:()=>new Promise(r=>resolve=r)});
 migration.edit('old');const work=migration.preview();migration.invalidate();resolve(plan);await work;
 assert.equal(migration.state.phase,'editing');assert.equal(migration.state.preview,null);assert.equal(migration.state.content,'old');
 await migration.confirm();assert.deepEqual(calls,[]);
});
test('lost confirmation stays unknown and queries original plan instead of replaying import',async()=>{
 let commits=0,checks=0;const {migration}=make({commit:async()=>{commits++;throw{code:'connection_closed',message:'secret-token'};},status:async id=>{checks++;assert.equal(id,'plan-1');return receipt;}});
 migration.edit('draft');await migration.preview();await migration.confirm();assert.equal(migration.state.phase,'unknown');assert.ok(!migration.state.error.includes('secret-token'));
 await migration.confirm();assert.equal(commits,1);await migration.check();assert.equal(checks,1);assert.equal(commits,1);assert.equal(migration.state.phase,'saved');
});
test('explicit conflict keeps draft, invalidates confirmed scope, and exposes no backend secret text',async()=>{
 const {migration}=make({commit:async()=>{throw{code:'conflict',message:'https://secret/token'};}});migration.edit('draft');await migration.preview();await migration.confirm();
 assert.equal(migration.state.phase,'error');assert.equal(migration.state.content,'draft');assert.ok(!migration.state.error.includes('https'));
});
test('destroyed UI cannot receive a late operation acknowledgement',async()=>{
 let finish;const {migration,seen}=make({commit:()=>new Promise(r=>finish=r)});migration.edit('draft');await migration.preview();const work=migration.confirm();const count=seen.length;migration.destroy();finish(receipt);await work;assert.equal(seen.length,count);
});

test('unknown save keeps its original plan through edit and configuration-change attempts',async()=>{
 let commits=0;const {migration}=make({commit:async()=>{commits++;throw{code:'connection_closed'};}});
 migration.edit('original');await migration.preview();await migration.confirm();migration.edit('replacement');migration.invalidate();await migration.preview();
 assert.equal(migration.state.phase,'unknown');assert.equal(migration.state.content,'original');assert.equal(migration.state.preview.planId,'plan-1');assert.equal(commits,1);
});

test('oversized UTF8 content is rejected before creating a host preview',async()=>{
 const {migration,calls}=make();migration.edit('中'.repeat(800000));await migration.preview();assert.equal(migration.state.phase,'error');assert.match(migration.state.error,/2 MiB/);assert.deepEqual(calls,[]);
});
