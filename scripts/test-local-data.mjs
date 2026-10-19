import {test} from 'node:test';
import assert from 'node:assert/strict';
import {decodeLocalData, storageAge} from '../src/lib/features/mobile/local-data.ts';
const sample = () => ({sampledAtUnixMs:'10', stores:['logs','history','capture'].map(kind => ({
  kind,state:'sampled',bytes:'18446744073709551615',files:1,byteLimit:null,recordLimit:5000,
  maxAgeMs:kind === 'history' ? '2592000000' : null,enforcement:'append-rotation',path:'SECRET_PATH',error:'SECRET_TOKEN'}))});
test('storage facts preserve exact decimal bytes, real zero and unavailable while dropping free fields',()=>{
  const value=sample();value.stores[0].bytes='0';Object.assign(value.stores[2],{state:'unavailable',bytes:null,files:null});
  const result=decodeLocalData(value);
  assert.equal(result.stores[0].bytes,'0');assert.equal(result.stores[1].bytes,'18446744073709551615');assert.equal(result.stores[2].bytes,null);
  assert.ok(!JSON.stringify(result).includes('SECRET'));
  assert.equal(storageAge(result.stores[1].maxAgeMs),'30 天');assert.equal(storageAge(null),'未设置固定时长');
});
test('duplicate scopes, fabricated zero for failed reads and unsafe metrics cannot pass as sampled capacity',()=>{
  for(const patch of [row=>row.bytes=18446744073709552000,row=>row.bytes='18446744073709551616',row=>row.files=-1,
    row=>row.state='healthy',row=>row.enforcement='guaranteed',row=>row.recordLimit=Infinity,row=>row.byteLimit=5,
    row=>row.kind='all',row=>row.kind='history',row=>row.state='unavailable']){
    const value=sample();patch(value.stores[0]);assert.throws(()=>decodeLocalData(value));
  }
  const value=sample();value.stores.pop();assert.throws(()=>decodeLocalData(value));
});
