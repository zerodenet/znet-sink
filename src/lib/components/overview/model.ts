import type { ConnectionStatus, CoreOverview, PolicyGroup, PolicyOutbound, ProxyModeStatus, SelfTestSnapshot } from '$lib/types/gui-api';
import type { GuiManagedTunStatus } from '$lib/types/tun';

export type Destination = 'core' | 'tun' | 'dns' | 'network' | 'logs' | 'nodes' | 'profiles' | 'connections';
export interface Finding { title: string; detail: string; target: Destination; severity: 'error' | 'warning' }
export interface OverviewInput {
  now: number;
  connection: ConnectionStatus | null;
  connectionAt: number;
  connectionError: string | null;
  core: CoreOverview | null;
  tun: GuiManagedTunStatus | null;
  tunError: string | null;
  selfTest: SelfTestSnapshot | null;
  selfTestAt: number;
  mode: ProxyModeStatus | null;
  groups: PolicyGroup[];
  groupsAt?: number;
  groupsError?: string | null;
}
export function ageLabel(at: number, now: number): string {
  if (!at) return '尚未取得';
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  return seconds < 5 ? '刚刚更新' : seconds < 60 ? `${seconds} 秒前` : `${Math.floor(seconds / 60)} 分钟前`;
}
export function formatUptime(elapsedMs: number): string {
  if (!Number.isFinite(elapsedMs)) return '—';
  const total = Math.max(0, Math.floor(elapsedMs / 1000));
  const days = Math.floor(total / 86400);
  const hours = Math.floor(total / 3600) % 24;
  const minutes = Math.floor(total / 60) % 60;
  const seconds = total % 60;
  return [days ? `${days} 天` : '', days || hours ? `${hours} 小时` : '', total >= 60 ? `${minutes} 分` : '', `${seconds} 秒`].filter(Boolean).join(' ');
}
export function policyProbeLabel(outbound: PolicyOutbound, now: number, ready: boolean): string {
  const fresh = ready && outbound.lastCheckedUnixMs != null && now - outbound.lastCheckedUnixMs <= 300_000;
  if (!fresh) return '待探测';
  if (outbound.alive === false) return '探测失败';
  return outbound.alive === true && outbound.delayMs != null ? `${outbound.delayMs} ms` : '结果未知';
}

// Follow only confirmed single selections. A relay/load-balancer cannot be
// represented by one member's latency, and a cycle must not invent an exit.
export function selectedPolicyPath(group: PolicyGroup, groups: PolicyGroup[]): { path: string[]; probe?: PolicyOutbound } {
  const path: string[] = [];
  const seen = new Set<string>();
  let current = group;
  while (!seen.has(current.name)) {
    seen.add(current.name);
    if (!current.selected) return { path };
    path.push(current.selected);
    const member = current.outbounds.find((outbound) => outbound.tag === current.selected);
    if (!member) return { path };
    const child = groups.find((candidate) => candidate.name === member.tag);
    if (!child) return { path, probe: member };
    if (!['selector', 'urltest', 'url_test'].includes(child.kind?.toLowerCase() ?? '')) return { path };
    current = child;
  }
  return { path };
}

