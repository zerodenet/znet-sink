import assert from 'node:assert/strict';
import { isKernelUpdate, newestStableKernel } from '../src/lib/services/kernel-version-policy.ts';
assert.equal(isKernelUpdate('0.0.17-rc.1', '0.0.16'), false);
assert.equal(isKernelUpdate('0.0.17-rc.1', '0.0.17'), true);
assert.equal(isKernelUpdate('v0.0.17', '0.0.17+build.2'), false);
assert.equal(isKernelUpdate('dev', '0.0.17'), false);
assert.equal(isKernelUpdate('0.0.17', null), false);
assert.equal(newestStableKernel([
  { version:'0.0.9', channel:'stable', publishedAtUnixMs:999 },
  { version:'0.0.10', channel:'stable', publishedAtUnixMs:1 },
  { version:'0.0.11-rc.1', channel:'beta', publishedAtUnixMs:1000 },
]).version, '0.0.10');
console.log('kernel version policy tests passed');
