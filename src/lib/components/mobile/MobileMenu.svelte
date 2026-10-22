<script lang="ts">
  import { ChevronRight, Network, ChartNoAxesCombined, Cable, Route, Puzzle, ScrollText, Wrench, Cpu, Download, Settings, Info, Shield, Globe, Gauge, FileJson, Import, Database } from '@lucide/svelte';
  import { useMobileNavigation, type MobilePage } from '$lib/features/mobile/context';
  let { settings = false }: { settings?: boolean } = $props();
  const navigation = useMobileNavigation();
  const groups = [
    { label: '网络', entries: [{ page: 'endpoints', label: '端点', icon: Network }, { page: 'traffic', label: '流量统计', icon: ChartNoAxesCombined }, { page: 'connections', label: '连接', icon: Cable }, { page: 'packet', label: 'Packet 路径', icon: Route }] },
    { label: '扩展与诊断', entries: [{ page: 'plugins', label: '插件', icon: Puzzle }, { page: 'logs', label: '日志', icon: ScrollText }, { page: 'diagnostics', label: '诊断', icon: Wrench }] },
    { label: '版本与应用', entries: [{ page: 'kernel', label: '内核管理', icon: Cpu }, { page: 'updates', label: '客户端升级', icon: Download }, { page: 'settings', label: '设置', icon: Settings }, { page: 'about', label: '关于', icon: Info }] },
  ];
  const settingsGroups = [
    { label: '网络与配置', entries: [{ page: 'tun', label: 'VPN 与接管', icon: Shield }, { page: 'dns', label: 'DNS', icon: Globe }, { page: 'network', label: '运行网络', icon: Network }, { page: 'probes', label: '节点延迟测试', icon: Gauge }, { page: 'workspace', label: '配置工作区', icon: FileJson }, { page: 'migration', label: '设置迁移', icon: Import }] },
    { label: '应用', entries: [{ page: 'data', label: '本地数据', icon: Database }] },
  ];
</script>
<div class="mobile-menu">
  {#if !settings}<h1>更多</h1>{/if}
  {#each settings ? settingsGroups : groups as group}
    <section aria-label={group.label}>
      <h2>{group.label}</h2>
      <div class="menu-card">
        {#each group.entries as entry}
          <button data-slot="surface-button" onclick={() => navigation.open(entry.page as MobilePage)}>
            <span class="entry-icon"><entry.icon size={20} aria-hidden="true" /></span>
            <span>{entry.label}</span><ChevronRight class="chevron" size={18} aria-hidden="true" />
          </button>
        {/each}
      </div>
    </section>
  {/each}
</div>
<style>
  .mobile-menu { overflow-y:auto; padding:16px 14px 26px; flex:1; min-height:0; }
  h1 { font-size:1.45rem; font-weight:500; margin:0 0 16px; }
  h2 { font-size:.8rem; font-weight:500; color:var(--muted-foreground); margin:20px 3px 8px; }
  .menu-card { background:var(--card); border:1px solid var(--border); border-radius:12px; overflow:hidden; }
  button { display:flex; align-items:center; gap:12px; width:100%; min-height:64px; padding:10px 12px; text-align:left; font:inherit; color:var(--foreground); background:transparent; border:0; cursor:pointer; }
  button + button { border-top:1px solid var(--border); }
  button:focus-visible { outline:2px solid var(--ring); outline-offset:-3px; }
  .entry-icon { display:grid; place-items:center; width:34px; height:34px; border-radius:9px; background:var(--muted); flex-shrink:0; }
  :global(.chevron) { margin-left:auto; color:var(--muted-foreground); flex-shrink:0; }
</style>
