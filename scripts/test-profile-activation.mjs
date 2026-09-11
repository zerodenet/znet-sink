import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ProfileActivation } from '../src/lib/features/configuration/profile-activation.ts';
const deferred=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};};
function harness() {
  const state={calls:[],failure:null,effectFailure:null,commitGate:null,entered:deferred(),warns:[]};
  const ports={
    load:async id=>{state.calls.push(`load:${id}`);return {id};},
    commit:async id=>{state.calls.push(`commit:${id}`);state.entered.resolve();if(state.commitGate)await state.commitGate;if(state.failure)throw state.failure;return {id};},
    restartObservation:async()=>{state.calls.push('observe');if(state.effectFailure==='observe')throw new Error('channel closed');},
    changed:()=>{state.calls.push('changed');},
    reconcile:async()=>{state.calls.push('reconcile');if(state.effectFailure==='reconcile')throw new Error('TUN read failed');},
    warn:(context,error)=>state.warns.push([context,error]),
  };
  return {state,ports,activation:new ProfileActivation(ports)};
}
test('concurrent activations serialize source loading through the last post-commit effect',async()=>{
  const h=harness();const gate=deferred();h.state.commitGate=gate.promise;
  const first=h.activation.activate('a');const second=h.activation.activate('b');
  await h.state.entered.promise;
  assert.deepEqual(h.state.calls,['load:a','commit:a']);
  gate.resolve();await Promise.all([first,second]);
  assert.deepEqual(h.state.calls,['load:a','commit:a','observe','changed','reconcile','load:b','commit:b','observe','changed','reconcile']);
});
test('rejected activation leaves the client-owned TUN alone and does not publish success',async()=>{
  const h=harness();h.state.failure={code:'core_error'};
  await assert.rejects(h.activation.activate('a'),error=>error===h.state.failure);
  assert.deepEqual(h.state.calls,['load:a','commit:a']);
});
test('unknown application and identity conflict observe without replay or TUN rollback',async()=>{
  for(const failure of [{code:'config_apply_uncertain'},{code:'conflict',details:{resource:'config',id:'runtime'}}]) {
    const h=harness();h.state.failure=failure;
    await assert.rejects(h.activation.activate('a'),error=>error===failure);
    assert.deepEqual(h.state.calls,['load:a','commit:a','observe']);
  }
});
test('post-commit observation or TUN failure never misreports the committed profile as rejected',async()=>{
  for(const effect of ['observe','reconcile']) {
    const h=harness();h.state.effectFailure=effect;
    assert.deepEqual(await h.activation.activate('a'),{id:'a'});
    assert.ok(h.state.calls.includes('changed'));assert.ok(h.state.calls.includes('reconcile'));
    assert.equal(h.state.calls.some(c=>c.startsWith('restore')),false);assert.equal(h.state.warns.length,1);
  }
});
test('failed observation retains the original rejection and does not poison the next activation',async()=>{
  const h=harness();const original={code:'config_apply_uncertain'};h.state.failure=original;
  h.state.effectFailure='observe';
  await assert.rejects(h.activation.activate('a'),error=>error===original);
  h.state.failure=null;h.state.effectFailure=null;
  assert.deepEqual(await h.activation.activate('b'),{id:'b'});
  assert.equal(h.state.warns.length,1);
});
test('a failure before commit never invokes backend application or rollback',async()=>{
  const h=harness();h.ports.load=async()=>{throw new Error('profile missing');};
  await assert.rejects(h.activation.activate('missing'),/profile missing/);
  assert.deepEqual(h.state.calls,[]);
});

test('a lost frontend invoke response cannot trigger rollback of an already committed backend',async()=>{
  for(const failure of [new Error('channel closed'), 'invoke response lost']) {
    const h=harness();h.state.failure=failure;
    await assert.rejects(h.activation.activate('a'),error=>error===failure);
    assert.deepEqual(h.state.calls,['load:a','commit:a','observe']);
  }
});
