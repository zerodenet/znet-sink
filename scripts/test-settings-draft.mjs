import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import ts from 'typescript';
import { settingsDraftKey, captureSettingsDraftRead, canAcceptSettingsDraftRead } from '../src/lib/features/mobile/settings-draft.ts';

test('initial/clean refresh is accepted without treating object order as an edit', () => {
  const source = {dns:{servers:{system:{type:'system'}},default_server:'system'},mode:'real'};
  const reordered = {mode:'real',dns:{default_server:'system',servers:{system:{type:'system'}}}};
  assert.equal(settingsDraftKey(source),settingsDraftKey(reordered));
  assert.ok(canAcceptSettingsDraftRead(captureSettingsDraftRead(null,null),null,null));
  assert.ok(canAcceptSettingsDraftRead(captureSettingsDraftRead(source,source),source,reordered));
});
test('dirty drafts, edits during read and a newer confirmed baseline reject stale refresh', () => {
  const original={url:'https://source.test',tolerance:'50'};
  const edited={...original,url:'https://edited.test'};
  assert.equal(canAcceptSettingsDraftRead(captureSettingsDraftRead(original,edited),original,edited),false);
  const read=captureSettingsDraftRead(original,original);
  assert.equal(canAcceptSettingsDraftRead(read,original,edited),false);
  assert.equal(canAcceptSettingsDraftRead(read,edited,edited),false);
  assert.notEqual(settingsDraftKey({fallback:['a','b']}),settingsDraftKey({fallback:['b','a']}));
});
function navigation() {
  const source=readFileSync(new URL('../src/lib/features/mobile/navigation.svelte.ts',import.meta.url),'utf8');
  const {outputText}=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
  const exports={};new Function('exports','$state',outputText)(exports,value=>value);
  return new exports.MobileNavigation();
}
test('mounted guard registration/disposal never erases another editor guard',()=>{
  const nav=navigation();nav.open('dns');
  let dirty=true, continuation=null;
  nav.guardBeforeLeave(next=>{if(!dirty)return true;continuation=next;return false;});
  const dispose=nav.guardBeforeLeave(()=>true);
  nav.back();assert.equal(nav.current.page,'dns');
  dispose();nav.selectTab('nodes');assert.equal(nav.activeTab,'overview');
  dirty=false;continuation();assert.equal(nav.activeTab,'nodes');
});
test('busy navigation is consumed without later replay; a new explicit request is required',()=>{
  const nav=navigation();nav.open('network');let busy=true;
  nav.guardBeforeLeave(()=>!busy);
  assert.equal(nav.back(),true);assert.equal(nav.current.page,'network');
  busy=false;assert.equal(nav.current.page,'network');
  assert.equal(nav.back(),true);assert.equal(nav.current,undefined);
});
