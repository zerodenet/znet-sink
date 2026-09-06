import assert from 'node:assert/strict';
import {test} from 'node:test';
import {logWindow} from '../src/lib/services/log-window.ts';
import {networkLocation} from '../src/lib/services/network-location.ts';
import {describeUiError} from '../src/lib/services/ui-error.ts';

test('log windows bound DOM work while covering expanded rows, boundaries and all history', () => {
  const ids = Array.from({length:5000},(_,i)=>i+1);
  const heights = new Map([[1,900],[10,73]]);
  for (const scrollTop of [0,100,899,900,30000,179999]) {
    const result=logWindow(ids,heights,scrollTop,500);
    assert.ok(result.end-result.start<30);
    assert.ok(result.top<=scrollTop);
    assert.ok(result.offsets[result.end]>=Math.min(scrollTop+500,result.offsets.at(-1)));
    assert.equal(result.top+result.offsets[result.end]-result.offsets[result.start]+result.bottom,result.offsets.at(-1));
  }
  const empty=logWindow([],new Map(),0,500);
  assert.equal(empty.start,0);assert.equal(empty.end,0);assert.equal(empty.bottom,0);
  assert.equal(logWindow(ids,heights,999999,500).end,ids.length);
});

test('network geography supports country names, ISO codes and honest unknown fallbacks', () => {
  for (const country of ['US','us','United States','美国']) assert.deepEqual(networkLocation({country,region:'California',city:'Los Angeles'}),{countryCode:'us',location:'美国 · California · Los Angeles'});
  assert.equal(networkLocation({country:'SG',city:'新加坡'}).location,'新加坡');
  assert.equal(networkLocation({country:'香港'}).countryCode,'hk');
  assert.deepEqual(networkLocation(),{countryCode:undefined,location:'地区未知'});
  assert.equal(networkLocation({country:'AA'}).countryCode,undefined);
  assert.equal(networkLocation({country:'未知地区'}).location,'未知地区');
});

test('UI diagnostics preserve cross-realm exceptions and opaque failures without inventing causes', () => {
  assert.deepEqual(describeUiError({message:'duplicate key',name:'Error',stack:'render at NodesTab:12'}),{message:'duplicate key',name:'Error',stack:'render at NodesTab:12'});
  assert.equal(describeUiError(null,'Script error.').message,'Script error.');
  assert.equal(describeUiError('request failed').message,'request failed');
});
