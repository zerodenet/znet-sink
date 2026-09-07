<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import * as Dialog from '$lib/components/ui/dialog';
  import type { OverviewModel } from './model';
  import type { OverviewActions, OverviewFeedback, OverviewNetwork } from './types';
  import OperationFeedback from './OperationFeedback.svelte';
  let { model, network, feedback, actions, busy, refreshing, canDisableTun, inspect = $bindable(false), policies = $bindable(false), version = $bindable(false), detailTab = $bindable('capture') }: {
    model: OverviewModel; network: OverviewNetwork; feedback: OverviewFeedback; actions: OverviewActions;
    busy: boolean; refreshing: boolean; canDisableTun: boolean; inspect?: boolean; policies?: boolean; version?: boolean; detailTab?: string;
  } = $props();
  const tun = $derived(model.tunSnapshot);
  const tunIssue = $derived(model.findings.find((finding) => finding.target === 'tun'));
  const source = $derived(tun?.configSource === 'profile' ? `配置：${tun.configSourceName ?? '当前配置'}` : tun?.configSource === 'app' ? 'ZNet-Sink 缺省' : tun?.configSource === 'runtime' ? '临时运行态' : '尚未取得');
  const probeSummary = $derived(!model.ready ? '等待内核确认' : !model.groups.length ? '没有运行策略组' : model.groups.some((group) => group.failed) ? `${model.groups.filter((group) => group.failed).length} 个已选出口探测失败` : model.groups.every((group) => group.health === '最近探测成功') ? '已选出口有近期成功探测' : '部分已选出口尚无近期探测');
  function navigate(target: Parameters<OverviewActions['navigate']>[0]) { inspect = policies = version = false; actions.navigate(target); }
</script>

