<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { onMount } from 'svelte';
  import { getMobilePlatformSnapshot, type MobilePlatformSnapshot } from '$lib/features/mobile/runtime';
  import { getVersion } from '@tauri-apps/api/app';
  import { getGuiCoreHealth, getAppErrorMessage } from '$lib/services/core';
  import { openExternalUrl } from '$lib/services/platform';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  let { kernel = false }: { kernel?: boolean } = $props();
  const navigation = useMobileNavigation();
  let clientVersion = $state<string | null>(null);
  let runningVersion = $state<string | null>(null);
  let platform = $state<(MobilePlatformSnapshot & { bundledKernelAvailable?: boolean }) | null>(null);
  let checking = $state(false);
  let error = $state<string | null>(null);
  let linkError = $state(false);
  async function refresh() {
    if (checking) return;
    checking = true; error = null;
    const results = await Promise.allSettled([getVersion(), getGuiCoreHealth(), getMobilePlatformSnapshot()]);
    if (results[0].status === 'fulfilled') clientVersion = results[0].value;
    if (results[2].status === 'fulfilled') platform = results[2].value;
    if (results[1].status === 'fulfilled') runningVersion = results[1].value.engineVersion ?? null;
    if (platform?.mobile && platform.bundledKernelAvailable !== true) runningVersion = null;
    const relevant = kernel ? results[1] : results[0];
    if (relevant.status === 'rejected') error = getAppErrorMessage(relevant.reason, '版本读取失败');
    if (results[2].status === 'rejected') error = getAppErrorMessage(results[2].reason, '平台分发状态未确认');
    checking = false;
  }
  const releaseUrl = $derived(`https://github.com/zerodenet/${kernel ? 'core' : 'znet-sink'}/releases`);
  async function openReleases() { linkError = false; try { await openExternalUrl(releaseUrl); } catch { linkError = true; } }
  onMount(() => { void refresh(); });
</script>
<section class="version-page">
  <dl><div><dt>客户端版本</dt><dd>{clientVersion ?? '—'}</dd></div>{#if kernel}<div><dt>运行内核版本</dt><dd>{runningVersion ?? '—'}</dd></div><div><dt>默认分发策略</dt><dd>计划随应用分发</dd></div><div><dt>随包内核产物</dt><dd>{platform?.bundledKernelAvailable === true ? '平台确认可用' : platform?.bundledKernelAvailable === false ? '随包内核产物未接入' : '状态未确认'}</dd></div>{/if}<div><dt>发行能力</dt><dd>适配待完成</dd></div></dl>
  <p>{kernel ? '默认策略为移动内核随应用更新；当前随包产物接入尚未完成。平台运行服务、实例重建与重启核对尚未适配，当前不执行独立内核安装或重启。' : '移动安装器、商店及测试发行流程尚未适配。下载与系统安装验收尚未完成，当前版本信息不代表已完成升级。'}</p>
  <p>发行渠道：稳定版、候选版、开发版。可查看项目实际发行资料；渠道安装仅在对应平台适配完成后开放。</p>
  {#if error}<p role="alert">{error}</p>{/if}
  <Button variant="outline" onclick={refresh} disabled={checking}>{checking ? '正在核对…' : '重新核对版本'}</Button>
  <Button variant="outline" onclick={openReleases}>查看发行说明</Button>
  {#if kernel}<Button variant="outline" onclick={() => navigation.open('updates')}>通过客户端升级更新内核</Button>{/if}
  {#if linkError}<p role="alert">系统浏览器打开失败，可复制发行页面地址：</p><Input readonly aria-label="发行页面地址" value={releaseUrl} />{/if}
</section>
<style>
  .version-page { overflow:auto; padding:16px 14px 26px; }
  dl { border:1px solid var(--border); border-radius:12px; background:var(--card); padding:12px; }
  dl div { display:flex; justify-content:space-between; gap:12px; margin:8px 0; }
  dt,p { color:var(--muted-foreground); } dd { margin:0; overflow-wrap:anywhere; text-align:right; }
  p { font-size:.85rem; line-height:1.6; } p[role="alert"] { color:var(--destructive); }
  .version-page :global(button) { display:block; width:100%; min-height:44px; margin-top:10px; padding:10px; border:1px solid var(--border); border-radius:9px; background:var(--card); color:var(--foreground); font:inherit; }
  .version-page :global(input) { width:100%; min-height:44px; font-size:1rem; }
</style>
