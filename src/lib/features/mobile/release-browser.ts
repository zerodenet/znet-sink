import { compareAppVersions } from '$lib/services/app-update-policy';
import type { MobileReleaseCatalog, MobileReleaseChannel, MobileReleaseGateway, MobileReleaseSource } from '$lib/types/mobile-release';

export interface ReleaseBrowserState {
  source: MobileReleaseSource;
  channel: MobileReleaseChannel;
  phase: 'idle' | 'checking' | 'ready' | 'error';
  catalog: MobileReleaseCatalog | null;
  error: string | null;
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

export class MobileReleaseBrowser {
  state: ReleaseBrowserState;
  private generation = 0;
  private disposed = false;
  constructor(private gateway: Pick<MobileReleaseGateway, 'catalog'>, private notify: (state: ReleaseBrowserState) => void,
    source: MobileReleaseSource, channel: MobileReleaseChannel = 'stable') {
    this.state = { source, channel, phase: 'idle', catalog: null, error: null };
  }
  private publish(next: ReleaseBrowserState) { if (this.disposed) return; this.state = next; this.notify(next); }
  select(source: MobileReleaseSource, channel: MobileReleaseChannel) {
    if (source === this.state.source && channel === this.state.channel) return;
    this.generation++;
    this.publish({ source, channel, phase: 'idle', catalog: null, error: null });
  }
  async check() {
    if (this.disposed || this.state.phase === 'checking') return;
    const { source, channel } = this.state;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'checking', error: null });
    try {
      const catalog = await this.gateway.catalog(source, channel);
      if (this.disposed || generation !== this.generation) return;
      if (catalog.source !== source || catalog.channel !== channel) throw new Error('invalid scope');
      this.publish({ ...this.state, phase: 'ready', catalog, error: null });
    } catch {
      if (this.disposed || generation !== this.generation) return;
      this.publish({ ...this.state, phase: 'error', error: '发行目录检查失败，无法确认是否有更新。请检查网络后重试。' });
    }
  }
  destroy() { this.disposed = true; this.generation++; }
}
