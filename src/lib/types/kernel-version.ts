export type ReleaseChannel = 'stable' | 'beta' | 'nightly';

export interface KernelRelease {
  version: string;
  channel: ReleaseChannel;
  prerelease: boolean;
  publishedAtUnixMs?: number;
  assetSizeBytes?: number;
  assetDownloadUrl?: string;
  releaseNotesUrl?: string;
  checksumSha256?: string;
}

export interface KernelVersionList {
  versions: KernelRelease[];
}

export interface KernelDownloadProgress {
  version: string;
  bytesDownloaded: number;
  bytesTotal?: number;
  percent?: number;
}

export interface KernelInstallResult {
  success: boolean;
  executablePath: string;
  version: string;
  channel: ReleaseChannel;
  checksumVerified: boolean;
  message: string;
}

export interface KernelVersionDetect {
  version?: string;
  source: string;
}
