import { registerModule } from '$lib/features/diagnostics/registry';
import type { DnsCacheResult, DnsLookupResult, FakeIpLookupResult } from '$lib/types/diagnostics';
import type * as Core from './client';
import type { getAppErrorMessage } from '$lib/services/core';
interface Ports {
  guiDnsLookup: typeof Core.guiDnsLookup;
  guiFakeIpLookup: typeof Core.guiFakeIpLookup;
  guiDnsCache: typeof Core.guiDnsCache;
  guiClearFakeIp: typeof Core.guiClearFakeIp;
  getAppErrorMessage: typeof getAppErrorMessage;
  confirm(message: string): boolean;
}
export class DnsState {
  dnsHost = $state('');
  dnsLoading = $state(false);
  dnsResult = $state<DnsLookupResult | null>(null);
  dnsError = $state<string | null>(null);
  fakeQuery = $state('');
  fakeDirection = $state<'domain' | 'ip'>('domain');
  fakeLoading = $state(false);
  fakeClearLoading = $state(false);
  fakeResult = $state<FakeIpLookupResult | null>(null);
  cacheResult = $state<DnsCacheResult | null>(null);
  fakeMessage = $state<string | null>(null);
  fakeError = $state<string | null>(null);
  private releaseDiagnostic = registerModule('dns', () => ({id: 'dns', title: 'DNS 与 Fake-IP', state: this.dnsLoading || this.fakeLoading || this.fakeClearLoading ? 'busy' : this.dnsError || this.fakeError ? 'error' : this.dnsResult || this.fakeResult || this.cacheResult ? 'ready' : 'idle', summary: '按需查询；清理与读取互斥', error: this.dnsError || this.fakeError, facts: []}));
  private generation = 0;
  private ports: Ports;
  constructor(ports: Ports) { this.ports = ports; }
  dispose() {
    this.releaseDiagnostic(); this.generation++; this.dnsLoading = false; this.fakeLoading = false; this.fakeClearLoading = false; }
  async runDns() {
    const host = this.dnsHost.trim();
    if (!host || this.dnsLoading) return;
    this.dnsLoading = true;
    this.dnsError = null;
    this.dnsResult = null;
    const generation = this.generation;
    try {
      const result = await this.ports.guiDnsLookup(host);
      if (generation === this.generation) this.dnsResult = result;
    } catch (e) {
      if (generation !== this.generation) return;
      this.dnsError = this.ports.getAppErrorMessage(e, 'DNS 查询失败');
    } finally {
      if (generation === this.generation) {
        this.dnsLoading = false;
      }
    }
  }

  async runFakeIp() {
    const query = this.fakeQuery.trim();
    if (!query || this.fakeLoading || this.fakeClearLoading) return;
    this.fakeLoading = true;
    this.fakeError = null;
    this.fakeMessage = null;
    const generation = this.generation;
    try {
      const [result, cache] = await Promise.all([
        this.ports.guiFakeIpLookup(this.fakeDirection === 'domain' ? { domain: query } : { ip: query }),
        this.ports.guiDnsCache(undefined, 50),
      ]);
      if (generation !== this.generation) return;
      this.fakeResult = result; this.cacheResult = cache;
    } catch (e) {
      if (generation !== this.generation) return;
      this.fakeError = this.ports.getAppErrorMessage(e, 'Fake-IP 诊断失败');
    } finally {
      if (generation === this.generation) {
        this.fakeLoading = false;
      }
    }
  }

  async clearFakeIp(scope: 'selected' | 'all') {
    if (this.fakeClearLoading || this.fakeLoading) return;
    const query = this.fakeQuery.trim();
    if (scope === 'selected' && !query) {
      this.fakeError = '请先填写要删除的域名或 Fake-IP';
      return;
    }
    if (scope === 'all' && !this.ports.confirm('确认清空全部 Fake-IP 映射？\n\n应用可能仍缓存旧的虚拟地址，受影响连接需要重新解析 DNS。')) {
      return;
    }

    this.fakeClearLoading = true;
    this.fakeError = null;
    this.fakeMessage = null;
    const generation = this.generation;
    try {
      const result = await this.ports.guiClearFakeIp(
        scope === 'all'
          ? undefined
          : this.fakeDirection === 'domain'
            ? { domain: query }
            : { ip: query },
      );
      if (generation !== this.generation) return;
      this.fakeMessage = result.enabled
        ? `已删除 ${result.removedMappings} 个映射（${result.removedAddresses} 个地址），剩余 ${result.liveMappings} 个，隔离地址 ${result.retiredAddresses} 个`
        : '当前配置未启用 Fake-IP，没有可清理的映射';
      this.fakeResult = null;
      const cache = await this.ports.guiDnsCache(undefined, 50);
      if (generation === this.generation) this.cacheResult = cache;
    } catch (e) {
      if (generation !== this.generation) return;
      this.fakeError = this.ports.getAppErrorMessage(e, '清理 Fake-IP 缓存失败');
    } finally {
      if (generation === this.generation) {
        this.fakeClearLoading = false;
      }
    }
  }

}
