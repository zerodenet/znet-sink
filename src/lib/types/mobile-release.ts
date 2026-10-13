export type MobileReleaseSource = 'client' | 'kernel';
export type MobileReleaseChannel = 'stable' | 'preview' | 'test';
export interface ReleasePreferences {
  clientAutoCheck: boolean;
  clientChannel: MobileReleaseChannel;
  kernelChannel: MobileReleaseChannel;
}
export interface ReleasePreferencesSnapshot {
  preferences: ReleasePreferences;
  expectedRevision: string;
  schedulerState: 'not-adapted';
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
}
export interface MobileReleaseGateway {
  catalog(source: MobileReleaseSource, channel: MobileReleaseChannel): Promise<MobileReleaseCatalog>;
  preferences(): Promise<ReleasePreferencesSnapshot>;
  savePreferences(expectedRevision: string, preferences: ReleasePreferences): Promise<ReleasePreferencesSnapshot>;
}
