import type { EndpointTrafficVisit } from '$lib/features/endpoints/traffic-navigation';
import type { MobileLocationTarget } from './location';
export type MobileTab = 'overview' | 'nodes' | 'configuration' | 'more';
export type MobilePage = 'traffic' | 'endpoints' | 'connections' | 'packet' | 'plugins' | 'logs' |
  'diagnostics' | 'kernel' | 'updates' | 'settings' | 'about' | 'tun' | 'dns' | 'network' |
  'probes' | 'workspace' | 'migration' | 'data' | 'rules' | 'profiles' | 'subscriptions' | 'nodes';
export type MobileRouteOrigin = Readonly<{ tab: MobileTab; key: number | null }>;
export type MobileRoute = { key: number; page: MobilePage; trafficVisit?: EndpointTrafficVisit;
  locationTarget?: MobileLocationTarget; origin?: MobileRouteOrigin };

/** Each bottom tab owns its return stack. Mounted panes retain drafts and scroll. */
export class MobileNavigation {
  activeTab = $state<MobileTab>('overview');
  stacks = $state<Record<MobileTab, MobileRoute[]>>({ overview: [], nodes: [], configuration: [], more: [] });
  /** Host hook for browser/system return integration; contains no business data. */
  onOpen: (() => void) | null = null;
  returnNotice = $state<string | null>(null);
  private sequence = 0;
  private navigationEpoch = 0;
  private beforeLeave = new Set<(continueNavigation: () => void) => boolean>();
  selectTab(tab: MobileTab) {
    if (tab !== this.activeTab && !this.canLeave(() => this.selectTab(tab))) return;
    if (tab !== this.activeTab) { this.navigationEpoch++; this.returnNotice = null; }
    this.activeTab = tab;
  }
  open(page: MobilePage, trafficVisit?: EndpointTrafficVisit) {
    if (!this.canLeave(() => this.open(page, trafficVisit))) return;
    this.navigationEpoch++; this.returnNotice = null;
    this.stacks[this.activeTab] = [...this.stacks[this.activeTab], { key: ++this.sequence, page, trafficVisit }];
    this.onOpen?.();
  }
  visit(tab: 'configuration', page: 'profiles', target: Extract<MobileLocationTarget, { kind: 'profile' }>): boolean;
  visit(tab: 'nodes', page: 'nodes', target: Extract<MobileLocationTarget, { kind: 'nodes' }>): boolean;
  /** One guarded transition preserves both tabs' existing mounted stacks. */
  visit(tab: 'configuration' | 'nodes', page: 'profiles' | 'nodes', target: MobileLocationTarget): boolean {
    if ((tab === 'configuration' && (page !== 'profiles' || target.kind !== 'profile'))
      || (tab === 'nodes' && (page !== 'nodes' || target.kind !== 'nodes'))) return false;
    if (target.kind === 'nodes' && (!target.selection || !['global', 'rule', 'direct', 'unconfirmed'].includes(target.selection.kind))) return false;
    const binding = target.binding ? { ...target.binding } : null;
    const u64 = (value: string) => typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value)
      && BigInt(value) <= 18446744073709551615n;
    if (binding && (!binding.binding || !binding.profileId || !u64(binding.configRevision)
      || !u64(binding.coreInstanceId) || !u64(binding.scopeGeneration) || binding.scopeGeneration === '0')) return false;
    if ((target.kind === 'profile' && (target.profileId === null ? binding !== null : target.profileId !== binding?.profileId))
      || (target.kind === 'nodes' && (!binding || (target.selection.kind === 'global' && (typeof target.selection.rootTag !== 'string' || !target.selection.rootTag))))) return false;
    const fixed: MobileLocationTarget = target.kind === 'profile'
      ? { kind: 'profile', profileId: target.profileId, binding }
      : { kind: 'nodes', binding: binding!, selection: { ...target.selection } };
    const origin = { tab: this.activeTab, key: this.current?.key ?? null };
    const sourceEpoch = this.navigationEpoch;
    const sourceCurrent = () => this.navigationEpoch === sourceEpoch && this.activeTab === origin.tab && (this.current?.key ?? null) === origin.key;
    const perform = (): boolean => {
      // A delayed discard callback belongs to its original page, not a later visit.
      if (!sourceCurrent()) return false;
      if (!this.canLeave(() => { perform(); })) return false;
      if (!sourceCurrent()) return false;
      this.navigationEpoch++;
      this.returnNotice = null;
      this.stacks[tab] = [...this.stacks[tab], { key: ++this.sequence, page, locationTarget: fixed, origin }];
      this.activeTab = tab;
      this.onOpen?.();
      return true;
    };
    return perform();
  }
  back(): boolean {
    if (!this.canLeave(() => this.back())) return true;
    if (!this.current) return false;
    const returning = this.current;
    this.navigationEpoch++;
    this.stacks[this.activeTab] = this.stacks[this.activeTab].slice(0, -1);
    if (returning.origin) {
      const origin = returning.origin;
      this.returnNotice = (this.stacks[origin.tab].at(-1)?.key ?? null) !== origin.key
        ? '返回来源已变化，已返回该底栏的最新页面；原草稿和页面仍保留。' : null;
      this.activeTab = origin.tab;
    }
    returning?.trafficVisit?.restore();
    return true;
  }
  get current() { return this.stacks[this.activeTab].at(-1); }
  /** Editors can retain their draft and require an explicit discard decision. */
  guardBeforeLeave(guard: (continueNavigation: () => void) => boolean): () => void {
    this.beforeLeave.add(guard);
    return () => { this.beforeLeave.delete(guard); };
  }
  private canLeave(continueNavigation: () => void) {
    for (const guard of [...this.beforeLeave].reverse()) if (!guard(continueNavigation)) return false;
    return true;
  }
}
