import assert from 'node:assert/strict';
import {
  serializeDebugFrameForClipboard,
  serializeDebugFramesForClipboard,
  serializeLogForClipboard,
  serializeLogsForClipboard,
} from '../src/lib/services/diagnostic-copy.ts';

const longMessage = `copy-me-${'x'.repeat(8_192)}`;
const log = {
  id: 10,
  source: 'app',
  level: 'error',
  message: 'fallback',
  occurredAtUnixMs: 1_784_313_264_649,
  fields: {
    message: longMessage,
    raw_line: longMessage,
    operation: 'notification.show',
  },
};

const copiedLog = JSON.parse(serializeLogForClipboard(log));
assert.equal(copiedLog.message, longMessage);
assert.equal(copiedLog.fields.raw_line, longMessage);

const copiedLogs = JSON.parse(serializeLogsForClipboard([log], {
  source: 'app',
  minLevel: 'error',
  hasMore: true,
  copiedAtUnixMs: 123,
}));
assert.equal(copiedLogs.schemaId, 'znet.clipboard.logs.v1');
assert.equal(copiedLogs.partial, true);
assert.equal(copiedLogs.items[0].message, longMessage);

const frame = {
  id: 272,
  atMs: 1_784_313_264_649,
  direction: 'rx',
  frameType: 'event',
  elapsedMs: 1,
  payload: { nested: { message: longMessage } },
};

const copiedFrame = JSON.parse(serializeDebugFrameForClipboard(frame));
assert.equal(copiedFrame.payload.nested.message, longMessage);

const copiedFrames = JSON.parse(serializeDebugFramesForClipboard([frame], {
  frameType: 'event',
  hasMore: false,
  copiedAtUnixMs: 456,
}));
assert.equal(copiedFrames.schemaId, 'znet.clipboard.ipc-debug.v1');
assert.equal(copiedFrames.items[0].id, 272);

console.log('diagnostic-copy: ok');
