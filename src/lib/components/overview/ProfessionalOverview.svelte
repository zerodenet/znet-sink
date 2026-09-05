<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Destination, OverviewModel } from './model';
  import { Button } from '$lib/components/ui/button';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  let { model, busy = false, refreshing = false, traffic, navigate, refresh, start, restart, setMode }: {
    model: OverviewModel; busy?: boolean; refreshing?: boolean; traffic: Snippet;
    navigate: (destination: Destination) => void; refresh: () => void;
    start: () => void; restart: () => void; setMode: (mode: 'global' | 'rule' | 'direct') => void;
  } = $props();
  const modes = [{ value: 'rule', label: '规则' }, { value: 'global', label: '全局' }, { value: 'direct', label: '直连' }] as const;
</script>

<div class="professional-overview" aria-label="专业运行概览">
  <header class="overview-heading">
    <div><div class="eyebrow">运行诊断</div><h2 class={model.tone}>{model.title}</h2><p>状态响应 · {model.freshness}。控制面就绪不代表每个网站均可访问。</p></div>
    <div class="actions">
      <Button variant="outline" size="sm" disabled={refreshing} onclick={refresh}>{refreshing ? '检查中…' : '重新检查'}</Button>
      <Button variant="outline" size="sm" onclick={() => navigate('logs')}>查看日志</Button>
    </div>
  </header>

  {#if model.findings.length}
    <section class="findings" aria-label="需要关注">
      {#each model.findings as finding}
        <div class="finding"><span class="signal {finding.severity}" aria-hidden="true"></span><div><strong>{finding.title}</strong><p>{finding.detail}</p></div><Button variant="ghost" size="sm" onclick={() => navigate(finding.target)}>去处理</Button></div>
      {/each}
    </section>
  {/if}

  <section class="runtime-card" aria-label="当前运行上下文">
    <div class="runtime-main"><div><span class="label">活动配置</span><strong class="config-name">{model.source}</strong></div><Button variant="ghost" size="sm" onclick={() => navigate('profiles')}>管理配置</Button></div>
    <div class="runtime-controls">
      <dl class="runtime-meta"><div><dt>内核版本</dt><dd>{model.version}</dd></div><div><dt>PID</dt><dd>{model.pid}</dd></div><div><dt>运行时长</dt><dd>{model.uptime}</dd></div></dl>
      <SegmentedControl.Root value={model.mode} disabled={busy || !model.ready} aria-label="路由模式" onValueChange={(value) => { if (value === 'global' || value === 'rule' || value === 'direct') setMode(value); }}>
        {#each modes as mode}<SegmentedControl.Item value={mode.value}>{mode.label}</SegmentedControl.Item>{/each}
      </SegmentedControl.Root>
      <Button size="sm" variant="outline" disabled={busy || model.stale} onclick={() => model.running ? restart() : start()}>{model.running ? '重启内核' : '启动内核'}</Button>
    </div>
  </section>

  <div class="detail-grid">
    <section class="panel" aria-label="流量接管与解析">
      <header><h3>流量接管与解析</h3><Button variant="ghost" size="sm" onclick={() => navigate('network')}>代理设置</Button></header>
      <dl class="facts">
        <div><dt>系统代理</dt><dd>{model.proxy}<small>{model.endpoint}</small></dd></div>
        <div><dt><button data-slot="surface-button" type="button" onclick={() => navigate('tun')}>TUN ↗</button></dt><dd>{model.tunLabel}<small>{model.tunDetails}</small></dd></div>
        <div><dt><button data-slot="surface-button" type="button" onclick={() => navigate('dns')}>DNS 路径 ↗</button></dt><dd>{model.dns}</dd></div>
        <div><dt>IPv4 / IPv6 出口</dt><dd>{model.ipv4} / {model.ipv6}<small>网络代次 {model.networkGeneration}</small></dd></div>
      </dl>
    </section>
    <section class="panel" aria-label="策略实际选择">
      <header><h3>策略实际选择 <span class="count">{model.groups.length}</span></h3><Button variant="ghost" size="sm" onclick={() => navigate('nodes')}>节点与测速</Button></header>
      <p class="hint">按策略组显示内核选择；规则模式可能同时使用多个出口。</p>
      {#if model.groups.length}
        <div class="policy-list">
          {#each model.groups.slice(0, 4) as group}
            <div class="policy-row"><div><strong>{group.name}</strong><small>{group.kind} → {group.selected}</small></div><div class:error={group.failed}><strong>{group.delay}</strong><small>{group.health}</small></div></div>
          {/each}
        </div>
        {#if model.groups.length > 4}<button data-slot="surface-button" type="button" class="more" onclick={() => navigate('nodes')}>查看全部 {model.groups.length} 个策略组 →</button>{/if}
      {:else}<p class="empty">尚无运行策略组；静态直连出站不会生成策略组。</p>{/if}
    </section>
  </div>

  <section class="traffic-section" aria-label="流量观测">
    <div class="section-heading"><h3>流量观测</h3><Button variant="ghost" size="sm" onclick={() => navigate('connections')}>查看连接 →</Button></div>
    <div class="traffic-chart">{@render traffic()}</div>
  </section>

  <details class="panel checks">
    <summary>就绪检查明细 <span>{model.selfTest ? `${model.selfTest.checks.length} 项 · ${model.selfTestAge}${model.selfTestStale ? ' · 结果待刷新' : ''}` : '尚未取得结果'}</span></summary>
    {#each model.selfTest?.checks ?? [] as check}
      <div class="check-row"><span class="check-status">{check.status === 'pass' ? '通过' : check.status === 'warn' ? '警告' : '失败'}</span><div><strong>{check.key}</strong><p>{check.message}</p></div></div>
    {/each}
  </details>
</div>

<style>
  .professional-overview { width: 100%; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 2px 3px 16px 0; }
  .overview-heading, .actions, .runtime-main, .runtime-controls, .section-heading, header, .finding { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .overview-heading { align-items: flex-start; flex-wrap: wrap; }
  .eyebrow, .label, dt, small, .hint, .empty, summary span { font-size: 11px; color: var(--muted-foreground); }
  .eyebrow { letter-spacing: .08em; margin-bottom: 4px; }
  h2 { font-size: 20px; line-height: 1.3; font-weight: 650; margin: 0; }
  h3 { margin: 0; font-size: 12px; font-weight: 650; }
  p { margin: 5px 0 0; font-size: 11px; line-height: 1.6; color: var(--muted-foreground); overflow-wrap: anywhere; }
  .error { color: var(--destructive); } .warning { color: var(--warning); } .good { color: var(--success, #16a34a); }
  .panel, .runtime-card, .findings { border: 1px solid var(--border); border-radius: 10px; background: var(--card); padding: 12px 14px; min-width: 0; }
  .findings { border-color: color-mix(in srgb, var(--warning) 45%, var(--border)); background: color-mix(in srgb, var(--warning) 5%, var(--card)); }
  .finding + .finding { border-top: 1px solid var(--border); margin-top: 9px; padding-top: 9px; }
  .finding > div { flex: 1; min-width: 0; } .finding strong { font-size: 12px; } .signal { width: 6px; height: 6px; border-radius: 50%; background: currentColor; flex-shrink: 0; }
  .config-name { display: block; margin-top: 3px; font-size: 14px; overflow-wrap: anywhere; }
  .runtime-main { padding-bottom: 10px; border-bottom: 1px solid var(--border); } .runtime-main > div { min-width: 0; }
  .runtime-controls { flex-wrap: wrap; padding-top: 10px; }
  .runtime-meta { display: flex; flex-wrap: wrap; gap: 8px 20px; flex: 1; min-width: 180px; margin: 0; }
  dd { margin: 2px 0 0; font-size: 12px; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  .detail-grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 12px; }
  .facts { margin: 3px 0 0; } .facts > div { display: grid; grid-template-columns: 108px minmax(0, 1fr); gap: 8px; padding: 9px 0; border-bottom: 1px solid var(--border); }
  .facts > div:last-child { border-bottom: 0; padding-bottom: 0; } small { display: block; margin-top: 3px; overflow-wrap: anywhere; }
  dt button, .more { color: var(--primary); background: none; border: 0; padding: 0; cursor: pointer; font: inherit; }
  button:focus-visible, summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; }
  .count { color: var(--muted-foreground); font-weight: 400; margin-left: 5px; }
  .policy-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; border-bottom: 1px solid var(--border); padding: 9px 0; }
  .policy-row > div { min-width: 0; } .policy-row > div:last-child { text-align: right; flex-shrink: 0; } .policy-row strong { font-size: 11px; overflow-wrap: anywhere; }
  .more { font-size: 11px; margin-top: 9px; } .empty { padding: 24px 0; }
  .traffic-section { min-width: 0; } .section-heading { margin-bottom: 6px; } .traffic-chart { height: 230px; }
  summary { cursor: pointer; font-size: 12px; font-weight: 600; } summary span { margin-left: 10px; font-weight: 400; }
  .check-row { display: flex; gap: 12px; padding-top: 12px; font-size: 11px; } .check-status { flex-shrink: 0; color: var(--muted-foreground); } .check-row p { margin: 2px 0 0; }
  @media (max-width: 700px) { .detail-grid { grid-template-columns: minmax(0, 1fr); } .overview-heading .actions { width: 100%; justify-content: flex-end; } }
</style>
