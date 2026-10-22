import assert from 'node:assert/strict';
import { test } from 'node:test';
import { guiExportDiagnostics, guiDiscardDiagnostics } from '../src/lib/services/core.ts';
import { previewNativeDiagnostics } from '../src/lib/services/native-diagnostics.ts';

test('desktop delivery consumes only the host-prepared plan ID and discards only original pending previews', async () => {
  const calls = [];
  globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => {
    calls.push({ command, args });
    return command === 'native_document_preview_diagnostics' ? { planId: 'original-host-plan' } : undefined;
  } } };
  try {
    const original = await previewNativeDiagnostics();
    await guiExportDiagnostics(original.planId);
    await guiDiscardDiagnostics('cancelled-original-plan');
    assert.deepEqual(calls, [
      { command: 'native_document_preview_diagnostics', args: {} },
      { command: 'gui_export_diagnostics', args: { planId: 'original-host-plan' } },
      { command: 'gui_discard_diagnostics', args: { planId: 'cancelled-original-plan' } },
    ]);
  } finally { delete globalThis.window; }
});

test('failed desktop delivery neither replays nor changes to an unpreviewed raw export operation', async () => {
  const calls = [];
  globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => {
    calls.push({ command, args });
    throw new Error('unknown delivery');
  } } };
  try {
    await assert.rejects(guiExportDiagnostics('fixed-once'), /unknown delivery/);
    assert.deepEqual(calls, [{ command: 'gui_export_diagnostics', args: { planId: 'fixed-once' } }]);
  } finally { delete globalThis.window; }
});
