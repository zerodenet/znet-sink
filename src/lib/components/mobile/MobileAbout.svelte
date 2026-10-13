<script lang="ts">
  import {onMount} from 'svelte';
  import {Button} from '$lib/components/ui/button';
  import {ABOUT_IDENTITY,ABOUT_LINKS,MobileAboutController,unavailableKernelVersions,type AboutView,type AboutLink} from '$lib/features/mobile/about';
  import {mobileAboutGateway} from '$lib/services/mobile-about';
  import {projectLicense,mobileLicenseNotices,licenseInventoryGap} from '$lib/services/mobile-license-notices';
  import {useMobileNavigation} from '$lib/features/mobile/context';
  const navigation=useMobileNavigation();
  const controller=new MobileAboutController(mobileAboutGateway,next=>view=next);
  let view=$state<AboutView>(controller.view);
  onMount(()=>{void controller.load();return()=>controller.dispose();});
</script>
<section class="mobile-about" aria-label="关于 ZNet Sink">
  <div class="about-card">
    <h2>ZNet Sink</h2>
    <p>零域网络代理客户端</p>
    <dl>
      <div><dt>应用名称</dt><dd>{view.name ?? '—'}</dd></div>
      <div><dt>客户端版本</dt><dd>{view.version ?? '—'}</dd></div>
      <div><dt>作者</dt><dd>{ABOUT_IDENTITY.author}</dd></div>
      <div><dt>应用标识</dt><dd>{view.identifier ?? '—'}</dd></div>
      <div><dt>项目标识</dt><dd>{ABOUT_IDENTITY.declaredIdentifier}</dd></div>
      <div><dt>运行 Zero 版本</dt><dd>{unavailableKernelVersions.running.version ?? '—'}</dd></div>
      <div><dt>随包 Zero 版本</dt><dd>{unavailableKernelVersions.bundled.version ?? '—'}</dd></div>
    </dl>
    <p>{unavailableKernelVersions.running.reason} {unavailableKernelVersions.bundled.reason}</p>
    {#each view.errors as error}<p role="alert">{error}</p>{/each}
    <Button variant="outline" disabled={view.loading} onclick={()=>controller.load()}>{view.loading ? '正在读取版本…' : '重新核对应用信息'}</Button>
  </div>
  <div class="about-card">
    <h2>项目与反馈</h2>
    <p>反馈页面由系统浏览器打开。你可以先准备复现描述与脱敏诊断材料，再自行提交。</p>
    {#each Object.entries(ABOUT_LINKS) as [key,link]}
      <Button variant="outline" disabled={!!view.opening || view.copying} onclick={()=>controller.open(key as AboutLink)}>{view.opening===key ? '正在打开…' : link.label}</Button>
    {/each}
    <Button variant="outline" onclick={()=>navigation.open('diagnostics')}>准备脱敏诊断材料</Button>
    {#if view.linkMessage}<p role="status">{view.linkMessage}</p>{/if}
    {#if view.failedLink}
      <p class="address" aria-label="未打开的项目地址">{ABOUT_LINKS[view.failedLink].url}</p>
      <Button variant="outline" disabled={view.copying} onclick={()=>controller.copyFailedLink()}>{view.copying ? '正在复制…' : '复制地址'}</Button>
    {/if}
  </div>
  <div class="about-card">
    <h2>许可与依赖资料</h2>
    <details><summary>Mozilla Public License 2.0 全文</summary><p>{projectLicense.source}</p><pre>{projectLicense.text}</pre></details>
    <p>{licenseInventoryGap}</p>
    {#each mobileLicenseNotices as notice}
      <details><summary>{notice.name} · {notice.license}</summary><p class="address">版本 / 来源修订：{notice.version}</p><pre class="notice">{notice.notice}</pre><pre>{notice.text}</pre></details>
    {/each}
  </div>
</section>
<style>
  .mobile-about { min-width:0; padding:16px 14px calc(26px + env(safe-area-inset-bottom)); color:var(--foreground); }
  .about-card { min-width:0; margin:0 0 12px; padding:14px; border:1px solid var(--border); border-radius:12px; background:var(--card); }
  h2 { margin:0 0 8px; font-size:1.05rem; font-weight:500; }
  p { color:var(--muted-foreground); font-size:.85rem; line-height:1.6; overflow-wrap:anywhere; }
  p[role="alert"] { color:var(--destructive); }
  dl { margin:12px 0; } dl div { display:flex; flex-wrap:wrap; gap:5px 12px; margin:10px 0; }
  dt { flex:0 0 95px; color:var(--muted-foreground); font-size:.85rem; }
  dd { flex:1 1 105px; margin:0; text-align:right; font-size:.85rem; overflow-wrap:anywhere; }
  .mobile-about :global(button) { width:100%; min-height:44px; height:auto; margin-top:10px; white-space:normal; padding:10px 12px; }
  .address { user-select:text; overflow-wrap:anywhere; }
  details { min-width:0; border-top:1px solid var(--border); }
  summary { min-height:44px; padding:12px 0; cursor:pointer; font-size:.88rem; overflow-wrap:anywhere; }
  pre { white-space:pre-wrap; overflow-wrap:anywhere; font-size:.75rem; line-height:1.6; color:var(--foreground); max-width:100%; }
  .notice { color:var(--muted-foreground); }
</style>
