import assert from 'node:assert/strict';
import {test} from 'node:test';
import {parseRuntimeInspection} from '../src/lib/features/mobile/runtime-inspection.ts';
function receipt(){return {status:'confirmed',transport:'pinned-ipc',coreInstanceId:'actual-instance',configRevision:'9007199254740993',vpnObservation:'unprovided',receipt:{operationId:'original-read',profileId:'profile',coreInstanceId:'actual-instance',configRevision:'9007199254740993',state:'confirmed',mutation:false}};}
test('actual runtime revision stays lossless and never claims VPN forwarding',()=>{const result=parseRuntimeInspection(receipt());assert.equal(result.configRevision,'9007199254740993');assert.equal(result.vpnObservation,'unprovided');});
test('wrong identity, unknown receipt and mutation cannot be displayed as confirmed read',()=>{for(const patch of [{coreInstanceId:'old'},{configRevision:'1'},{state:'unknown'},{mutation:true}]){const result=receipt();Object.assign(result.receipt,patch);assert.throws(()=>parseRuntimeInspection(result));}});
test('numeric or invalid revision is refused instead of truncated',()=>{for(const revision of [9007199254740992,'-1','1e3','01','18446744073709551616',null]){const result=receipt();result.configRevision=revision;result.receipt.configRevision=revision;assert.throws(()=>parseRuntimeInspection(result));}});
