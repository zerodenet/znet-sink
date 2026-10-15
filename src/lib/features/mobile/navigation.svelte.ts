export type MobileTab = 'overview' | 'nodes' | 'configuration' | 'more';
export type MobilePage = 'traffic' | 'endpoints' | 'connections' | 'packet' | 'plugins' | 'logs' |
  'diagnostics' | 'kernel' | 'updates' | 'settings' | 'about' | 'tun' | 'dns' | 'network' |
  'probes' | 'workspace' | 'migration' | 'data' | 'rules' | 'profiles' | 'subscriptions';
export type MobileRoute = { key: number; page: MobilePage };

/** Each bottom tab owns its return stack. Mounted panes retain drafts and scroll. */
export class MobileNavigation {
  activeTab = $state<MobileTab>('overview');
  stacks = $state<Record<MobileTab, MobileRoute[]>>({ overview: [], nodes: [], configuration: [], more: [] });
  /** Host hook for browser/system return integration; contains no business data. */
  onOpen: (() => void) | null = null;
  private sequence = 0;
  private beforeLeave = new Set<(continueNavigation: () => void) => boolean>();
  selectTab(tab: MobileTab) {
    if (tab !== this.activeTab && !this.canLeave(() => this.selectTab(tab))) return;
    this.activeTab = tab;
  }
  open(page: MobilePage) {
    if (!this.canLeave(() => this.open(page))) return;
    this.stacks[this.activeTab] = [...this.stacks[this.activeTab], { key: ++this.sequence, page }];
    this.onOpen?.();
  }
  back(): boolean {
    if (!this.canLeave(() => this.back())) return true;
    if (!this.current) return false;
    this.stacks[this.activeTab] = this.stacks[this.activeTab].slice(0, -1);
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
