import { compareAppVersions } from '$lib/services/app-update-policy';
import type { MobileReleaseCatalog, MobileReleaseChannel, MobileReleaseGateway, MobileReleaseSource } from '$lib/types/mobile-release';

export interface ReleaseBrowserState {
  source: MobileReleaseSource;
  channel: MobileReleaseChannel;
  phase: 'idle' | 'checking' | 'ready' | 'error';
  catalog: MobileReleaseCatalog | null;
  error: string | null;
  catalogCurrent: boolean;
}

export function releaseComparison(catalog: MobileReleaseCatalog | null, current: string | null): 'unchecked' | 'no-release' | 'current-unknown' | 'available' | 'no-update' {
  if (!catalog) return 'unchecked';
  if (!catalog.releases.length) return 'no-release';
  const target = catalog.releases[0].version;
  // The existing desktop comparator uses Number; avoid unsafe numeric versions
  // or its lexical fallback when a current mobile build is not a semantic version.
  const comparable = (version: string | null): version is string => Boolean(version
    && /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(version)
    && version.replace(/^v/, '').split('+', 1)[0].split(/[.-]/).every(part => !/^\d+$/.test(part) || Number.isSafeInteger(Number(part))));
  if (!comparable(current) || !comparable(target)) return 'current-unknown';
  return compareAppVersions(target, current) > 0 ? 'available' : 'no-update';
}

export function validReleaseCatalog(c: MobileReleaseCatalog): boolean {
  const u64 = (v: unknown): v is string => typeof v === 'string' && v.length <= 20 && /^(0|[1-9]\d*)$/.test(v) && BigInt(v) <= 18446744073709551615n;
  if (!c || !['client', 'kernel'].includes(c.source) || !['stable', 'preview', 'test'].includes(c.channel)
    || !['android', 'ios', 'desktop'].includes(c.platform) || typeof c.requestId !== 'string' || !c.requestId || c.requestId.length > 128
    || (c.responseStatus !== 200 && c.responseStatus !== 304) || c.reusedValidatedCache !== (c.responseStatus === 304)
    || !u64(c.checkedAtUnixMs) || !Number.isSafeInteger(c.catalogLimit) || c.catalogLimit <= 0 || c.catalogLimit > 30
    || !Array.isArray(c.releases) || c.releases.length > c.catalogLimit || typeof c.targetReason !== 'string' || c.targetReason.length > 4096
    || (c.source === 'kernel' ? c.targetStatus !== 'bundled-via-client' : c.targetStatus !== 'installation-not-adapted' && !(c.platform === 'android' && c.targetStatus === 'android-system-installer'))) return false;
  const project = c.source === 'client' ? 'znet-sink' : 'core';
  const releaseUrl = new RegExp(`^https://github\\.com/zerodenet/${project}/releases/tag/v?[0-9A-Za-z.+-]+$`);
  return c.releases.every(r => !!r && typeof r.tagName === 'string' && r.tagName.length > 0 && r.tagName.length <= 256
    && typeof r.version === 'string' && r.version.length > 0 && r.version.length <= 256 && r.channel === c.channel
    && (c.source === 'client' ? r.kernelChannel === null : r.kernelChannel === ({stable: 'stable', preview: 'beta', test: 'nightly'} as const)[c.channel])
    && (r.notes === null || (typeof r.notes === 'string' && new TextEncoder().encode(r.notes).byteLength <= 65536))
    && typeof r.notesTruncated === 'boolean' && (r.publishedAt === null || (typeof r.publishedAt === 'string' && r.publishedAt.length <= 128))
    && typeof r.releaseUrl === 'string' && r.releaseUrl.length <= 1024 && releaseUrl.test(r.releaseUrl)
    && (r.androidAssets === undefined || Array.isArray(r.androidAssets) && r.androidAssets.length <= 16 && r.androidAssets.every(a =>
      !!a && c.source === 'client' && u64(a.assetId) && a.assetId !== '0'
      && /^[0-9a-f]{64}$/.test(a.sha256) && /^[A-Za-z0-9._-]{1,180}\.apk$/.test(a.name)
      && u64(a.sizeBytes) && BigInt(a.sizeBytes) > 0n && BigInt(a.sizeBytes) <= 536870912n
      && a.url === `https://github.com/zerodenet/znet-sink/releases/download/${r.tagName}/${a.name}`)));
}

export class MobileReleaseBrowser {
  state: ReleaseBrowserState;
  private generation = 0;
  private disposed = false;
  constructor(private gateway: Pick<MobileReleaseGateway, 'catalog'>, private notify: (state: ReleaseBrowserState) => void,
    source: MobileReleaseSource, channel: MobileReleaseChannel = 'stable') {
    this.state = { source, channel, phase: 'idle', catalog: null, error: null, catalogCurrent: false };
  }
  private publish(next: ReleaseBrowserState) { if (this.disposed) return; this.state = next; this.notify(next); }
  select(source: MobileReleaseSource, channel: MobileReleaseChannel) {
    if (source === this.state.source && channel === this.state.channel) return;
    this.generation++;
    this.publish({ source, channel, phase: 'idle', catalog: null, error: null, catalogCurrent: false });
  }
  async check() {
    if (this.disposed || this.state.phase === 'checking') return;
    const { source, channel } = this.state;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'checking', error: null });
    try {
      const catalog = await this.gateway.catalog(source, channel);
      if (this.disposed || generation !== this.generation) return;
      if (catalog.source !== source || catalog.channel !== channel || !validReleaseCatalog(catalog)) throw new Error('invalid scope');
      this.publish({ ...this.state, phase: 'ready', catalog, catalogCurrent: true, error: null });
    } catch {
      if (this.disposed || generation !== this.generation) return;
      this.publish({ ...this.state, phase: 'error', catalogCurrent: false, error: '发行目录检查失败，无法确认是否有更新。请检查网络后重试。' });
    }
  }
  acceptCached(catalog: MobileReleaseCatalog | null, current: boolean) {
    if (this.disposed || this.state.phase === 'checking') return;
    if (catalog === null) {
      if (this.state.catalog) this.publish({ ...this.state, catalogCurrent: false });
      return;
    }
    if (!validReleaseCatalog(catalog) || catalog.source !== this.state.source || catalog.channel !== this.state.channel) return;
    const before = this.state.catalog;
    if (before && BigInt(before.checkedAtUnixMs) > BigInt(catalog.checkedAtUnixMs)) return;
    const fresh = current && (!before || BigInt(catalog.checkedAtUnixMs) > BigInt(before.checkedAtUnixMs) || catalog.requestId !== before.requestId);
    const preserveError = this.state.phase === 'error' && !fresh;
    this.publish({ ...this.state, phase: preserveError ? 'error' : 'ready', catalog,
      catalogCurrent: current && !preserveError, error: preserveError ? this.state.error : null });
  }
  destroy() { this.disposed = true; this.generation++; }
}
