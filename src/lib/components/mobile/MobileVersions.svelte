<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import * as Dialog from '$lib/components/ui/dialog';
  import MobileSettingsDraftGuard from './MobileSettingsDraftGuard.svelte';
  import { getMobilePlatformSnapshot, type MobilePlatformSnapshot } from '$lib/features/mobile/runtime';
  import { getVersion } from '@tauri-apps/api/app';
  import { getGuiCoreHealth } from '$lib/services/core';
  import { openExternalUrl } from '$lib/services/platform';
  import { useMobileNavigation, optionalMobileRoute } from '$lib/features/mobile/context';
  import { mobileReleaseGateway } from '$lib/services/mobile-release';
  import { MobileReleaseBrowser, releaseComparison, type ReleaseBrowserState } from '$lib/features/mobile/release-browser';
  import { MobileReleasePreferences, type ReleasePreferenceState } from '$lib/features/mobile/release-preferences';
  import { MobileReleaseSchedule, releaseTime, releaseOutcome, type ReleaseScheduleState } from '$lib/features/mobile/release-schedule';
  import type { MobileReleaseChannel } from '$lib/types/mobile-release';

  let { kernel = false }: { kernel?: boolean } = $props();
  const navigation = useMobileNavigation();
  const route = optionalMobileRoute();
  let mounted = $state(false);
  const source = $derived(kernel ? 'kernel' : 'client');
  const browser = new MobileReleaseBrowser(mobileReleaseGateway, value => catalogState = value, 'client');
  let catalogState = $state<ReleaseBrowserState>(browser.state);
  const preferences = new MobileReleasePreferences(mobileReleaseGateway, value => preferenceState = value);
  let preferenceState = $state<ReleasePreferenceState>(preferences.state);
  const schedule = new MobileReleaseSchedule(mobileReleaseGateway, value => scheduleState = value);
  let scheduleState = $state<ReleaseScheduleState>(schedule.state);
  const savedSchedule = $derived(scheduleState.snapshot);
  let clientVersion = $state<string | null>(null);
  let runningVersion = $state<string | null>(null);
  let platform = $state<MobilePlatformSnapshot | null>(null);
  let versionReading = $state(false);
  let versionError = $state<string | null>(null);
  let destroyed = false;
  let selectedTag = $state<string | null>(null);
  let discardOpen = $state(false);
  let linkError = $state<string | null>(null);
  const channel = $derived(kernel ? preferenceState.draft.kernelChannel : preferenceState.draft.clientChannel);
  const currentVersion = $derived(kernel ? runningVersion : clientVersion);
  const comparison = $derived(releaseComparison(catalogState.catalog, currentVersion));
  const selected = $derived(catalogState.catalog?.releases.find(release => release.tagName === selectedTag) ?? catalogState.catalog?.releases[0] ?? null);
  const busyPreference = $derived(preferenceState.phase !== 'ready');
  const preferencesChanged = $derived(preferenceState.snapshot !== null && (preferenceState.draft.clientAutoCheck !== preferenceState.snapshot.preferences.clientAutoCheck || preferenceState.draft.clientChannel !== preferenceState.snapshot.preferences.clientChannel || preferenceState.draft.kernelChannel !== preferenceState.snapshot.preferences.kernelChannel));
  const preferenceBusy = $derived(preferenceState.phase === 'saving' || preferenceState.phase === 'confirming');
  const releaseUrl = $derived(`https://github.com/zerodenet/${kernel ? 'core' : 'znet-sink'}/releases`);
  const labels: { value: MobileReleaseChannel; label: string }[] = [{ value: 'stable', label: '稳定版' }, { value: 'preview', label: '候选版' }, { value: 'test', label: '开发版' }];

  $effect(() => { browser.select(source, channel); selectedTag = null; });
  $effect(() => {
    if (!kernel && scheduleState.phase === 'ready' && savedSchedule?.clientChannel === channel) {
      browser.acceptCached(savedSchedule.cachedCatalog, savedSchedule.cacheCurrent);
    }
  });
  $effect(() => {
    const active = !route || (navigation.activeTab === route.tab && navigation.current?.key === route.key);
    if (mounted) {
      if (active) { void schedule.read(); void preferences.read(); } else schedule.invalidate();
    }
  });
  async function discardAndReload() {
    if (preferenceState.phase !== 'ready') return;
    discardOpen = false;
    await preferences.discardAndReload();
  }
  async function savePreferences() { await preferences.save(); if (!destroyed) await schedule.read(); }
  async function confirmPreferences() { await preferences.confirm(); if (!destroyed) await schedule.read(); }
  async function checkCatalog() { await browser.check(); if (!destroyed && !kernel) await schedule.read(); }
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
  const time = releaseTime;
  onMount(() => {
    void readVersions();
    mounted = true;
    const resume = () => { if (document.visibilityState === 'visible') { void schedule.read(); void preferences.read(); } };
    document.addEventListener('visibilitychange', resume);
    let unlisten: UnlistenFn | null = null;
    void listen('mobile-release:schedule-updated', () => { void schedule.read(); }).then(stop => {
      if (destroyed) stop(); else unlisten = stop;
    }).catch(() => { /* Explicit refresh and foreground resume remain available. */ });
    return () => { destroyed = true; document.removeEventListener('visibilitychange', resume); unlisten?.(); browser.destroy(); preferences.destroy(); schedule.destroy(); };
  });
