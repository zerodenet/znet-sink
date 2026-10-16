export type MobileReleaseSource = 'client' | 'kernel';
export type MobileReleaseChannel = 'stable' | 'preview' | 'test';
export interface ReleasePreferences {
  clientAutoCheck: boolean;
  clientChannel: MobileReleaseChannel;
  kernelChannel: MobileReleaseChannel;
}
export interface ReleasePreferenceOperation {
  operationId: string;
  expectedRevision: string;
  preferences: ReleasePreferences;
  state: 'prepared' | 'unknown' | 'saved-awaiting-confirm' | 'confirmed' | 'rejected';
  acknowledged: boolean;
  reviewBinding: string | null;
  reason: string | null;
  receipt: { ownerOperation: string; beforeEpoch: string; afterEpoch: string } | null;
}
export interface ReleasePreferencesSnapshot {
  preferences: ReleasePreferences;
  expectedRevision: string;
  schedulerState: 'native-foreground' | 'not-native-host' | 'storage-error' | 'preferences-unconfirmed';
  operation: ReleasePreferenceOperation | null;
}
export interface MobileRelease {
  tagName: string;
  version: string;
  channel: MobileReleaseChannel;
  kernelChannel: 'stable' | 'beta' | 'nightly' | null;
  publishedAt: string | null;
  notes: string | null;
  notesTruncated: boolean;
  releaseUrl: string;
}
export interface MobileReleaseCatalog {
  source: MobileReleaseSource;
  channel: MobileReleaseChannel;
  platform: 'android' | 'ios' | 'desktop';
  checkedAtUnixMs: string;
  catalogLimit: number;
  releases: MobileRelease[];
  targetStatus: 'bundled-via-client' | 'installation-not-adapted';
  targetReason: string;
  requestId: string;
  responseStatus: 200 | 304;
  reusedValidatedCache: boolean;
}
export interface ReleaseScheduleSnapshot {
  hostSessionId: string;
  revision: string;
  foreground: boolean;
  checking: boolean;
  enabled: boolean;
  clientChannel: MobileReleaseChannel;
  lastAttemptedAtUnixMs: string | null;
  lastSuccessAtUnixMs: string | null;
  nextDueAtUnixMs: string | null;
  retryAtUnixMs: string | null;
  outcome: string;
  reason: string | null;
  storageError: boolean;
  cachedCatalog: MobileReleaseCatalog | null;
  cacheCurrent: boolean;
  preferenceOperation: ReleasePreferenceOperation | null;
  foregroundScheduling: 'native-lifecycle' | 'not-native-host';
  backgroundScheduling: 'not-adapted';
}
export interface MobileReleaseGateway {
  catalog(source: MobileReleaseSource, channel: MobileReleaseChannel): Promise<MobileReleaseCatalog>;
  schedule(): Promise<ReleaseScheduleSnapshot>;
  preferences(): Promise<ReleasePreferencesSnapshot>;
  savePreferences(expectedRevision: string, preferences: ReleasePreferences, operationId: string): Promise<ReleasePreferencesSnapshot>;
  preferenceOperation(operationId: string): Promise<ReleasePreferenceOperation | null>;
  confirmPreferences(operationId: string, reviewBinding: string): Promise<ReleasePreferencesSnapshot>;
}
