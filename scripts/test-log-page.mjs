import assert from 'node:assert/strict';
import { mergeLogPage } from '../src/lib/services/log-page.ts';

function entry(id, message) {
  return {
    id,
    source: 'app',
    level: 'info',
    message,
    fields: null,
    occurredAtUnixMs: id,
  };
}

{
  const merged = mergeLogPage([], {
    items: [
      entry(10627, 'before'),
      entry(10628, 'stale duplicate'),
      entry(10629, 'after'),
      entry(10628, 'latest duplicate'),
    ],
    hasMore: false,
    oldestAvailableId: 10627,
  });

  assert.deepEqual(
    merged.map(({ id, message }) => ({ id, message })),
    [
      { id: 10627, message: 'before' },
      { id: 10628, message: 'latest duplicate' },
      { id: 10629, message: 'after' },
    ],
    'initial pages must be deduplicated before reaching a keyed each block',
  );
}

{
  const merged = mergeLogPage(
    [entry(1, 'rotated away'), entry(2, 'existing'), entry(3, 'old value')],
    {
      items: [entry(3, 'refreshed value'), entry(4, 'new')],
      hasMore: false,
      oldestAvailableId: 2,
    },
  );

  assert.deepEqual(
    merged.map(({ id, message }) => ({ id, message })),
    [
      { id: 2, message: 'existing' },
      { id: 3, message: 'refreshed value' },
      { id: 4, message: 'new' },
    ],
  );
}

console.log('log page tests passed');
