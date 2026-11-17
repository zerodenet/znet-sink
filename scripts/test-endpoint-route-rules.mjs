import {test} from 'node:test';
import assert from 'node:assert/strict';
import {endpointRuleLines} from '../src/lib/features/routing/endpoint-rules.ts';
test('batch input normalizes single IPs and deduplicates lines',()=>{
 assert.deepEqual(endpointRuleLines('192.168.1.235\n192.168.1.235\n16.0.0.0/8','ipv4_cidr'),[{type:'ipv4_cidr',value:'192.168.1.235/32'},{type:'ipv4_cidr',value:'16.0.0.0/8'}]);
 assert.deepEqual(endpointRuleLines('fd00::1','ipv6_cidr'),[{type:'ipv6_cidr',value:'fd00::1/128'}]);
});
test('domain normalization preserves suffix versus exact intent',()=>{
 assert.deepEqual(endpointRuleLines(' Starmerx.COM \n','domain_suffix'),[{type:'domain_suffix',value:'starmerx.com'}]);
 assert.equal(endpointRuleLines('vending.starmerx.com','domain_exact')[0].type,'domain_exact');
 assert.throws(()=>endpointRuleLines('\n','domain_suffix'));
});
