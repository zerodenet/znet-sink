<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { onMount } from 'svelte';
  import { getMobilePlatformSnapshot, type MobilePlatformSnapshot } from '$lib/features/mobile/runtime';
  import { getVersion } from '@tauri-apps/api/app';
  import { getGuiCoreHealth } from '$lib/services/core';
  import { openExternalUrl } from '$lib/services/platform';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  import { mobileReleaseGateway } from '$lib/services/mobile-release';
  import { MobileReleaseBrowser, releaseComparison, type ReleaseBrowserState } from '$lib/features/mobile/release-browser';
  import { MobileReleasePreferences, type ReleasePreferenceState } from '$lib/features/mobile/release-preferences';
  import type { MobileReleaseChannel } from '$lib/types/mobile-release';

  let { kernel = false }: { kernel?: boolean } = $props();
  const navigation = useMobileNavigation();
  const source = $derived(kernel ? 'kernel' : 'client');
  const browser = new MobileReleaseBrowser(mobileReleaseGateway, value => catalogState = value, 'client');
  let catalogState = $state<ReleaseBrowserState>(browser.state);
  const preferences = new MobileReleasePreferences(mobileReleaseGateway, value => preferenceState = value);
  let preferenceState = $state<ReleasePreferenceState>(preferences.state);
  let clientVersion = $state<string | null>(null);
  let runningVersion = $state<string | null>(null);
  let platform = $state<MobilePlatformSnapshot | null>(null);
  let versionReading = $state(false);
  let versionError = $state<string | null>(null);
  let destroyed = false;
  let selectedTag = $state<string | null>(null);
  let linkError = $state<string | null>(null);
  const channel = $derived(kernel ? preferenceState.draft.kernelChannel : preferenceState.draft.clientChannel);
  const currentVersion = $derived(kernel ? runningVersion : clientVersion);
  const comparison = $derived(releaseComparison(catalogState.catalog, currentVersion));
  const selected = $derived(catalogState.catalog?.releases.find(release => release.tagName === selectedTag) ?? catalogState.catalog?.releases[0] ?? null);
  const busyPreference = $derived(preferenceState.phase !== 'ready');
  const preferencesChanged = $derived(preferenceState.snapshot !== null && JSON.stringify(preferenceState.draft) !== JSON.stringify(preferenceState.snapshot.preferences));
  const releaseUrl = $derived(`https://github.com/zerodenet/${kernel ? 'core' : 'znet-sink'}/releases`);
  const labels: { value: MobileReleaseChannel; label: string }[] = [{ value: 'stable', label: '稳定版' }, { value: 'preview', label: '候选版' }, { value: 'test', label: '开发版' }];

  $effect(() => { browser.select(source, channel); selectedTag = null; });
  async function readVersions() {
    if (versionReading || destroyed) return;
    versionReading = true; versionError = null;
    const results = await Promise.allSettled([getVersion(), getMobilePlatformSnapshot()]);
    if (destroyed) return;
    clientVersion = results[0].status === 'fulfilled' ? results[0].value : null;
    platform = results[1].status === 'fulfilled' ? results[1].value : null;
    runningVersion = null;
    if (platform?.mobile === false) {
      try { const health = await getGuiCoreHealth(); if (!destroyed) runningVersion = health.engineVersion ?? null; } catch { /* unknown remains — */ }
    }
    if (destroyed) return;
    if (results.some(result => result.status === 'rejected')) versionError = '当前版本或平台分发状态读取失败，请重新核对。';
    versionReading = false;
  }
  async function openReleases(url = releaseUrl) {
    linkError = null;
    if (!/^https:\/\/github\.com\/zerodenet\/(?:core|znet-sink)\/releases(?:\/tag\/v?[0-9A-Za-z.+-]+)?$/.test(url)) {
      linkError = releaseUrl; return;
    }
    try { await openExternalUrl(url); } catch { if (!destroyed) linkError = url; }
  }
  function chooseChannel(value: string) {
    if (value !== 'stable' && value !== 'preview' && value !== 'test') return;
    preferences.edit({ ...preferenceState.draft, [kernel ? 'kernelChannel' : 'clientChannel']: value });
    browser.select(source, value); selectedTag = null;
  }
  function time(raw: string) { const value = Number(raw); return Number.isSafeInteger(value) && value > 0 ? new Date(value).toLocaleString() : '—'; }
  onMount(() => {
    void readVersions();
    void preferences.read().then(() => { if (!destroyed) { browser.select(source, channel); void browser.check(); } });
    return () => { destroyed = true; browser.destroy(); preferences.destroy(); };
  });
</script>

