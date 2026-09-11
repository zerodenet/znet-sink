import type { DnsCacheResult, DnsLookupResult, FakeIpLookupResult } from '$lib/types/diagnostics';
import type { GuiFakeIpClearResult, ToolJobSnapshot } from '$lib/types/gui-api';
import { ToolJobsState, type ToolJobPorts } from '$lib/features/tool-jobs/state.svelte';

interface Ports extends ToolJobPorts {
  getAppErrorMessage(error: unknown, fallback?: string): string;
  confirm(message: string): boolean;
}

function terminalMessage(job: ToolJobSnapshot, fallback: string): string | null {
  if (job.state === 'failed') return job.error?.message ?? fallback;
  if (job.state === 'timed_out') return `${fallback}：任务超时`;
  if (job.state === 'cancelled') return '任务已取消';
  if (job.state === 'invalidated_by_config_change') return '配置已切换，旧任务结果已失效';
  if (job.state === 'invalidated_by_core_restart') return '内核已重启，旧任务结果已失效';
  return null;
}

export class DnsState {
  dnsHost = $state('');
  dnsResult = $state<DnsLookupResult | null>(null);
  dnsError = $state<string | null>(null);
  fakeQuery = $state('');
  fakeDirection = $state<'domain' | 'ip'>('domain');
  fakeResult = $state<FakeIpLookupResult | null>(null);
  cacheResult = $state<DnsCacheResult | null>(null);
  fakeMessage = $state<string | null>(null);
  fakeError = $state<string | null>(null);
  readonly jobs: ToolJobsState;
  private readonly ports: Ports;
  private refreshedClearJobs = new Set<number>();
  private dnsStarting = $state(false);
  private fakeStarting = $state(false);
  private clearStarting = $state(false);

  constructor(ports: Ports) {
    this.ports = ports;
    this.jobs = new ToolJobsState(
      ['dns_lookup', 'dns_cache', 'fake_ip_lookup', 'fake_ip_clear'],
      ports,
      (job) => this.applyJob(job),
    );
    void this.jobs.init();
  }

  get dnsLoading(): boolean {
    return this.dnsStarting || this.jobs.active('dns_lookup').length > 0;
  }

  get fakeLoading(): boolean {
    return this.fakeStarting || this.jobs.active('fake_ip_lookup').length > 0
      || this.jobs.active('dns_cache').length > 0;
  }

  get fakeClearLoading(): boolean {
    return this.clearStarting || this.jobs.active('fake_ip_clear').length > 0;
  }

  dispose(): void {
    // Closing the page only releases its event subscription. The backend
    // retains execution and history for the next page instance to recover.
    this.jobs.dispose();
  }

  async runDns(): Promise<void> {
    const hostname = this.dnsHost.trim();
    if (!hostname || this.dnsLoading) return;
    this.dnsError = null;
    this.dnsResult = null;
    this.dnsStarting = true;
    try {
      await this.jobs.start({ kind: 'dns_lookup', params: { hostname } });
    } catch (error) {
      this.dnsError = this.ports.getAppErrorMessage(error, 'DNS 查询任务启动失败');
    } finally {
      this.dnsStarting = false;
    }
  }

  async runFakeIp(): Promise<void> {
    const query = this.fakeQuery.trim();
    if (!query || this.fakeLoading || this.fakeClearLoading) return;
    this.fakeError = null;
    this.fakeMessage = null;
    this.fakeResult = null;
    this.fakeStarting = true;
    try {
      await Promise.all([
        this.jobs.start({
          kind: 'fake_ip_lookup',
          params: this.fakeDirection === 'domain' ? { domain: query } : { ip: query },
        }),
        this.jobs.start({ kind: 'dns_cache', params: { limit: 50 } }),
      ]);
    } catch (error) {
      this.fakeError = this.ports.getAppErrorMessage(error, 'Fake-IP 诊断任务启动失败');
    } finally {
      this.fakeStarting = false;
    }
  }

  async clearFakeIp(scope: 'selected' | 'all'): Promise<void> {
    if (this.fakeClearLoading || this.fakeLoading) return;
    const query = this.fakeQuery.trim();
    if (scope === 'selected' && !query) {
      this.fakeError = '请先填写要删除的域名或 Fake-IP';
      return;
    }
    if (scope === 'all' && !this.ports.confirm('确认清空全部 Fake-IP 映射？\n\n应用可能仍缓存旧的虚拟地址，受影响连接需要重新解析 DNS。')) return;

    this.fakeError = null;
    this.fakeMessage = null;
    this.clearStarting = true;
    try {
      await this.jobs.start({
        kind: 'fake_ip_clear',
        params: scope === 'all'
          ? {}
          : this.fakeDirection === 'domain'
            ? { domain: query }
            : { ip: query },
      });
    } catch (error) {
      this.fakeError = this.ports.getAppErrorMessage(error, 'Fake-IP 清理任务启动失败');
    } finally {
      this.clearStarting = false;
    }
  }

  async cancel(jobId: number): Promise<void> {
    try {
      await this.jobs.cancel(jobId);
    } catch (error) {
      this.fakeError = this.ports.getAppErrorMessage(error, '取消诊断任务失败');
    }
  }

  private applyJob(job: ToolJobSnapshot): void {
    if (this.jobs.latest(job.kind)?.id !== job.id) return;
    if (job.kind === 'dns_lookup') {
      if (job.state === 'completed') {
        this.dnsResult = job.result as DnsLookupResult;
        this.dnsError = null;
      } else {
        this.dnsError = terminalMessage(job, 'DNS 查询失败');
      }
      return;
    }
    if (job.kind === 'dns_cache') {
      if (job.state === 'completed') this.cacheResult = job.result as DnsCacheResult;
      else this.fakeError = terminalMessage(job, 'DNS 缓存读取失败');
      return;
    }
    if (job.kind === 'fake_ip_lookup') {
      if (job.state === 'completed') {
        this.fakeResult = job.result as FakeIpLookupResult;
        this.fakeError = null;
      } else {
        this.fakeError = terminalMessage(job, 'Fake-IP 诊断失败');
      }
      return;
    }
    if (job.kind === 'fake_ip_clear') {
      if (job.state === 'completed') {
        const result = job.result as GuiFakeIpClearResult;
        this.fakeMessage = result.enabled
          ? `已删除 ${result.removedMappings} 个映射（${result.removedAddresses} 个地址），剩余 ${result.liveMappings} 个，隔离地址 ${result.retiredAddresses} 个`
          : '当前配置未启用 Fake-IP，没有可清理的映射';
        this.fakeResult = null;
        if (!this.refreshedClearJobs.has(job.id)) {
          this.refreshedClearJobs.add(job.id);
          void this.jobs.start({ kind: 'dns_cache', params: { limit: 50 } }).catch((error) => {
            this.fakeError = this.ports.getAppErrorMessage(error, 'DNS 缓存读取任务启动失败');
          });
        }
      } else {
        this.fakeError = terminalMessage(job, '清理 Fake-IP 缓存失败');
      }
    }
  }
}
