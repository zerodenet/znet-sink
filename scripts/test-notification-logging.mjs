import assert from 'node:assert/strict';
import {
  buildNotificationLogInput,
  notificationLogLevel,
} from '../src/lib/services/notification-log.ts';

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

console.log('notification-logging: ok');
