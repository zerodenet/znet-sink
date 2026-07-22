import assert from 'node:assert/strict';
import {
  buildNotificationLogInput,
  notificationLogLevel,
} from '../src/lib/services/notification-log.ts';
import {
  MAX_ACTIVE_TOASTS,
  planToastAdmission,
} from '../src/lib/services/toast-policy.ts';

assert.equal(notificationLogLevel('success'), 'info');
assert.equal(notificationLogLevel('info'), 'info');
assert.equal(notificationLogLevel('warning'), 'warn');
assert.equal(notificationLogLevel('error'), 'error');

const completeMessage = `连接失败：${'详细错误/'.repeat(120)}`;
const input = buildNotificationLogInput({
  id: 7,
  type: 'error',
  message: completeMessage,
  duration: 8_000,
});

assert.equal(input.source, 'app');
assert.equal(input.level, 'error');
assert.equal(input.message, completeMessage);
assert.deepEqual(input.fields, {
  schema: 'znet.notification.v1',
  area: 'ui',
  operation: 'notification.show',
  notificationId: 7,
  notificationType: 'error',
  durationMs: 8_000,
  placement: 'app-header-center',
});

const existing = [
  { id: 1, type: 'info', message: 'one' },
  { id: 2, type: 'warning', message: 'two' },
  { id: 3, type: 'error', message: 'three' },
];
assert.equal(MAX_ACTIVE_TOASTS, 3);
assert.deepEqual(
  planToastAdmission(existing, { type: 'success', message: 'four' }),
  { evictIds: [1] },
);
assert.deepEqual(
  planToastAdmission(existing, { type: 'warning', message: 'two' }),
  { duplicateId: 2, evictIds: [] },
);

console.log('notification-logging: ok');