<section class="version-page" aria-label={kernel ? '内核版本管理' : '客户端升级管理'}>
  <dl>
    <div><dt>客户端版本</dt><dd>{clientVersion ?? '—'}</dd></div>
    {#if kernel}
      <div><dt>运行内核版本</dt><dd>{runningVersion ?? '—'}</dd></div>
      <div><dt>默认分发策略</dt><dd>计划随应用分发</dd></div>
      <div><dt>随包内核产物</dt><dd>{platform?.bundledKernelAvailable === true ? '平台确认可用' : platform?.bundledKernelAvailable === false ? '随包内核产物未接入' : '状态未确认'}</dd></div>
    {/if}
    <div><dt>安装能力</dt><dd>适配待完成</dd></div>
  </dl>
  {#if versionError}<p role="alert">{versionError}</p>{/if}
  <Button variant="outline" onclick={readVersions} disabled={versionReading}>{versionReading ? '正在核对…' : '重新核对版本'}</Button>
  <h2>发行渠道</h2>
  <SegmentedControl.Root value={channel} onValueChange={chooseChannel} disabled={busyPreference} aria-label="发行渠道" class="w-full">
    {#each labels as item}<SegmentedControl.Item value={item.value}>{item.label}</SegmentedControl.Item>{/each}
  </SegmentedControl.Root>
  <p>{kernel ? '候选版对应内核 beta，开发版对应 nightly；保留内核原渠道契约。' : '候选版对应 preview，开发版对应 test；渠道选择不授权安装。'}</p>
  {#if !kernel}
    <Button variant="outline" aria-pressed={preferenceState.draft.clientAutoCheck} disabled={busyPreference} onclick={() => preferences.edit({ ...preferenceState.draft, clientAutoCheck: !preferenceState.draft.clientAutoCheck })}>自动检查意图：{preferenceState.draft.clientAutoCheck ? '开启' : '关闭'}</Button>
    <p>仅保存客户端自动检查意图。移动后台调度尚未适配，当前只在打开此页或手动检查时读取目录；不承诺锁屏或退出后执行。</p>
  {/if}
  <Button variant="outline" disabled={busyPreference || !preferencesChanged} onclick={() => preferences.save()}>{preferenceState.phase === 'saving' ? '正在保存偏好…' : '保存发行偏好'}</Button>
  {#if preferenceState.error}<p role="alert">{preferenceState.error}</p>{/if}
  {#if preferenceState.notice}<p role="status">{preferenceState.notice}</p>{/if}
  {#if preferenceState.phase === 'error' || preferenceState.phase === 'unknown'}<Button variant="outline" onclick={() => preferences.read()}>{preferenceState.phase === 'unknown' ? '核对当前保存值' : '重新读取发行偏好'}</Button>{/if}

  <h2>发行目录</h2>
  <Button onclick={() => browser.check()} disabled={catalogState.phase === 'checking'}>{catalogState.phase === 'checking' ? '正在检查发行目录…' : '检查当前渠道'}</Button>
  {#if catalogState.error}<p role="alert">{catalogState.error}</p>{/if}
  {#if catalogState.catalog}
    <p>{catalogState.phase === 'error' || catalogState.phase === 'checking' ? '上次成功目录' : '最近检查'}：{time(catalogState.catalog.checkedAtUnixMs)}。本次读取最多 {catalogState.catalog.catalogLimit} 条项目发行记录。</p>
    {#if catalogState.phase === 'ready'}
      <p role="status">{comparison === 'no-release' ? '当前渠道在本次目录中暂无发行记录。' : comparison === 'current-unknown' ? '当前版本未确认，无法判定是否有更新。' : comparison === 'available' ? '当前渠道发现较新的项目版本；手机安装能力仍未适配。' : '本次当前渠道目录中未发现更新版本。'}</p>
    {/if}
    <p>{catalogState.catalog.targetReason}</p>
    {#each catalogState.catalog.releases as release (release.tagName)}
      <Button variant="outline" aria-pressed={selected?.tagName === release.tagName} onclick={() => selectedTag = release.tagName}><span>{release.version}</span><span>{release.publishedAt ?? '发布时间未提供'}</span></Button>
    {/each}
    {#if selected}
      <h2>{selected.version} 发行说明</h2>
      <pre class="release-notes">{selected.notes ?? '本发行未提供说明正文。'}</pre>
      {#if selected.notesTruncated}<p>发行说明超过展示预算，正文已截断；可打开项目页面查看全文。</p>{/if}
      <Button variant="outline" onclick={() => selected && openReleases(selected.releaseUrl)}>打开此版本项目页面</Button>
    {/if}
  {/if}
  <p>{kernel ? '默认移动内核通过客户端升级更新。运行服务、实例重建与重启核对尚未适配，当前不执行独立内核下载、安装或重启。' : '商店、测试发行及系统安装器尚未接入。本页读取项目发行资料，不会调用桌面 updater 或执行下载、安装。'}</p>
  <Button variant="outline" onclick={() => openReleases()}>查看项目发行页面</Button>
  {#if kernel}<Button variant="outline" onclick={() => navigation.open('updates')}>通过客户端升级更新内核</Button>{/if}
  {#if linkError}<p role="alert">系统浏览器打开失败，可复制发行页面地址：</p><Input readonly aria-label="发行页面地址" value={linkError} />{/if}
</section>
<style>
  .version-page { overflow:auto; padding:16px 14px 26px; }
  dl { border:1px solid var(--border); border-radius:12px; background:var(--card); padding:12px; }
  dl div { display:flex; justify-content:space-between; gap:12px; margin:8px 0; }
  dt,p { color:var(--muted-foreground); } dd { margin:0; overflow-wrap:anywhere; text-align:right; }
  h2 { margin:18px 0 9px; font-size:1rem; font-weight:500; }
  p { font-size:.85rem; line-height:1.6; } p[role="alert"] { color:var(--destructive); }
  .release-notes { white-space:pre-wrap; overflow-wrap:anywhere; font:inherit; font-size:.85rem; line-height:1.6; border:1px solid var(--border); border-radius:9px; padding:12px; }
  .version-page :global(button:not([data-slot='segmented-item'])) { display:flex; justify-content:space-between; gap:8px; white-space:normal; width:100%; min-height:44px; margin-top:10px; padding:10px; }
  .version-page :global([data-slot='segmented-item']) { flex:1; min-height:44px; font-size:.85rem; }
  .version-page :global(input) { width:100%; min-height:44px; font-size:1rem; }
  .version-page :global(button span) { overflow-wrap:anywhere; }
</style>