</script>

<MobileSettingsDraftGuard label="发行偏好" dirty={!['unknown', 'checking'].includes(preferenceState.phase) && preferencesChanged} busy={preferenceBusy} onDiscard={() => preferences.discardDraft()} />
<Dialog.Root bind:open={discardOpen}>
  <Dialog.Content class="release-discard-dialog">
    <Dialog.Header>
      <Dialog.Title>放弃发行偏好草稿并重新读取？</Dialog.Title>
      <Dialog.Description>只放弃此页的未保存草稿，再读取当前已保存的发行偏好。取消会保留输入；此操作不会保存、确认或重放原操作，也不会检查发行目录。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => discardOpen = false}>继续编辑</Button>
      <Button variant="destructive" disabled={preferenceState.phase !== 'ready'} onclick={discardAndReload}>放弃草稿并读取</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
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
    <p>保存后由宿主在前台执行已到期的客户端渠道检查；成功后间隔六小时，失败后至少等待十五分钟。系统后台调度尚未适配，不承诺锁屏或退出后执行。未保存的渠道不会改变自动检查。</p>
  {/if}
  <Button variant="outline" disabled={busyPreference || !preferencesChanged} onclick={savePreferences}>{preferenceBusy ? '正在保存偏好…' : '保存发行偏好'}</Button>
  <Button variant="outline" disabled={preferenceState.phase !== 'ready'} onclick={() => discardOpen = true}>放弃草稿并重新读取发行偏好</Button>
  {#if preferenceState.error}<p role="alert">{preferenceState.error}</p>{/if}
  {#if preferenceState.notice}<p role="status">{preferenceState.notice}</p>{/if}
  {#if preferenceState.phase === 'error'}<Button variant="outline" onclick={() => preferences.read()}>重新读取发行偏好</Button>{/if}
  {#if preferenceState.operation && ['unknown', 'checking', 'confirming'].includes(preferenceState.phase)}
    <p>原偏好操作：{preferenceState.operation.operationId}。当前值与原操作的确认状态分别核对。</p>
    <p>原操作渠道：{labels.find(item => item.value === preferenceState.operation?.preferences.clientChannel)?.label ?? '—'}；自动检查：{preferenceState.operation.preferences.clientAutoCheck ? '开启' : '关闭'}。</p>
    <Button variant="outline" disabled={preferenceState.phase !== 'unknown'} onclick={() => preferences.checkOperation()}>{preferenceState.phase === 'checking' ? '正在核对原操作…' : '核对原偏好操作'}</Button>
    {#if preferenceState.operation.state === 'saved-awaiting-confirm'}<Button variant="outline" disabled={preferenceState.phase !== 'unknown' || preferenceState.confirmAttempted || !preferenceState.operation.reviewBinding} onclick={confirmPreferences}>确认原操作本地回执</Button>{/if}
  {/if}
  {#if !kernel}
    <section class="schedule-facts" aria-label="前台自动检查状态">
      <h2>前台自动检查</h2>
      <p>此处为宿主实际状态；读取状态不会发起目录检查。</p>
      {#if scheduleState.error}<p role="alert">{scheduleState.error}</p>{/if}
      {#if savedSchedule}
        {#if scheduleState.phase === 'error'}<p>以下为上次读取的历史状态，当前执行情况未确认。</p>{/if}
        <dl>
          <div><dt>已保存渠道</dt><dd>{labels.find(item => item.value === savedSchedule.clientChannel)?.label ?? '—'}</dd></div>
          <div><dt>已保存自动检查</dt><dd>{savedSchedule.enabled ? '开启' : '关闭'}</dd></div>
          <div><dt>宿主状态</dt><dd>{savedSchedule.checking ? '正在检查' : savedSchedule.foreground ? '前台' : '非前台'}</dd></div>
          <div><dt>最近执行结果</dt><dd>{releaseOutcome(savedSchedule.outcome)}</dd></div>
          <div><dt>最近实际执行</dt><dd>{time(savedSchedule.lastAttemptedAtUnixMs)}</dd></div>
          <div><dt>最近成功</dt><dd>{time(savedSchedule.lastSuccessAtUnixMs)}</dd></div>
          <div><dt>下次到期</dt><dd>{time(savedSchedule.nextDueAtUnixMs)}</dd></div>
          <div><dt>失败后最早重试</dt><dd>{time(savedSchedule.retryAtUnixMs)}</dd></div>
        </dl>
        {#if savedSchedule.foregroundScheduling === 'not-native-host'}<p role="status">当前为桌面宿主；移动前台到期调度未在此宿主启用，桌面更新器继续沿用原独立流程。</p>{/if}
        {#if !savedSchedule.enabled}<p>已保存的自动检查处于关闭状态，不会发起自动目录请求。</p>{/if}
        {#if savedSchedule.reason}<p role="status">{savedSchedule.reason}</p>{/if}
        {#if savedSchedule.storageError}<p role="alert">调度记录存储异常；不能将当前值或旧目录当作调度成功。</p>{/if}
      {:else}<p>{scheduleState.phase === 'reading' ? '正在读取前台检查状态…' : '尚未取得前台检查状态。'}</p>{/if}
      <Button variant="outline" disabled={scheduleState.phase === 'reading'} onclick={() => schedule.read()}>重新读取前台状态</Button>
      <p>仅客户端已保存渠道参与到期检查。内核目录仍需手动检查；此处不下载或安装任何软件。</p>
    </section>
  {/if}

  <h2>发行目录</h2>
  <Button onclick={checkCatalog} disabled={catalogState.phase === 'checking'}>{catalogState.phase === 'checking' ? '正在检查发行目录…' : '检查当前渠道'}</Button>
  {#if catalogState.error}<p role="alert">{catalogState.error}</p>{/if}
  {#if catalogState.catalog}
    <p>{catalogState.phase === 'error' || catalogState.phase === 'checking' || !catalogState.catalogCurrent ? '上次成功目录（历史）' : '最近检查'}：{time(catalogState.catalog.checkedAtUnixMs)}。本次读取最多 {catalogState.catalog.catalogLimit} 条项目发行记录。</p>
    {#if catalogState.phase === 'ready' && catalogState.catalogCurrent}
      <p role="status">{comparison === 'no-release' ? '当前渠道在本次目录中暂无发行记录。' : comparison === 'current-unknown' ? '当前版本未确认，无法判定是否有更新。' : comparison === 'available' ? '当前渠道发现较新的项目版本；手机安装能力仍未适配。' : '本次当前渠道目录中未发现更新版本。'}</p>
    {/if}
    <p>{catalogState.catalog.reusedValidatedCache ? '服务器返回 304，已核对并复用原有效目录。' : '服务器返回 200，已读取本次目录。'}</p>
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
  :global(.release-discard-dialog) { max-height:calc(100dvh - 32px); overflow:auto; }
  :global(.release-discard-dialog button) { min-height:44px; white-space:normal; }
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
