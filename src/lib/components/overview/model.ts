import type { ConnectionStatus, CoreOverview, PolicyGroup, ProxyModeStatus, SelfTestSnapshot } from '$lib/types/gui-api';
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
}
export function ageLabel(at: number, now: number): string {
  if (!at) return '尚未取得';
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  return seconds < 5 ? '刚刚更新' : seconds < 60 ? `${seconds} 秒前` : `${Math.floor(seconds / 60)} 分钟前`;
}
export function buildOverview(input: OverviewInput) {
  const { connection: c, tun, now } = input;
  const stale = !input.connectionAt || now - input.connectionAt > 15_000 || !!input.connectionError;
  const running = c?.processState === 'running';
  const ready = !stale && c?.coreAvailable === true;
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
  else if (ready && tun?.desiredEnabled && !tun.enabled) add('TUN 尚未按预期启动', tun.lastError || '期望开启，内核尚未确认接管。', 'tun', 'error');
  if (ready && tun?.enabled && !input.tunError) {
    if (tun.ipv4Egress?.availability === 'unavailable') add('IPv4 出口不可用', tun.ipv4Egress.reason || '当前没有可用 IPv4 出口。', 'tun');
    if (tun.dualStack && tun.ipv6Egress?.availability === 'unavailable') add('IPv6 出口不可用', tun.ipv6Egress.reason || '检查 IPv6 网络或地址族策略。', 'tun');
  }
  // Self-test is a separate observation: only fresh failures affect the header.
  if (input.selfTestAt && now - input.selfTestAt <= 60_000) {
    for (const detail of input.selfTest?.blockingIssues ?? []) add('就绪检查未通过', detail, input.selfTest?.activeProxyConfigId ? 'logs' : 'profiles', 'error');
    for (const check of input.selfTest?.checks ?? []) {
      if (check.status === 'warn') add('就绪检查提醒', check.message || check.key, check.key === 'internetSharing' ? 'network' : 'logs');
    }
  }
  const groups = input.groups.map((g) => {
    const selected = g.outbounds.find((o) => o.tag === g.selected);
    const fresh = ready && selected?.lastCheckedUnixMs != null && now - selected.lastCheckedUnixMs <= 5 * 60_000;
    return {
      name: g.name, kind: g.kind ?? '策略组', selected: ready ? g.selected ?? '等待选择' : '待内核确认',
      health: !fresh ? '未取得近期探测' : selected?.alive === false ? '最近探测失败' : selected?.alive === true ? '最近探测成功' : '探测结果未知',
      delay: fresh && selected?.alive === true && selected.delayMs != null ? `${selected.delayMs} ms` : '—',
      failed: fresh && selected?.alive === false,
    };
  }).sort((a, b) => Number(b.failed) - Number(a.failed));
  const failed = groups.filter((g) => g.failed);
  if (failed.length) add('已选出口最近探测失败', failed.map((g) => `${g.name} → ${g.selected}`).join('；'), 'nodes');
  const tone = findings.some((f) => f.severity === 'error') ? 'error' : findings.length ? 'warning' : ready ? 'good' : 'neutral';
  const title = tone === 'error' ? '需要处理运行异常' : tone === 'warning' ? '有待确认的运行状态' : ready ? '内核控制面就绪' : c?.processState === 'starting' ? '内核正在启动' : '内核已停止';
  const proxy = stale ? '状态待确认' : c?.systemProxyEnabled ? '已开启' : '未开启';
  const tunLabel = input.tunError || stale ? '状态待确认' : !ready ? '内核未就绪' : !tun ? '尚未取得' : !tun.supported ? '不支持' : tun.enabled ? tun.healthy ? '已开启 · 健康' : '已开启 · 异常' : '未开启';
  const endpoint = c?.localProxyHost && c.localProxyPort ? `${c.localProxyHost.includes(':') ? `[${c.localProxyHost}]` : c.localProxyHost}:${c.localProxyPort}` : '尚未取得';
  const family = (key: 'ipv4Egress' | 'ipv6Egress') => {
    if (!ready || input.tunError || !tun?.enabled) return '—';
    const egress = tun[key];
    return egress?.availability === 'available' ? egress.interface ?? '可用' : egress?.availability === 'unavailable' ? '不可用' : '尚未确认';
  };
  return {
    ready, running, stale, title, tone, findings, groups,
    freshness: ageLabel(input.connectionAt, now),
    source: input.selfTest?.activeProxyConfigName ? `${input.selfTest.activeProxyConfigName}${!input.selfTestAt || now - input.selfTestAt > 60_000 ? '（上次检查）' : ''}` : '尚未确认活动配置',
    mode: input.mode?.currentMode ?? '',
    version: input.core?.version ?? '—', pid: c?.processPid == null ? '—' : String(c.processPid),
    uptime: !stale && running && c?.startedAtUnixMs ? `${Math.floor(Math.max(0, now - c.startedAtUnixMs) / 60000)} 分钟` : '—',
    proxy, endpoint, tunLabel,
    tunDetails: tun?.enabled && ready && !input.tunError ? [tun.name, `MTU ${tun.mtu ?? '—'}`, tun.autoRoute ? '自动路由' : '手动路由', tun.strictRoute ? '严格路由' : null].filter(Boolean).join(' · ') : '接管状态由内核确认',
    dns: !ready || input.tunError || !tun?.enabled ? 'TUN 未确认接管' : !tun.dnsHijack ? '不拦截 · 跟随系统 DNS' : tun.fakeIpEnabled ? 'Fake-IP 拦截' : 'Real DNS 拦截',
    ipv4: family('ipv4Egress'), ipv6: family('ipv6Egress'),
    networkGeneration: !ready || input.tunError || !tun?.enabled ? '—' : String(tun.networkGeneration),
    selfTest: input.selfTest,
    selfTestAge: ageLabel(input.selfTestAt, now),
    selfTestStale: !input.selfTestAt || now - input.selfTestAt > 60_000,
  };
}
export type OverviewModel = ReturnType<typeof buildOverview>;
