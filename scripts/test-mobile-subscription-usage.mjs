import assert from 'node:assert/strict';
import {test} from 'node:test';
import {subscriptionUsage,subscriptionFailureSummary} from '../src/lib/features/mobile/subscription-usage.ts';
test('quota without observed usage is unknown instead of zero or healthy percentage',()=>{
 const usage=subscriptionUsage({totalBytes:100});assert.equal(usage.used,null);assert.equal(usage.percent,null);assert.equal(usage.upload,null);
 assert.equal(subscriptionUsage({uploadBytes:0,totalBytes:100}).used,null);
});
test('explicit zeros, complete directions and aggregate provider scope retain their distinct facts',()=>{
 assert.equal(subscriptionUsage({uploadBytes:0,downloadBytes:0,totalBytes:100}).percent,0);
 const aggregate=subscriptionUsage({usedBytes:20,totalBytes:100});assert.equal(aggregate.percent,20);assert.equal(aggregate.upload,null);assert.equal(aggregate.aggregate,true);
 assert.equal(subscriptionUsage({uploadBytes:20,downloadBytes:30,totalBytes:100}).used,50);
});
test('malformed, unsafe and overflowing counters are not rounded into quota facts',()=>{
 for(const value of [-1,1.5,NaN,Infinity,Number.MAX_SAFE_INTEGER+1,'0']){const usage=subscriptionUsage({usedBytes:value,totalBytes:100});assert.equal(usage.used,null);assert.equal(usage.percent,null);assert.equal(usage.invalid,true);}
 const overflow=subscriptionUsage({uploadBytes:Number.MAX_SAFE_INTEGER,downloadBytes:1,totalBytes:100});assert.equal(overflow.used,null);assert.equal(overflow.invalid,true);
});
test('zero quota is not a division or unlimited claim; overage remains explicit',()=>{
 assert.equal(subscriptionUsage({usedBytes:0,totalBytes:0}).percent,null);
 assert.equal(subscriptionUsage({usedBytes:1,totalBytes:0}).overQuota,true);
 const over=subscriptionUsage({usedBytes:120,totalBytes:100});assert.equal(over.percent,100);assert.equal(over.overQuota,true);
});
test('persisted legacy source errors cannot expose credentials; uncertainty stays distinct',()=>{
 for(const error of ['failed https://example.test/sub?token=canary-private', '解析失败 password=canary-private', 'private-key=canary-private 结果未确认'])assert.ok(!subscriptionFailureSummary(error).message.includes('canary-private'));
 assert.equal(subscriptionFailureSummary('订阅材料准备或运行提交结果未确认').unknown,true);
 assert.match(subscriptionFailureSummary('subscription server returned HTTP 304').message,/HTTP 304/);
 assert.equal(subscriptionFailureSummary(null),null);
});
