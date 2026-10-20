// Policy assertions over controlled workflow blocks; GitHub remains the YAML/expression executor.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import test from 'node:test';
const read = name => readFileSync(new URL(`../.github/workflows/${name}`,import.meta.url),'utf8');
function job(source,id) {
  const result=source.match(new RegExp(`^  ${id}:\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:|$(?![\\s\\S]))`,'m'));
  assert(result,`missing original check: ${id}`);
  return result[1];
}
const manual = {
  'android-debug-host.yml':['debug-arm64-host'],
  'android-unsigned-host.yml':['unsigned-arm64-host'],
  'ios-app-host.yml':['application'],
  'mobile-host.yml':['shared-mobile-targets','ios-embedded-runtime','android-rust-host'],
  'native-documents.yml':['android-keystore-runtime','android-library','ios-unsigned-library'],
};
test('mobile checks preserve their events and identities, with explicit manual-only job admission',()=>{
  for(const [name,ids] of Object.entries(manual)) {
    const source=read(name);
    const events=source.match(/^on:\n([\s\S]*?)(?=^\S)/m)?.[1];
    assert(events,`${name}: missing event declaration`);
    for(const event of ['pull_request','push','workflow_dispatch'])assert.match(events,new RegExp(`^  ${event}:`,'m'));
    for(const id of ids) {
      const body=job(source,id);
      assert.match(body,/^    name: .+/m);
      assert.match(body,/^    if: github\.event_name == 'workflow_dispatch'$/m);
      assert.doesNotMatch(body,/continue-on-error:\s*true/);
    }
  }
});
test('routine host policies and real desktop/browser gates remain executable and strict',()=>{
  for(const [name,id] of [['mobile-host.yml','embedded-runtime'],['native-documents.yml','jvm-policy'],['native-documents.yml','swift-policy']]) {
    const body=job(read(name),id);
    assert.doesNotMatch(body,/^    if:/m);
    assert.doesNotMatch(body,/continue-on-error:\s*true/);
    assert.doesNotMatch(body,/sdkmanager|android-emulator-runner|xcodebuild|tauri (?:ios|android) build/);
  }
  const gate=job(read('ci.yml'),'ui-controls');
  assert.match(gate,/needs: ui-browser/);
  assert.match(gate,/needs\.ui-browser\.result.* = success/);
  assert.doesNotMatch(gate,/needs:.*(?:android|ios|mobile|native)/);
  const ci=read('ci.yml');
  for(const id of ['shared-client-core','product-composition','frontend','ui-browser','windows-client-core','windows-zero-dns-contract','download-contracts']) {
    const body=job(ci,id);
    assert.doesNotMatch(body,/^    if:.*workflow_dispatch/m);
  }
});
