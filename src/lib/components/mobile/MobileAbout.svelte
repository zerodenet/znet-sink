<script lang="ts">
  import {onMount} from 'svelte';
  import {Button} from '$lib/components/ui/button';
  import {Input} from '$lib/components/ui/input';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import {ABOUT_IDENTITY,ABOUT_LINKS,MobileAboutController,unavailableKernelVersions,type AboutView,type AboutLink} from '$lib/features/mobile/about';
  import {mobileAboutGateway} from '$lib/services/mobile-about';
  import {projectLicense,loadLicenseInventories,licenseEvidenceLabels,type LicenseInventoryView,type LicenseEntry,type LicenseInventory} from '$lib/services/mobile-license-notices';
  import {useMobileNavigation} from '$lib/features/mobile/context';
  const navigation=useMobileNavigation();
  const controller=new MobileAboutController(mobileAboutGateway,next=>view=next);
  let view=$state<AboutView>(controller.view);
  let licenses=$state<LicenseInventoryView>({inventories:[],errors:[]});
  let licensesLoading=$state(true);
  let licenseSearch=$state('');
  let licenseFilter=$state('all');
  async function loadLicenses(){licensesLoading=true;licenses=await loadLicenseInventories();licensesLoading=false;}
  const rows=$derived.by(()=>{const records=new Map<string,{entry:LicenseEntry;inventory:LicenseInventory}>();for(const inventory of licenses.inventories)for(const entry of inventory.entries){const old=records.get(entry.id);if(!old||entry.evidence.length>old.entry.evidence.length)records.set(entry.id,{entry,inventory});}return [...records.values()].filter(({entry})=>(licenseFilter==='all'||entry.ecosystem===licenseFilter)&&`${entry.name} ${entry.declaredLicense??''} ${entry.reviewedLicenseMapping?.expression??''} ${entry.source}`.toLowerCase().includes(licenseSearch.toLowerCase()));});
  onMount(()=>{void controller.load();void loadLicenses();return()=>controller.dispose();});
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
    <p>此目录来自本次构建的模块、目标解析与原生依赖记录。解析依赖不代表最终链接保留；构建工具和系统框架单独标明。缺失声明或正文会显示原因。</p>
    <label>搜索依赖<Input class="min-h-11 text-base" aria-label="搜索依赖许可" type="search" bind:value={licenseSearch} placeholder="名称、许可或来源" /></label>
    <label>依赖范围<FieldSelect class="min-h-11 text-base" aria-label="依赖范围" bind:value={licenseFilter} options={[{value:'all',label:'全部'},{value:'npm',label:'前端'},{value:'cargo',label:'Rust'},{value:'maven',label:'Android'},{value:'swift',label:'Swift'},{value:'platform',label:'系统框架'},{value:'resource',label:'资源'}]} /></label>
    <Button variant="outline" disabled={licensesLoading} onclick={loadLicenses}>{licensesLoading?'读取许可目录…':'重新读取许可目录'}</Button>
    {#each licenses.errors as error}<p role="status">{error}</p>{/each}
    {#each licenses.inventories as inventory}<p>目标：{inventory.target.triple} · {inventory.target.profile} · {inventory.provenance.kind}；{inventory.entries.length} 条，{inventory.gaps.length} 项资料缺口。</p>{/each}
    <p aria-live="polite">显示 {rows.length} 个依赖</p>
    {#each rows as {entry,inventory} (entry.id)}
      <details><summary>{entry.name} · {entry.declaredLicense??'许可声明未提供'}</summary>
        {#if entry.reviewedLicenseMapping}<p>已审阅正文映射：{entry.reviewedLicenseMapping.expression}；包 manifest 未声明许可。此映射不替代原始声明，资料缺口保留。</p>{/if}
        <p class="address">{entry.ecosystem} · 版本 / 修订：{entry.version??'未提供'}<br />来源：{entry.source}</p>
        {#each entry.evidence as evidence}<p>{licenseEvidenceLabels[evidence.kind]??evidence.kind}：{evidence.detail}</p>{/each}
        {#if entry.missing.length}<p role="status">资料缺口：{entry.missing.join('；')}</p>{/if}
        {#each [...entry.licenseTextIds,...entry.noticeTextIds] as id}
          {@const material=inventory.texts.find(text=>text.id===id)}
          {#if material}<details><summary>{entry.licenseTextIds.includes(id)?'许可全文':'归属与附带通知'} · {material.name}</summary><p class="address">{material.source}<br />SHA-256：{material.sha256}</p>{#each entry.materialReferences?.filter(reference=>reference.id===id)??[] as reference}<p class="address">该组件材料来源：{reference.source} · {reference.name}</p>{/each}<pre>{material.text}</pre></details>{/if}
        {/each}
      </details>
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
  label { display:block; font-size:.85rem; }
</style>
