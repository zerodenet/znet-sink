import assert from 'node:assert/strict';
import { test } from 'node:test';
import { subscriptionScheduling } from '../src/lib/features/mobile/subscription-scheduling.ts';

test('saved mobile interval never becomes a promised task or automatic completion', () => {
  const text = subscriptionScheduling({ enabled: true, updateIntervalSecs: 3600 }, 'mobile-unadapted');
  assert.match(text, /3600 秒周期意图/);
  assert.match(text, /下一次自动执行未安排/);
  assert.match(text, /不代表自动任务已经执行/);
});
test('manual sources work without any auto-sync intent', () => {
  assert.match(subscriptionScheduling({ enabled: true }, 'mobile-unadapted'), /仅手动同步/);
});
test('disabled schedule retains cache and does not imply resource deletion', () => {
  assert.match(subscriptionScheduling({ enabled: false, updateIntervalSecs: 3600 }, 'mobile-unadapted'), /已有缓存仍保留/);
});
test('unknown platform state is not inferred from the narrow layout', () => {
  assert.match(subscriptionScheduling({ enabled: true, updateIntervalSecs: 3600 }, 'unknown'), /平台调度状态未确认/);
  assert.equal(subscriptionScheduling({ enabled: true, updateIntervalSecs: 3600 }, 'desktop'), null);
});
test('managed sources never reuse ordinary URL scheduling claims', () => {
  assert.match(subscriptionScheduling({ enabled: true, managedSource: {} }, 'mobile-unadapted'), /来源插件管理/);
});
test('unsafe or malformed interval is not truncated to an executable intent', () => {
  for (const value of [-1, 0, 0.1, Number.MAX_SAFE_INTEGER + 1, NaN]) {
    assert.match(subscriptionScheduling({ enabled: true, updateIntervalSecs: value }, 'mobile-unadapted'), /无法确认/);
  }
});
