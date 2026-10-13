import { invoke } from '@tauri-apps/api/core';
import type { MobileReleaseGateway } from '$lib/types/mobile-release';

export const mobileReleaseGateway: MobileReleaseGateway = {
  catalog: (source, channel) => invoke('mobile_release_catalog', { source, channel }),
  preferences: () => invoke('mobile_release_preferences'),
  savePreferences: (expectedRevision, preferences) => invoke('mobile_release_preferences_save', { expectedRevision, preferences }),
};
