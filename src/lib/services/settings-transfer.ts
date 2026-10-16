import { invoke } from '@tauri-apps/api/core';
import type { SettingsTransferExport, SettingsTransferGateway, SettingsTransferPreview, SettingsTransferReceipt } from '$lib/types/settings-transfer';
export const settingsTransferGateway: SettingsTransferGateway = {
  export: () => invoke<SettingsTransferExport>('settings_transfer_export'),
  preview: content => invoke<SettingsTransferPreview>('settings_transfer_preview', { content }),
  commit: planId => invoke<SettingsTransferReceipt>('settings_transfer_commit', { planId }),
  status: planId => invoke<SettingsTransferReceipt>('settings_transfer_status', { planId }),
};
