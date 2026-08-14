import assert from 'node:assert/strict';
import {
  getConfigEditorErrorMessage,
  normalizeConfigValidationError,
  normalizeConfigValidationResponse,
} from '../src/lib/services/config-validation.ts';

assert.deepEqual(
  normalizeConfigValidationResponse({
    accepted: true,
    result: { valid: true },
  }),
  { valid: true, errors: [] },
  'current Zero config.validate success envelope should be accepted',
);

assert.deepEqual(
  normalizeConfigValidationResponse({
    ok: true,
    result: {
      accepted: true,
      result: { valid: true },
    },
  }),
  { valid: true, errors: [] },
  'an additional transport envelope must not turn a valid result into a failure',
);

const zeroValidationError = {
  ok: false,
  error: {
    code: 'invalid_argument',
    message: 'config validation failed',
    details: [
      {
        field_path: 'outbounds[0]',
        message: 'duplicate outbound tag `proxy`',
      },
    ],
  },
};

assert.deepEqual(
  normalizeConfigValidationResponse(zeroValidationError),
  {
    valid: false,
    errors: [
      {
        fieldPath: 'outbounds[0]',
        message: 'duplicate outbound tag `proxy`',
      },
    ],
  },
  'resolved validation errors should preserve field-level diagnostics',
);

assert.deepEqual(
  normalizeConfigValidationError({
    code: 'core_error',
    message: 'config validation failed',
    details: zeroValidationError,
  }),
  [
    {
      fieldPath: 'outbounds[0]',
      message: 'duplicate outbound tag `proxy`',
    },
  ],
  'rejected Tauri invokes should unwrap AppError.details instead of stringifying the object',
);

const objectMessage = getConfigEditorErrorMessage(
  { message: { code: 'bad_config', detail: 'invalid value' } },
  'fallback',
);
assert.notEqual(objectMessage, '[object Object]');
assert.match(objectMessage, /bad_config|invalid value/);

console.log('config-validation: ok');
