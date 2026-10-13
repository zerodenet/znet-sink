import assert from 'node:assert/strict';
import { test } from 'node:test';
import { documentReceiptMessage } from '../src/lib/services/native-documents.ts';
const receipt = status => ({ operationId:'fixed-operation',status,content:null,bytesRead:null,reason:null });
test('share acceptance never says recipient saved or operation completed', () => {
  const message=documentReceiptMessage(receipt('share-accepted'));
  assert.match(message,/未确认/); assert.match(message,/分享面板/); assert.doesNotMatch(message,/保存成功|已完成/);
});
test('cancel and unknown outcomes retain their distinct recovery instructions', () => {
  assert.match(documentReceiptMessage(receipt('cancelled')),/原有草稿和数据保留/);
  const message=documentReceiptMessage(receipt('result-unknown'));
  assert.match(message,/先检查目标/); assert.match(message,/不会自动重试/);
});
test('only native saved confirmation describes confirmed file write', () => {
  assert.equal(documentReceiptMessage(receipt('saved')),'系统文件写入已确认');
  assert.match(documentReceiptMessage(receipt('read')),/校验后保存/);
  assert.match(documentReceiptMessage(receipt('failed')),/失败/);
});
