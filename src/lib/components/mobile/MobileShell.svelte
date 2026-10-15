<script lang="ts">
  import { onMount, untrack, type Snippet } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { CircleGauge, Waypoints, Layers3, Grid2X2, ChevronLeft } from '@lucide/svelte';
  import { store } from '$lib/services/store.svelte';
  import { provideMobileNavigation, type MobilePage, type MobileTab } from '$lib/features/mobile/context';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  import MobileMenu from './MobileMenu.svelte';
  import MobileSecondary from './MobileSecondary.svelte';
  let { overview, nodes, configuration }: { overview: Snippet; nodes: Snippet; configuration: Snippet } = $props();
  const navigation = provideMobileNavigation();
  const tabs = [{ id: 'overview', label: '概览', icon: CircleGauge }, { id: 'nodes', label: '节点', icon: Waypoints }, { id: 'configuration', label: '配置', icon: Layers3 }, { id: 'more', label: '更多', icon: Grid2X2 }] as const;
  const titles: Record<MobilePage, string> = { traffic: '流量统计', endpoints: '端点', connections: '连接', packet: 'Packet 路径', plugins: '插件', logs: '日志', diagnostics: '诊断', kernel: '内核管理', updates: '客户端升级', settings: '设置', about: '关于', tun: 'VPN 与接管', dns: 'DNS', network: '运行网络', probes: '节点延迟测试', workspace: '配置工作区', migration: '设置迁移', data: '本地数据', rules: '路由与规则集', profiles: '配置', subscriptions: '订阅' };
  const visited = new SvelteSet<MobileTab>(['overview']);
  let shell: HTMLElement;
  function selectTab(tab: MobileTab) {
    navigation.selectTab(tab);
    if (navigation.activeTab === tab) store.activeTab = tab === 'configuration' ? 'profiles' : tab;
  }
  function back(): boolean {
    // Dialogs own their Escape and return handling; never navigate underneath one.
    const dialog = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"], [role="alertdialog"]'))
      .filter(element => element.getClientRects().length > 0 && element.getAttribute('aria-hidden') !== 'true').at(-1);
    if (dialog) {
      dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      return true;
    }
    return navigation.back();
  }
  $effect(() => {
    const tab = store.activeTab;
    const section = store.settingsSection;
    untrack(() => {
      if (tab === 'overview' || tab === 'nodes' || tab === 'profiles' || tab === 'more') {
        const mainTab = tab === 'profiles' ? 'configuration' : tab;
        navigation.selectTab(mainTab);
        if (navigation.activeTab === mainTab) visited.add(mainTab);
      } else if (tab === 'settings') {
        const target: MobilePage = ({ core: 'kernel', general: 'settings', config: 'workspace', network: 'network', tun: 'tun', dns: 'dns', logs: 'data', about: 'about' } as const)[section];
        if (navigation.current?.page !== target) navigation.open(target);
      } else if (['endpoints', 'connections', 'logs', 'plugins', 'rules', 'subscriptions'].includes(tab)) {
        if (navigation.current?.page !== tab) navigation.open(tab as MobilePage);
      }
    });
  });
  $effect(() => {
    const tab = navigation.activeTab;
    untrack(() => {
      visited.add(tab);
      store.activeTab = tab === 'configuration' ? 'profiles' : tab;
    });
  });
  onMount(() => {
    const adjustViewport = () => {
      const viewport = window.visualViewport;
      if (viewport && shell) shell.style.height = `${viewport.height}px`;
    };
    const onBack = (event: Event) => { if (back()) event.preventDefault(); };
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || !navigation.current) return;
      if (document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]')) return;
      event.preventDefault(); back();
    };
    const onPopState = () => { if (navigation.current) back(); };
    navigation.onOpen = () => window.history.pushState({ ...window.history.state, mobileSecondary: true }, '');
    adjustViewport();
    window.visualViewport?.addEventListener('resize', adjustViewport);
    window.addEventListener('resize', adjustViewport);
    window.addEventListener('mobile:back', onBack);
    window.addEventListener('popstate', onPopState);
    document.addEventListener('keydown', onEscape);
    return () => {
      navigation.onOpen = null;
      window.visualViewport?.removeEventListener('resize', adjustViewport);
      window.removeEventListener('resize', adjustViewport);
      window.removeEventListener('mobile:back', onBack);
      window.removeEventListener('popstate', onPopState);
      document.removeEventListener('keydown', onEscape);
    };
  });
