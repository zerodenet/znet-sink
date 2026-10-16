import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import ts from 'typescript';

function navigation() {
  const source=readFileSync(new URL('../src/lib/features/mobile/navigation.svelte.ts',import.meta.url),'utf8');
  const {outputText}=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
  const exports={};new Function('exports','$state',outputText)(exports,value=>value);
  return new exports.MobileNavigation();
}
const binding=()=>({binding:'opaque-host-session-and-scope',profileId:'profile-a',configRevision:'18446744073709551615',coreInstanceId:'9007199254740993',scopeGeneration:'2'});
const profile=()=>({kind:'profile',profileId:'profile-a',binding:binding()});
const nodes=()=>({kind:'nodes',binding:binding(),selection:{kind:'global',rootTag:'selector-a'}});

test('cross-tab visit preserves destination root and every older secondary route',()=>{
  const nav=navigation();nav.selectTab('configuration');nav.open('dns');nav.open('workspace');
  const previous=nav.stacks.configuration.slice();nav.selectTab('overview');
  assert.equal(nav.visit('configuration','profiles',profile()),true);
  assert.equal(nav.activeTab,'configuration');assert.equal(nav.current.page,'profiles');
  assert.deepEqual(nav.current.origin,{tab:'overview',key:null});
  assert.deepEqual(nav.stacks.configuration.slice(0,-1),previous);
  assert.equal(nav.back(),true);assert.equal(nav.activeTab,'overview');assert.deepEqual(nav.stacks.configuration,previous);
});
test('guard runs before both tab and stack change and continuation retains a fixed copy',()=>{
  const nav=navigation();let dirty=true,next;const target=nodes();
  nav.guardBeforeLeave(continuation=>{if(!dirty)return true;next=continuation;return false;});
  assert.equal(nav.visit('nodes','nodes',target),false);assert.equal(nav.activeTab,'overview');assert.equal(nav.stacks.nodes.length,0);
  target.binding.profileId='other';target.selection.rootTag='later';dirty=false;next();
  assert.equal(nav.activeTab,'nodes');assert.equal(nav.current.locationTarget.binding.profileId,'profile-a');
  assert.equal(nav.current.locationTarget.selection.rootTag,'selector-a');
});
test('busy rejection creates no route or automatic transition when busy ends',()=>{
  const nav=navigation();let busy=true;nav.guardBeforeLeave(()=>!busy);
  assert.equal(nav.visit('nodes','nodes',nodes()),false);busy=false;
  assert.equal(nav.activeTab,'overview');assert.equal(nav.stacks.nodes.length,0);
  assert.equal(nav.visit('nodes','nodes',nodes()),true);
});
test('nested pages return to the visit before the visit returns to the original source',()=>{
  const nav=navigation();nav.selectTab('more');nav.open('settings');const origin=nav.current;
  nav.visit('nodes','nodes',nodes());const visit=nav.current;nav.open('probes');
  nav.back();assert.equal(nav.activeTab,'nodes');assert.equal(nav.current,visit);
  nav.back();assert.equal(nav.activeTab,'more');assert.equal(nav.current,origin);
});
test('a changed source route is retained and explained instead of discarding its new page',()=>{
  const nav=navigation();nav.visit('configuration','profiles',profile());
  nav.selectTab('overview');nav.open('traffic');const latest=nav.current;
  nav.selectTab('configuration');nav.back();assert.equal(nav.activeTab,'overview');assert.equal(nav.current,latest);
  assert.match(nav.returnNotice,/返回来源已变化/);assert.equal(nav.stacks.overview.length,1);
});
test('a delayed discard callback cannot navigate after an unseen leave-and-return ABA',()=>{
  const nav=navigation();let dirty=true,next;
  const stop=nav.guardBeforeLeave(continuation=>{if(!dirty)return true;next=continuation;return false;});
  nav.visit('nodes','nodes',nodes());stop();nav.selectTab('more');nav.selectTab('overview');dirty=false;next();
  assert.equal(nav.activeTab,'overview');assert.equal(nav.stacks.nodes.length,0);
});
test('a guard cannot redirect the source then let an old visit append to a different navigation',()=>{
  const nav=navigation();let redirected=false;
  nav.guardBeforeLeave(()=>{if(!redirected){redirected=true;nav.selectTab('more');}return true;});
  assert.equal(nav.visit('nodes','nodes',nodes()),false);assert.equal(nav.activeTab,'more');assert.equal(nav.stacks.nodes.length,0);
});
test('open hook observes a single completed visit with target and active destination together',()=>{
  const nav=navigation();const seen=[];nav.onOpen=()=>seen.push({tab:nav.activeTab,page:nav.current.page,target:nav.current.locationTarget});
  nav.visit('nodes','nodes',nodes());assert.equal(seen.length,1);assert.equal(seen[0].tab,'nodes');assert.equal(seen[0].page,'nodes');
  assert.equal(seen[0].target.binding.configRevision,'18446744073709551615');
});
test('no-profile visit is list-only and accepts no fabricated fixed identity',()=>{
  const nav=navigation();assert.equal(nav.visit('configuration','profiles',{kind:'profile',profileId:null,binding:null}),true);
  assert.equal(nav.current.locationTarget.profileId,null);nav.back();
  assert.equal(nav.visit('configuration','profiles',{kind:'profile',profileId:null,binding:binding()}),false);
});
test('unsafe or missing binding and mismatched tab/target fail without mutating navigation',()=>{
  for(const invalid of [
    {...profile(),binding:{...binding(),binding:''}},
    {...profile(),binding:{...binding(),configRevision:9007199254740992}},
    {...profile(),binding:{...binding(),scopeGeneration:'0'}},
    {...profile(),binding:{...binding(),coreInstanceId:'18446744073709551616'}},
    {...profile(),profileId:'replacement'},
  ]){const nav=navigation();assert.equal(nav.visit('configuration','profiles',invalid),false);assert.equal(nav.activeTab,'overview');assert.equal(nav.stacks.configuration.length,0);}
  const nav=navigation();assert.equal(nav.visit('nodes','profiles',profile()),false);assert.equal(nav.visit('nodes','nodes',{...nodes(),binding:null}),false);
});
test('visit return still observes the existing dirty guard and only explicit discard leaves',()=>{
  const nav=navigation();nav.visit('configuration','profiles',profile());let dirty=true,next;
  nav.guardBeforeLeave(continuation=>{if(!dirty)return true;next=continuation;return false;});
  assert.equal(nav.back(),true);assert.equal(nav.activeTab,'configuration');assert.equal(nav.current.page,'profiles');
  dirty=false;next();assert.equal(nav.activeTab,'overview');assert.equal(nav.stacks.configuration.length,0);
});
test('an explicitly unconfirmed outlet remains a read-only directory visit instead of becoming rule mode',()=>{
  const nav=navigation();assert.equal(nav.visit('nodes','nodes',{...nodes(),selection:{kind:'unconfirmed'}}),true);
  assert.deepEqual(nav.current.locationTarget.selection,{kind:'unconfirmed'});
  nav.back();assert.equal(nav.activeTab,'overview');
});
test('unknown selection kinds or non-text global targets are rejected without any navigation',()=>{
  for(const selection of [{kind:'unknown'},{kind:'rule-ish'},null,{kind:'global',rootTag:42}]){
    const nav=navigation();assert.equal(nav.visit('nodes','nodes',{...nodes(),selection}),false);
    assert.equal(nav.activeTab,'overview');assert.equal(nav.stacks.nodes.length,0);
  }
});
