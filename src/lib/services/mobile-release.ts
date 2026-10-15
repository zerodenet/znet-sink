import { invoke } from '@tauri-apps/api/core';
import type { MobileReleaseGateway } from '$lib/types/mobile-release';

export const mobileReleaseGateway: MobileReleaseGateway = {
  catalog: (source, channel) => invoke('mobile_release_catalog', { source, channel }),
  schedule: () => invoke('mobile_release_schedule'),
  preferences: () => invoke('mobile_release_preferences'),
  savePreferences: (expectedRevision, preferences, operationId) => invoke('mobile_release_preferences_save', { expectedRevision, preferences, operationId }),
  preferenceOperation: operationId => invoke('mobile_release_preferences_operation', { operationId }),
  confirmPreferences: (operationId, reviewBinding) => invoke('mobile_release_preferences_confirm', { operationId, reviewBinding }),
};
