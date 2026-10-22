import { invoke } from '@tauri-apps/api/core';
import type { MobileReleaseGateway, AndroidUpdateGateway } from '$lib/types/mobile-release';

export const mobileReleaseGateway: MobileReleaseGateway = {
  catalog: (source, channel) => invoke('mobile_release_catalog', { source, channel }),
  schedule: () => invoke('mobile_release_schedule'),
  preferences: () => invoke('mobile_release_preferences'),
  savePreferences: (expectedRevision, preferences, operationId) => invoke('mobile_release_preferences_save', { expectedRevision, preferences, operationId }),
  preferenceOperation: operationId => invoke('mobile_release_preferences_operation', { operationId }),
  confirmPreferences: (operationId, reviewBinding) => invoke('mobile_release_preferences_confirm', { operationId, reviewBinding }),
};

export const androidUpdateGateway: AndroidUpdateGateway = {
  status: () => invoke('mobile_app_update_status'),
  download: (channel, tag, asset, operationId) => invoke('mobile_app_update_download', { channel, tag, assetId: asset.assetId, expectedSha256: asset.sha256, operationId }),
  action: (action, operationId) => invoke('mobile_app_update_action', { action, operationId }),
};