export function buildOverview(input: OverviewInput) {
  const { connection: c, tun, now } = input;
  const stale = !input.connectionAt || now - input.connectionAt > 15_000 || !!input.connectionError;
  const running = c?.processState === 'running';
  const ready = !stale && c?.coreAvailable === true;
  const groupsReady = ready && !input.groupsError && (input.groupsAt === undefined || (input.groupsAt > 0 && now - input.groupsAt <= 15_000));
  const groupsPending = ready && !groupsReady && !input.groupsError && input.groups.length > 0;
  const egress = presentEgress(tun);
  const findings: Finding[] = [];
  const add = (title: string, detail: string, target: Destination, severity: Finding['severity'] = 'warning') => {
    if (!findings.some((f) => f.detail === detail)) findings.push({ title, detail, target, severity });
  };
  if (input.connectionError) add('运行状态更新失败', input.connectionError, 'logs');
  else if (stale) add('运行状态尚未确认', '等待新的状态响应，保留的信息不能证明当前网络可用。', 'core');
  if (c?.processState === 'failed') add('内核进程异常退出', c.processExitReason || c.message || '查看退出原因和内核日志。', 'logs', 'error');
  else if (running && !c?.coreAvailable) add('进程存在，控制接口未就绪', '请检查内核启动和控制接口错误；进程运行不代表代理可用。', 'logs', 'error');
  if (!stale && !c?.coreAvailable && c?.systemProxyEnabled) add('系统代理已开启但内核未就绪', '代理请求可能无法转发；请启动内核或在代理设置中关闭系统代理。', 'network', 'error');
  if (input.tunError) add('TUN 状态无法确认', input.tunError, 'tun');
  else if (tun?.enabled && !tun.healthy) add('TUN 已开启但不健康', tun.lastError || '检查路由、权限和实际出口。', 'tun', 'error');
  else if (tun && !tun.enabled && tun.lastError) add('TUN 停止后仍有错误', tun.lastError, 'tun', 'error');
  else if (ready && tun?.desiredEnabled && !tun.enabled) add('TUN 尚未按预期启动', tun.lastError || '期望开启，内核尚未确认接管。', 'tun', 'error');
  if (ready && tun?.enabled && !input.tunError) {
    if (egress.issue && tun.healthy) add(egress.issue.title, egress.issue.detail, 'tun');
  }
  // Self-test is a separate observation: only fresh failures affect the header.
  if (input.selfTestAt && now - input.selfTestAt <= 60_000) {
    for (const detail of input.selfTest?.blockingIssues ?? []) add('就绪检查未通过', detail, input.selfTest?.activeProxyConfigId ? 'logs' : 'profiles', 'error');
    for (const check of input.selfTest?.checks ?? []) {
      if (check.status === 'warn') add('就绪检查提醒', check.message || check.key, check.key === 'internetSharing' ? 'network' : 'logs');
    }
  }
  // A paused background poll makes the last policy snapshot old without
  // proving a fault. Keep old selections unconfirmed, but only surface an
  // actionable warning when the policy read itself failed.
  if (ready && input.groupsError) add('策略状态更新失败', input.groupsError, 'nodes');
  const groups = input.groups.map((g) => {
    const resolved = selectedPolicyPath(g, input.groups);
    const selected = resolved.probe;
    const fresh = groupsReady && selected?.lastCheckedUnixMs != null && now - selected.lastCheckedUnixMs <= 5 * 60_000;
    return {
      name: g.name, kind: g.kind ?? '策略组', selected: groupsReady ? g.selected ?? '等待选择' : '待内核确认',
      health: !fresh ? '未取得近期探测' : selected?.alive === false ? '最近探测失败' : selected?.alive === true ? '最近探测成功' : '探测结果未知',
      delay: fresh && selected?.alive === true && selected.delayMs != null ? `${selected.delayMs} ms` : '—',
      failed: fresh && selected?.alive === false,
      selectionLabel: groupsReady ? resolved.path.join(' → ') || '等待选择' : '待内核确认',
      selectedTag: groupsReady ? g.selected ?? '' : '',
      switchable: g.kind?.toLowerCase() === 'selector',
      options: g.outbounds.map((outbound) => {
        const resolved = selectedPolicyPath({ ...g, selected: outbound.tag }, input.groups);
        return { value: outbound.tag, label: `${resolved.path.join(' → ') || outbound.tag} · ${resolved.probe ? policyProbeLabel(resolved.probe, now, groupsReady) : '待探测'}` };
      }),
    };
  }).sort((a, b) => Number(b.failed) - Number(a.failed));
  const failed = groups.filter((g) => g.failed);
  if (failed.length) add('已选出口最近探测失败', failed.map((g) => `${g.name} → ${g.selected}`).join('；'), 'nodes');
  const tone = findings.some((f) => f.severity === 'error') ? 'error' : findings.length ? 'warning' : groupsPending ? 'neutral' : ready ? 'good' : 'neutral';
  const title = tone === 'error' ? '需要处理运行异常' : tone === 'warning' ? '有待确认的运行状态' : groupsPending ? '策略状态正在更新' : ready ? '内核控制面就绪' : c?.processState === 'starting' ? '内核正在启动' : '内核已停止';
  const proxy = stale ? '状态待确认' : c?.systemProxyEnabled ? '已开启' : '未开启';
  const tunLabel = input.tunError || stale ? '状态待确认' : !ready ? '内核未就绪' : !tun ? '尚未取得' : !tun.supported ? '不支持' : tun.enabled ? tun.healthy && !egress.issue ? '已开启 · 健康' : '已开启 · 异常' : tun.lastError ? '已停止 · 待处理' : '未开启';
  const endpoint = c?.localProxyHost && c.localProxyPort ? `${c.localProxyHost.includes(':') ? `[${c.localProxyHost}]` : c.localProxyHost}:${c.localProxyPort}` : '尚未取得';
  const family = (key: 'ipv4Egress' | 'ipv6Egress') => {
    if (!ready || input.tunError || !tun?.enabled) return '—';
    const egress = tun[key];
    return egress?.availability === 'available' ? egress.interface ?? '可用' : egress?.availability === 'unavailable' ? '不可用' : '尚未确认';
  };
  return {
    ready, running, stale, title, tone, findings, groups, groupsReady, groupsPending, egress,
    tunSnapshot: tun,
    tunConfirmed: ready && !!tun && !input.tunError,
    availableModes: input.mode?.availableModes ?? [],
    freshness: ageLabel(input.connectionAt, now),
    source: input.selfTest?.activeProxyConfigName ? `${input.selfTest.activeProxyConfigName}${!input.selfTestAt || now - input.selfTestAt > 60_000 ? '（上次检查）' : ''}` : '尚未确认活动配置',
    mode: input.mode?.currentMode ?? '',
    version: input.core?.version ?? '—', pid: c?.processPid == null ? '—' : String(c.processPid),
    uptime: !stale && running && c?.startedAtUnixMs != null ? formatUptime(now - c.startedAtUnixMs) : '—',
    proxy, endpoint, tunLabel,
    tunDetails: tun?.enabled && ready && !input.tunError ? [tun.name, `MTU ${tun.mtu ?? '—'}`, tun.autoRoute ? '自动路由' : '手动路由', tun.strictRoute ? '严格路由' : null].filter(Boolean).join(' · ') : '接管状态由内核确认',
    dns: !ready || input.tunError || !tun?.enabled ? 'TUN 未确认接管' : !tun.dnsHijack ? '不拦截 · 跟随系统 DNS' : tun.fakeIpEnabled ? 'Fake-IP 拦截' : 'Real DNS 拦截',
    ipv4: family('ipv4Egress'), ipv6: family('ipv6Egress'),
    networkGeneration: !ready || input.tunError || !tun?.enabled ? '—' : String(tun.networkGeneration),
    selfTest: input.selfTest,
    selfTestAge: ageLabel(input.selfTestAt, now),
    selfTestStale: !input.selfTestAt || now - input.selfTestAt > 60_000,
    selfTestPassed: ready && groupsReady && !!input.selfTest?.ready && !!input.selfTestAt && now - input.selfTestAt <= 60_000 && !findings.length && input.selfTest.checks.every((check) => check.status === 'pass'),
  };
}
export type OverviewModel = ReturnType<typeof buildOverview>;

