import type { guiState } from './gui-state.svelte';

type TrayState = Pick<typeof guiState,
  'connection' | 'connectionError' | 'isCaptureEnabled' | 'isTunDesiredEnabled'
  | 'isSystemProxyEnabled' | 'isProcessRunning' | 'hasUnmanagedSystemProxy'
  | 'canResetSystemProxy' | 'refreshConnectionStatus' | 'openSystemProxyRecovery' | 'syncTrayStatus'
  | 'disconnect' | 'disableSystemProxy' | 'startCore' | 'connect'
  | 'enableSystemProxy' | 'toggleSystemProxy' | 'toggleTun' | 'restartCore'>;

export async function handleTrayAction(state: TrayState, uiMode: string, action?: string) {
  try {
    if (action === 'recover_system_proxy' || action === 'toggle_proxy' || action === 'toggle_system_proxy') {
      if (!state.canResetSystemProxy) return;
      // An open native menu can outlive the snapshot used for its labels.
      await state.refreshConnectionStatus();
      if (!state.canResetSystemProxy) return;
      if (action === 'recover_system_proxy' || state.hasUnmanagedSystemProxy
        || state.connectionError || !state.connection) {
        state.openSystemProxyRecovery();
        return;
      }
    }
    switch (action) {
      case 'toggle_proxy':
        if (uiMode === 'lite') {
          if (state.isCaptureEnabled || state.isTunDesiredEnabled) {
            await state.disconnect();
            return;
          }
        } else if (state.isSystemProxyEnabled) {
          await state.disableSystemProxy();
          return;
        }
        // Starting the kernel is part of enabling capture, never disabling it.
        if (!state.isProcessRunning) {
          await state.startCore();
          if (!state.isProcessRunning) return;
        }
        if (uiMode === 'lite') await state.connect();
        else await state.enableSystemProxy();
        break;
      case 'toggle_system_proxy': await state.toggleSystemProxy(); break;
      case 'toggle_tun': await state.toggleTun(); break;
      case 'restart_core': await state.restartCore(); break;
    }
  } finally {
    // Native check items toggle before dispatch, even if the command is blocked.
    state.syncTrayStatus();
  }
}
