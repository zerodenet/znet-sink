import assert from 'node:assert/strict';
import test from 'node:test';
globalThis.$state = value => value;
const { mobileGroups, ungroupedNodes, failedProbeTargets, controlScope, sameProbeScope, latestProbeJob } = await import('../src/lib/features/nodes/mobile-view.ts');
const node = tag => ({ id:tag,tag,name:tag,protocol:'vmess',delay:0 });
const group = (name,kind='selector',members=['a','b']) => ({name,kind,outbounds:members.map(tag=>({tag})),selected:members[0]});
const scope={profileId:'p',configRevision:7,coreInstanceId:3};
test('twenty groups preserve configured order, group search includes all members and member search narrows temporarily',()=>{
 const groups=Array.from({length:20},(_,index)=>group(`group-${index}`));
 const nodes=[node('a'),node('b'),node('independent')];
 assert.deepEqual(mobileGroups(groups,nodes,'').map(row=>row.group.name),groups.map(row=>row.name));
 const matched=mobileGroups(groups,nodes,'group-19');assert.equal(matched.length,1);assert.deepEqual(matched[0].members.map(node=>node.tag),['a','b']);
 assert.equal(mobileGroups(groups,nodes,'b').length,20);assert(mobileGroups(groups,nodes,'b').every(row=>row.members.length===1));
 assert.deepEqual(ungroupedNodes(groups,nodes,'').map(node=>node.tag),['independent']);
 assert.deepEqual(ungroupedNodes([],nodes,'').map(node=>node.tag),['a','b','independent']);
 assert.equal(mobileGroups(groups,nodes,'absent').length,0);
});
test('failed-only retry freezes original targets and rejects changed profile instance revision and terminal cancellation',()=>{
 const job={scope,kind:'outbound',state:'partially_failed',targetTags:['a','b','c','b'],results:[{targetTag:'a',reachable:true},{targetTag:'b',reachable:false}]};
 assert.deepEqual(failedProbeTargets(job,scope),['b','c']);
 for(const changed of [{...scope,profileId:'other'},{...scope,configRevision:8},{...scope,coreInstanceId:4}]) assert.deepEqual(failedProbeTargets(job,changed),[]);
 for(const state of ['running','cancelled','completed','invalidated_by_core_restart']) assert.deepEqual(failedProbeTargets({...job,state},scope),[]);
});

test('configuration preconditions retain decimal u64 and reject unsafe numeric legacy wire',()=>{
 assert.equal(controlScope({...scope,configRevision:'18446744073709551615'}).configRevision,'18446744073709551615');
 assert(sameProbeScope(scope,{...scope,configRevision:'7'}));
 assert.throws(()=>controlScope({...scope,configRevision:9007199254740992}));
 assert.throws(()=>controlScope({...scope,configRevision:'18446744073709551616'}));
 assert.throws(()=>controlScope({...scope,configRevision:'01'}));
});

test('late start and historical job events cannot hide latest confirmed partial result',()=>{
 const terminal={id:7,state:'partially_failed',startedAtUnixMs:100,updatedAtUnixMs:200};
 assert.equal(latestProbeJob(terminal,{...terminal,state:'running',updatedAtUnixMs:200}),terminal);
 assert.equal(latestProbeJob(terminal,{...terminal,updatedAtUnixMs:199}),terminal);
 assert.equal(latestProbeJob(terminal,{...terminal,id:6,startedAtUnixMs:99}),terminal);
 const next={...terminal,id:8,startedAtUnixMs:201};assert.equal(latestProbeJob(terminal,next),next);
});