</script>
<div class="mobile-shell" bind:this={shell} data-platform={mobilePlatform.runtimePlatform} data-responsive-preview={mobilePlatform.responsivePreview}>
  <div class="mobile-content">
    {#each tabs as tab}
      {#if visited.has(tab.id)}
        <section class="tab-pane" class:inactive={navigation.activeTab !== tab.id} aria-label={tab.label} aria-hidden={navigation.activeTab !== tab.id} inert={navigation.activeTab !== tab.id}>
          <div class="root-pane" class:inactive={navigation.stacks[tab.id].length > 0} inert={navigation.stacks[tab.id].length > 0}>
            {#if tab.id === 'overview'}<div class="overview-root">{@render overview()}</div>{:else if tab.id === 'nodes'}{@render nodes()}{:else if tab.id === 'configuration'}{@render configuration()}{:else}<MobileMenu />{/if}
          </div>
          {#each navigation.stacks[tab.id] as route (route.key)}
            <div class="secondary-pane" class:inactive={route.key !== navigation.stacks[tab.id].at(-1)?.key} inert={route.key !== navigation.stacks[tab.id].at(-1)?.key}>
              <header class="secondary-header"><button data-slot="surface-button" aria-label="返回" onclick={back}><ChevronLeft size={22} aria-hidden="true" /></button><h1>{titles[route.page]}</h1></header>
              <div class="secondary-content"><MobileSecondary page={route.page} routeKey={route.key} tab={tab.id} /></div>
            </div>
          {/each}
        </section>
      {/if}
    {/each}
  </div>
  <nav class="bottom-nav" aria-label="主导航">
    {#each tabs as tab}<button data-slot="surface-button" aria-current={navigation.activeTab === tab.id ? 'page' : undefined} onclick={() => selectTab(tab.id)}><tab.icon size={20} aria-hidden="true" /><span>{tab.label}</span></button>{/each}
  </nav>
</div>
<style>
  .mobile-shell { display:flex; flex-direction:column; height:100vh; height:100dvh; width:100%; min-width:0; overflow:hidden; padding-top:env(safe-area-inset-top,0px); background:var(--background); color:var(--foreground); }
  .mobile-content { margin-left:env(safe-area-inset-left,0px); margin-right:env(safe-area-inset-right,0px); flex:1; min-height:0; position:relative; overflow:hidden; }
  .tab-pane,.root-pane,.secondary-pane { position:absolute; inset:0; display:flex; flex-direction:column; min-width:0; min-height:0; overflow:hidden; }
  .inactive { display:none; }
  .overview-root { overflow:auto; flex:1; min-height:0; padding:12px 14px 24px; }
  .secondary-header { display:flex; align-items:center; min-height:60px; gap:8px; padding:8px 12px; flex-shrink:0; border-bottom:1px solid var(--border); }
  .secondary-header button { width:44px; height:44px; flex-shrink:0; border:0; color:var(--foreground); background:transparent; border-radius:9px; display:grid; place-items:center; }
  .secondary-header h1 { font-size:1.125rem; font-weight:500; line-height:1.4; overflow-wrap:anywhere; margin:0; }
  .secondary-content { flex:1; display:flex; flex-direction:column; min-height:0; min-width:0; overflow:auto; }
  .bottom-nav { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); border-top:1px solid var(--border); background:var(--card); flex-shrink:0; padding:8px max(7px,env(safe-area-inset-right,0px)) max(8px,env(safe-area-inset-bottom,0px)) max(7px,env(safe-area-inset-left,0px)); }
  .bottom-nav button { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:3px; min-height:48px; min-width:44px; border:0; padding:4px; color:var(--muted-foreground); background:transparent; font:inherit; font-size:.75rem; }
  .bottom-nav button[aria-current] { color:var(--foreground); font-weight:500; }
  button:focus-visible { outline:2px solid var(--ring); outline-offset:-2px; }
  /* Scope touch and text rules to the mobile host; desktop density remains intact. */
  .mobile-shell :global(button),.mobile-shell :global([role="button"]),.mobile-shell :global(input:not([type="checkbox"]):not([type="radio"])),.mobile-shell :global(select) { min-height:44px; }
  .mobile-shell :global(button) { min-width:44px; }
  .mobile-shell :global(input:not([type="checkbox"]):not([type="radio"])),.mobile-shell :global(select),.mobile-shell :global(textarea) { font-size:1rem; }
  .mobile-shell :global(.config-section),.mobile-shell :global(.settings-panel),.mobile-shell :global(.config-editor),.mobile-shell :global(.diag-panel) { min-width:0; }
  .mobile-shell :global(.config-row) { flex-wrap:wrap; gap:12px; }
  .mobile-shell :global(.config-row-label) { min-width:0; }
  .mobile-shell :global(.statistics) { padding:14px; }
  .mobile-shell :global(.statistics .tools) { flex-wrap:wrap; }
  .mobile-shell :global(.statistics .scope-grid) { grid-template-columns:minmax(0,1fr); }
  @media (prefers-reduced-motion:reduce) { .mobile-shell :global(*) { scroll-behavior:auto; } }
</style>
