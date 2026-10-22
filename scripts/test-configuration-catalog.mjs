import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
function load(file, deps = {}) {
  const {outputText} = ts.transpileModule(readFileSync(file, 'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
  const exports = {};
  new Function('require','exports', outputText)(name => { assert.ok(name in deps, name); return deps[name]; }, exports);
  return exports;
}
const model = load('src/lib/features/configuration/catalog.ts', {
  '$lib/features/mobile/subscription-usage': load('src/lib/features/mobile/subscription-usage.ts'),
});
const entry = (extra = {}) => ({id:'profile:original',profileId:'original',name:'配置',active:false,hasContent:true,format:'json',path:null,sources:[],managedSource:null,...extra});
test('source filtering groups legacy shared sources by configuration and finds their plugin names', () => {
  const entries = [entry(),entry({id:'profile:shared',name:'来源配置',sources:[{name:'A'},{name:'B'}]}),
    entry({id:'profile:owned',sources:[{managedSource:{pluginId:'org.example.owner',sourceName:'服务方'}}]})];
  assert.equal(model.filterConfigurations(entries,'subscription','').length,1);
  assert.equal(model.filterConfigurations(entries,'managed','服务方')[0].id,'profile:owned');
  assert.equal(model.filterConfigurations(entries,'local','')[0].profileId,'original');
});
test('provider aggregate quota is not fabricated as upload/download and missing usage remains unknown', () => {
  assert.equal(model.subscriptionUsedBytes({usedBytes:375,uploadBytes:100,downloadBytes:200}),375);
  assert.equal(model.subscriptionUsedBytes({}),null);
  assert.equal(model.subscriptionUsedBytes({uploadBytes:0,downloadBytes:0}),0);
  assert.equal(model.subscriptionUsedBytes({uploadBytes:1}),null);
  assert.equal(model.subscriptionUsedBytes({usedBytes:Number.MAX_SAFE_INTEGER+1}),null);
});
test('only usable configurations or enabled manual pending sources can be selected', () => {
  const choices = model.configurationOptions([entry(),entry({id:'subscription:pending',profileId:null,hasContent:false,sources:[{enabled:true}]}),
    entry({id:'subscription:owned',hasContent:false,sources:[{enabled:true,managedSource:{pluginId:'owner'}}]})]);
  assert.deepEqual(choices.map(option => option.disabled),[false,false,true]);
});
function serviceHarness(failSync = false) {
  const calls=[];
  const service = load('src/lib/services/configuration-catalog.ts', {
    '@tauri-apps/api/core':{invoke:async command=>{calls.push(command);return[];}},
    './proxy-config-signal.svelte':{proxyConfigSignal:{revision:0}},
    './config':{
      getProxyConfig:async id=>{calls.push(['get',id]);return{id};},
      setActiveProxyConfig:async id=>{calls.push(['activate',id]);return{id};},
      syncSubscription:async id=>{calls.push(['sync',id]);if(failSync)throw new Error('rejected');return{targetProxyConfigId:'generated'};},
    },
  }); return{service,calls};
}
test('both modes activate existing identity without updating its source',async()=>{
  const h=serviceHarness();await h.service.activateConfiguration(entry());
  assert.deepEqual(h.calls,[['activate','original']]);
});
test('pending sources sync once before explicit activation; failures never activate',async()=>{
  const pending=entry({hasContent:false,profileId:null,sources:[{id:'s',enabled:true}]});
  const h=serviceHarness();await h.service.activateConfiguration(pending);
  assert.deepEqual(h.calls,[['sync','s'],['activate','generated']]);
  const rejected=serviceHarness(true);await assert.rejects(rejected.service.activateConfiguration(pending),/rejected/);
  assert.deepEqual(rejected.calls,[['sync','s']]);
});
test('managed pending content cannot bypass plugin ownership or use ordinary URL sync',async()=>{
  const h=serviceHarness();await assert.rejects(h.service.activateConfiguration(entry({hasContent:false,sources:[{id:'owned',enabled:true,managedSource:{pluginId:'owner'}}]})),/来源插件/);
  assert.deepEqual(h.calls,[]);
});

test('a confirmed activation survives failed follow-up reads and folds a pending identity once',()=>{
  const source={id:'s',name:'original source',policySelections:{Proxy:'chosen'}};
  const pending=entry({id:'subscription:s',profileId:null,hasContent:false,sources:[source]});
  const committed={id:'new',name:'new profile',content:{},format:'json',active:true,updatedAtUnixMs:10};
  const next=model.acceptActivatedConfiguration([entry({active:true}),pending],pending.id,committed);
  assert.equal(next.filter(item=>item.active).length,1);
  assert.equal(next.find(item=>item.active).profileId,'new');
  assert.equal(next.find(item=>item.active).sources[0].policySelections.Proxy,'chosen');
  assert.equal(source.targetProxyConfigId,undefined,'do not rewrite a caller-owned snapshot');
  const repeated=model.acceptActivatedConfiguration(next,pending.id,committed);
  assert.equal(repeated.filter(item=>item.profileId==='new').length,1);
});