<Dialog.Root bind:open={policies}>
  <Dialog.Content class="max-w-[520px]">
    <Dialog.Header><Dialog.Title>当前策略选择</Dialog.Title><Dialog.Description>按策略组切换出口，规则继续引用原策略组。自动选择组由内核决定出口。</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      {#each model.groups as group (group.name)}
        <div class="policy-edit">
          <div class="policy-name"><strong>{group.name}</strong><span>{group.switchable ? '手动选择' : ['urltest', 'url_test'].includes(group.kind.toLowerCase()) ? '自动测速' : '策略管理'}</span></div>
          {#if group.switchable}<FieldSelect aria-label={`${group.name} 当前出口`} bind:value={() => group.selectedTag, (value) => actions.choosePolicy(group.name, value)} disabled={busy || !model.groupsReady} options={group.options} placeholder="等待选择" />{:else}<p class="automatic-choice">{group.selectionLabel} · {group.delay}</p>{/if}
          <p>已确认：{group.selectionLabel} · {group.health}</p><OperationFeedback {feedback} target={`policy:${group.name}`} />
        </div>
      {:else}<p class="hint">{model.ready ? '当前没有运行策略组。静态直连出站不会生成策略组。' : '内核未就绪，暂时无法取得实际策略选择。'}</p>{/each}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" onclick={() => navigate('nodes')}>节点与测速</Button><Button onclick={() => policies = false}>完成</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={inspect}>
  <Dialog.Content class="max-w-[560px]">
    <Dialog.Header><Dialog.Title>网络检查</Dialog.Title><Dialog.Description>查看接管、解析和出口状态，按具体原因处理。状态响应：{model.freshness}。</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      <SegmentedControl.Root bind:value={detailTab} aria-label="检查项目" class="inspection-tabs">
        {#each [{value:'capture',label:'流量接管'},{value:'dns',label:'域名解析'},{value:'egress',label:'网络出口'},{value:'checks',label:'就绪检查'}] as tab}<SegmentedControl.Item value={tab.value}>{tab.label}</SegmentedControl.Item>{/each}
      </SegmentedControl.Root>
      <dl class="diagnostic-facts">
        {#if detailTab === 'capture'}
          <div><dt>系统代理</dt><dd>{model.proxy} · {model.endpoint}</dd></div>
          <div><dt>TUN 状态</dt><dd class:danger={!!tunIssue}>{model.tunLabel}{model.tunConfirmed && tun?.desiredEnabled && !tun.enabled ? ' · 期望开启' : ''}</dd></div>
          <div><dt>接管参数</dt><dd>{model.tunDetails}</dd></div>
          <div><dt>地址 / 配置来源</dt><dd>{model.tunConfirmed && tun?.enabled ? (tun.addresses?.join(' · ') || tun.addr || '—') : '—'} · {source}</dd></div>
          <div><dt>实际出口</dt><dd>IPv4：{model.ipv4} · IPv6：{model.ipv6}</dd></div>
          <div><dt>网络代次 / 回退</dt><dd>{model.networkGeneration} / IPv6 → IPv4 {model.tunConfirmed && tun?.enabled ? tun.ipv6ToIpv4Fallbacks : '—'} 次</dd></div>
          {#if tunIssue}<div><dt>异常原因</dt><dd class="danger">{tunIssue.detail}</dd></div>{/if}
        {:else if detailTab === 'dns'}
          <div><dt>当前解析路径</dt><dd>{model.dns}</dd></div><div><dt>DNS 拦截次数</dt><dd>{model.tunConfirmed && tun?.enabled ? tun.dnsHijackedQueries : '—'}</dd></div>
          <div><dt>判断范围</dt><dd>以上表示配置与接管状态，不能代替具体域名的解析测试。</dd></div>
        {:else if detailTab === 'egress'}
          <div><dt>IPv4 出口</dt><dd>{model.ipv4}</dd></div><div><dt>IPv6 出口</dt><dd>{model.ipv6}</dd></div>
          <div><dt>本地网络检测</dt><dd>{network.loading ? '检测中…' : network.error ?? (network.ip || '待检测')}{network.description ? ` · ${network.description}` : ''}</dd></div>
          <div><dt>检测范围</dt><dd>公网地址检测跟随本机当前网络与代理设置，不代表每个策略组的出口地址。</dd></div>
          <div><dt>策略组探测</dt><dd>{probeSummary}</dd></div>
        {:else}
          <div><dt>检查时间</dt><dd>{model.selfTestAge}{model.selfTestStale ? ' · 结果待刷新' : ''}</dd></div>
          {#each model.findings as finding}<div><dt>{finding.title}</dt><dd>{finding.detail}<Button variant="link" size="sm" onclick={() => navigate(finding.target)}>去处理</Button></dd></div>{/each}
          {#each model.selfTest?.checks ?? [] as check}<div><dt>{check.status === 'pass' ? '通过' : check.status === 'warn' ? '提醒' : '未通过'} · {check.key}</dt><dd>{check.message || '—'}</dd></div>{/each}
          {#if !model.selfTest}<div><dt>检查状态</dt><dd>尚未取得就绪检查结果，请重新检查。</dd></div>{/if}
        {/if}
      </dl>
      <OperationFeedback {feedback} target="checks" pendingLabel="正在读取运行状态与检测本地网络…" /><OperationFeedback {feedback} target="tun" />
    </Dialog.Body>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => navigate(detailTab === 'capture' ? 'tun' : detailTab === 'dns' ? 'dns' : 'network')}>相关设置</Button>
      {#if tunIssue && canDisableTun}<Button variant="outline" disabled={busy} onclick={actions.toggleTun}>关闭 TUN</Button>{/if}
      <Button variant="outline" disabled={refreshing || busy || network.loading} onclick={actions.refresh}>{refreshing ? '检查中…' : '重新检查'}</Button><Button onclick={() => inspect = false}>完成</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={version}>
  <Dialog.Content class="max-w-[460px]"><Dialog.Header><Dialog.Title>当前内核</Dialog.Title><Dialog.Description>当前运行信息与版本管理。</Dialog.Description></Dialog.Header><Dialog.Body><dl class="diagnostic-facts"><div><dt>运行版本</dt><dd>{model.version}</dd></div><div><dt>运行时长</dt><dd>{model.uptime}</dd></div></dl></Dialog.Body><Dialog.Footer><Button variant="outline" onclick={() => navigate('core')}>内核管理</Button><Button onclick={() => version = false}>关闭</Button></Dialog.Footer></Dialog.Content>
</Dialog.Root>

<style>
  .policy-edit + .policy-edit { margin-top:18px; }.policy-name { display:flex; justify-content:space-between; align-items:center; margin-bottom:8px; font-size:12px; }.policy-name span, .policy-edit p, .hint { color:var(--muted-foreground); font-size:11px; }.policy-edit p { margin:6px 0 0; }.policy-edit .automatic-choice { color:var(--foreground); font-size:12px; }
  .diagnostic-facts { margin:12px 0 0; }.diagnostic-facts > div { display:grid; grid-template-columns:100px minmax(0,1fr); gap:10px; padding:10px 0; border-bottom:1px solid var(--border); }dt { font-size:12px; color:var(--muted-foreground); }dd { font-size:12px; margin:0; overflow-wrap:anywhere; }.danger { color:var(--destructive); }
  :global(.inspection-tabs) { flex-wrap:wrap; }
</style>
