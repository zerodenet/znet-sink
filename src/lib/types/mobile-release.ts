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
  androidAssets?: AndroidReleaseAsset[];
}
export interface AndroidReleaseAsset { assetId: string; name: string; url: string; sha256: string; sizeBytes: string }
export interface MobileReleaseCatalog {
  source: MobileReleaseSource;
  channel: MobileReleaseChannel;
  platform: 'android' | 'ios' | 'desktop';
  checkedAtUnixMs: string;
  catalogLimit: number;
  releases: MobileRelease[];
  targetStatus: 'bundled-via-client' | 'installation-not-adapted' | 'android-system-installer';
  targetReason: string;
  requestId: string;
  responseStatus: 200 | 304;
  reusedValidatedCache: boolean;
}

export type AndroidUpdateState = 'idle' | 'queued' | 'downloading' | 'paused' | 'verifying' | 'ready' | 'preparing-install' | 'awaiting-user' | 'installing' | 'installed-confirmed' | 'cancelled' | 'failed' | 'unknown' | 'store-managed';
export interface AndroidUpdateSnapshot {
  state: AndroidUpdateState;
  operationId: string | null;
  reason: string | null;
  packageName: string;
  installedVersion: string;
  installedVersionCode: string;
  targetVersion: string | null;
  targetVersionCode: string | null;
  downloadedBytes: string;
  sizeBytes: string | null;
  assetName: string | null;
  sha256: string | null;
  installPermission: 'allowed' | 'blocked' | 'system-confirmation';
  confirmationAvailable: boolean;
  installerStatus: number | null;
}
export interface AndroidUpdateGateway {
  status(): Promise<AndroidUpdateSnapshot>;
  download(channel: MobileReleaseChannel, tag: string, asset: AndroidReleaseAsset, operationId: string): Promise<AndroidUpdateSnapshot>;
  action(action: 'install' | 'confirm' | 'cancel' | 'settings' | 'store', operationId: string | null): Promise<AndroidUpdateSnapshot>;
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