export function trafficUnavailableReason(model: OverviewModel, supported: boolean, sampledAt: number, live: boolean, now: number): string | null {
  return !supported ? '内核不支持流量查询' : !model.ready ? '内核未就绪，暂停展示实时速率'
    : !sampledAt ? '等待第一份流量采样' : now - sampledAt > 10_000 ? '流量采样已过期，等待恢复'
    : !live ? '正在建立流量采样基线' : null;
}

export function capturePresentation(model: OverviewModel, systemProxy: boolean, tunEnabled: boolean, tunDesired: boolean, busy: boolean) {
  const powerOn = systemProxy || tunEnabled || tunDesired;
  const stopError = model.tunConfirmed && !tunEnabled && !!model.tunSnapshot?.lastError;
  const tunFailed = model.tunSnapshot?.healthy === false || !!model.egress.issue;
  const healthy = model.ready && model.tunConfirmed && systemProxy && tunEnabled && model.tunSnapshot?.healthy === true && !tunFailed;
  const label = busy ? '正在更新代理状态'
    : model.stale || (model.ready && !model.tunConfirmed) ? '运行状态待确认'
    : stopError ? 'TUN 停止后仍有错误'
    : powerOn && !model.ready ? '内核未就绪'
    : tunEnabled && (model.tunSnapshot?.healthy !== true || tunFailed) ? 'TUN 运行异常'
    : healthy ? '系统代理与 TUN 已开启'
    : systemProxy ? '仅系统代理已开启'
    : tunEnabled ? '仅 TUN 已开启'
    : tunDesired ? 'TUN 等待恢复' : '代理已关闭';
  return { powerOn, healthy, label, warning: !busy && (model.stale || stopError || powerOn && !healthy), failed: stopError || model.tunConfirmed && tunEnabled && tunFailed };
}


/** Address preference permits fallback; a missing optional family is information. */
export function presentEgress(tun: GuiManagedTunStatus | null) {
  const v4 = tun?.ipv4Egress?.availability;
  const v6 = tun?.dualStack ? tun.ipv6Egress?.availability : 'unavailable';
  const policy = tun?.addressFamilyPolicy;
  const allowed4 = policy !== 'ipv6_only';
  const allowed6 = policy !== 'ipv4_only';
  const available = [allowed4 && v4 === 'available' ? 'IPv4' : '', allowed6 && v6 === 'available' ? 'IPv6' : ''].filter(Boolean);
  const unavailable = tun?.enabled && (!allowed4 || v4 === 'unavailable') && (!allowed6 || v6 === 'unavailable');
  const required = policy === 'ipv4_only' ? 'IPv4' : policy === 'ipv6_only' ? 'IPv6' : null;
  const issue = unavailable ? {
    title: required ? `${required} 出口不可用` : 'TUN 没有可用出口',
    detail: required === 'IPv4' ? tun.ipv4Egress?.reason || '当前策略仅使用 IPv4，但没有可用 IPv4 出口。'
      : required === 'IPv6' ? !tun.dualStack ? '当前策略仅使用 IPv6，但 TUN 未启用双栈。' : tun.ipv6Egress?.reason || '当前策略仅使用 IPv6，但没有可用 IPv6 出口。'
      : '当前没有可用出口，请检查本地网络、路由与地址族策略。',
  } : null;
  return { issue, summary: available.length ? `${available.join(' / ')} 出口可用` : unavailable ? '没有可用出口' : '出口状态待确认' };
}
