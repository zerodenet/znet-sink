export type AppReleaseChannel = 'stable' | 'preview' | 'test';

export interface AppRelease {
  tagName: string;
  version: string;
  channel: AppReleaseChannel;
  publishedAt: string | null;
  notes: string | null;
  releaseUrl: string;
}

interface GitHubReleaseAsset {
  name?: unknown;
}

interface GitHubRelease {
  tag_name?: unknown;
  draft?: unknown;
  published_at?: unknown;
  body?: unknown;
  html_url?: unknown;
  assets?: unknown;
}

interface ParsedVersion {
  major: number;
  minor: number;
  patch: number;
  prerelease: string[];
}

const RELEASES_API = 'https://api.github.com/repos/zerodenet/znet-sink/releases?per_page=30';

export function classifyAppVersion(version: string): AppReleaseChannel {
  const parsed = parseVersion(version);
  if (!parsed) return 'test';
  if (parsed.prerelease.length === 0) return 'stable';
  const label = parsed.prerelease[0]?.toLowerCase() ?? '';
  return label === 'rc' || label === 'preview' ? 'preview' : 'test';
}

export function compareAppVersions(left: string, right: string): number {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (!a || !b) return left.localeCompare(right);

  for (const key of ['major', 'minor', 'patch'] as const) {
    if (a[key] !== b[key]) return a[key] > b[key] ? 1 : -1;
  }

  if (a.prerelease.length === 0 && b.prerelease.length === 0) return 0;
  if (a.prerelease.length === 0) return 1;
  if (b.prerelease.length === 0) return -1;

  const length = Math.max(a.prerelease.length, b.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const aPart = a.prerelease[index];
    const bPart = b.prerelease[index];
    if (aPart == null) return -1;
    if (bPart == null) return 1;
    if (aPart === bPart) continue;

    const aNumeric = /^\d+$/.test(aPart);
    const bNumeric = /^\d+$/.test(bPart);
    if (aNumeric && bNumeric) return Number(aPart) > Number(bPart) ? 1 : -1;
    if (aNumeric !== bNumeric) return aNumeric ? -1 : 1;
    return aPart > bPart ? 1 : -1;
  }
  return 0;
}

export function shouldShowProminentUpdate(currentVersion: string, targetVersion: string | null): boolean {
  return Boolean(
    targetVersion
      && classifyAppVersion(currentVersion) === 'stable'
      && classifyAppVersion(targetVersion) === 'stable',
  );
}

export async function fetchAppReleases(
  fetchImpl: typeof fetch = fetch,
): Promise<AppRelease[]> {
  const response = await fetchImpl(RELEASES_API, {
    headers: { Accept: 'application/vnd.github+json' },
  });
  if (!response.ok) {
    throw new Error(`GitHub 发布列表请求失败 (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!Array.isArray(payload)) throw new Error('GitHub 发布列表格式无效');

  return payload
    .map(normalizeRelease)
    .filter((release): release is AppRelease => release != null)
    .sort((left, right) => compareAppVersions(right.version, left.version));
}

function normalizeRelease(value: unknown): AppRelease | null {
  if (!value || typeof value !== 'object') return null;
  const release = value as GitHubRelease;
  if (release.draft === true || typeof release.tag_name !== 'string') return null;

  const version = release.tag_name.replace(/^v/, '');
  if (!parseVersion(version)) return null;

  const assets = Array.isArray(release.assets) ? release.assets as GitHubReleaseAsset[] : [];
  if (!assets.some((asset) => asset?.name === 'latest.json')) return null;

  return {
    tagName: release.tag_name,
    version,
    channel: classifyAppVersion(version),
    publishedAt: typeof release.published_at === 'string' ? release.published_at : null,
    notes: typeof release.body === 'string' && release.body.trim() ? release.body : null,
    releaseUrl: typeof release.html_url === 'string' ? release.html_url : '',
  };
}

function parseVersion(input: string): ParsedVersion | null {
  const normalized = input.trim().replace(/^v/, '').split('+', 1)[0];
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/.exec(normalized);
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4]?.split('.') ?? [],
  };
}
