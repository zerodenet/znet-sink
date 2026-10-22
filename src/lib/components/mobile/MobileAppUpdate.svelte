<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { androidUpdateGateway } from '$lib/services/mobile-release';
  import { AndroidUpdateController, androidUpdateLabel, androidUpdateReason, type AndroidUpdateView } from '$lib/features/mobile/android-update';
  import type { MobileRelease, MobileReleaseChannel, AndroidReleaseAsset } from '$lib/types/mobile-release';
  let { release, channel, catalogCurrent }: { release: MobileRelease | null; channel: MobileReleaseChannel; catalogCurrent: boolean } = $props();
  const owner = new AndroidUpdateController(androidUpdateGateway, value => view = value);
  let view = $state<AndroidUpdateView>(owner.state);
  let installReview = $state<{ operationId: string; version: string | null; code: string | null; sha256: string | null; packageName: string } | null>(null);
  let downloadReview = $state<{ asset: AndroidReleaseAsset; tag: string; channel: MobileReleaseChannel } | null>(null);
  const snapshot = $derived(view.snapshot);
  const ready = $derived(view.phase === 'ready');
  const canDownload = $derived(ready && snapshot && ['idle', 'cancelled', 'failed', 'installed-confirmed'].includes(snapshot.state));
  const active = $derived(snapshot && !['idle', 'cancelled', 'failed', 'installed-confirmed', 'store-managed'].includes(snapshot.state));
  function operationId() {
    const bytes = crypto.getRandomValues(new Uint8Array(16)); bytes[6] = (bytes[6] & 15) | 64; bytes[8] = (bytes[8] & 63) | 128;
    const s = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
    return `${s.slice(0,8)}-${s.slice(8,12)}-${s.slice(12,16)}-${s.slice(16,20)}-${s.slice(20)}`;
  }
  async function download() {
    const target = downloadReview; downloadReview = null;
    if (target) await owner.download(target.channel, target.tag, target.asset, operationId());
  }
  async function install() { const review = installReview; installReview = null; if (review) await owner.action('install', review.operationId); }
  onMount(() => {
    void owner.read();
    const resume = () => { if (document.visibilityState === 'visible') void owner.read(); };
    document.addEventListener('visibilitychange', resume);
    // Observation only: the OS job/session continues without this page. Never use this timer to retry a mutation.
    const observation = setInterval(() => {
      if (document.visibilityState === 'visible' && view.phase === 'ready' && active) void owner.read();
    }, 2000);
    return () => { clearInterval(observation); document.removeEventListener('visibilitychange', resume); owner.destroy(); };
  });
</script>

<section aria-label="Android 系统更新安装" class="app-update">
  <h2>Android 更新安装</h2>
  <p>下载由 Android 系统作业持有，离开页面不会取消。网络、锁屏、省电、配额及强制停止可能推迟执行；每次尝试有时间、容量和重试预算。</p>
  {#if view.error}<p role="alert">{view.error}</p>{/if}
  {#if snapshot}
    <p role="status">{androidUpdateLabel[snapshot.state]}</p>
    <dl>
      <div><dt>已安装包</dt><dd>{snapshot.packageName}</dd></div>
      <div><dt>已安装版本</dt><dd>{snapshot.installedVersion} ({snapshot.installedVersionCode})</dd></div>
      {#if snapshot.operationId}<div><dt>原操作</dt><dd>{snapshot.operationId}</dd></div>{/if}
      {#if snapshot.assetName}<div><dt>已固定目标</dt><dd>{snapshot.assetName}</dd></div>{/if}
      {#if snapshot.targetVersion}<div><dt>目标版本</dt><dd>{snapshot.targetVersion} ({snapshot.targetVersionCode ?? '下载后读取'})</dd></div>{/if}
      {#if snapshot.sizeBytes}<div><dt>实际下载</dt><dd>{snapshot.downloadedBytes} / {snapshot.sizeBytes} 字节</dd></div>{/if}
      {#if snapshot.reason}<div><dt>阶段说明</dt><dd>{androidUpdateReason(snapshot.reason)}</dd></div>{/if}
      {#if snapshot.installerStatus !== null}<div><dt>系统回执码</dt><dd>{snapshot.installerStatus}</dd></div>{/if}
    </dl>
    {#if snapshot.state === 'store-managed'}<Button variant="outline" disabled={!ready} onclick={() => owner.action('store')}>打开 Google Play 更新页面</Button>{/if}
    {#if snapshot.state === 'ready'}
      <p>已比较包名、版本、设备 API/ABI 及当前签名证书；APK 签名和安装许可仍由 Android 安装器最终验证。签名轮换暂不支持，证书不同将拒绝安装。</p>
      {#if snapshot.installPermission === 'blocked'}
        <p>系统尚未允许此应用请求安装。可以主动查看系统设置；应用不会修改未知来源开关。返回后请重新核对。</p>
        <Button variant="outline" disabled={!ready} onclick={() => owner.action('settings')}>查看此应用的系统安装设置</Button>
      {/if}
      {#if snapshot.installPermission === 'system-confirmation'}
        <p>Android 7 的未知来源安装由系统安全设置与安装器判断；当前没有把未知来源开关视为已授权。若系统阻止，可主动查看设置后返回核对。</p>
        <Button variant="outline" disabled={!ready} onclick={() => owner.action('settings')}>查看 Android 系统安全设置</Button>
      {/if}
      <Button disabled={!ready || snapshot.installPermission === 'blocked' || !snapshot.operationId} onclick={() => installReview = snapshot?.operationId ? { operationId: snapshot.operationId, version: snapshot.targetVersion, code: snapshot.targetVersionCode, sha256: snapshot.sha256, packageName: snapshot.packageName } : null}>审阅并交给系统安装器</Button>
    {/if}
    {#if snapshot.state === 'awaiting-user'}
      <p>系统确认只在点击下方按钮后打开。取消、系统验证拒绝或回执丢失会分别保留；提交成功仍需安装后版本核对。</p>
      <Button disabled={!ready || !snapshot.confirmationAvailable} onclick={() => owner.action('confirm')}>打开系统安装确认</Button>
    {/if}
    {#if active}<Button variant="outline" disabled={!ready} onclick={() => owner.action('cancel')}>取消原更新操作</Button>{/if}
  {/if}
  <Button variant="outline" disabled={view.phase === 'sending' || view.phase === 'reading'} onclick={() => owner.read()}>读取原更新状态及安装版本</Button>
  {#if release && snapshot?.state !== 'store-managed'}
    {#if release.androidAssets?.length}
      <p>以下为项目目录提供 SHA-256 的候选 APK。摘要仅验证资产一致性，手机兼容性与已安装签名在下载后核对；不会执行任意桌面二进制。</p>
      {#each release.androidAssets as asset (asset.assetId)}
        <Button variant="outline" disabled={!canDownload || !catalogCurrent} onclick={() => downloadReview = { asset: { ...asset }, tag: release!.tagName, channel }}>下载 {asset.name}（{asset.sizeBytes} 字节）</Button>
      {/each}
    {:else}<p>此版本没有带可信 GitHub SHA-256 摘要的 APK，不能开始下载或安装。项目发行页面仍可查看。</p>{/if}
  {/if}
</section>
<Dialog.Root open={downloadReview !== null} onOpenChange={open => { if (!open) downloadReview = null; }}>
  <Dialog.Content class="android-install-review"><Dialog.Header><Dialog.Title>下载此 Android 更新？</Dialog.Title><Dialog.Description>固定本次目录资产与摘要，下载后先检查兼容性，不会自动安装。系统可能推迟后台下载。</Dialog.Description></Dialog.Header>
    {#if downloadReview}<p>{downloadReview.asset.name}</p><p>SHA-256：{downloadReview.asset.sha256}</p>{/if}
    <Dialog.Footer><Button variant="outline" onclick={() => downloadReview = null}>取消</Button><Button onclick={download}>确认下载并检查</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<Dialog.Root open={installReview !== null} onOpenChange={open => { if (!open) installReview = null; }}>
  <Dialog.Content class="android-install-review"><Dialog.Header><Dialog.Title>将此更新交给 Android 安装器？</Dialog.Title><Dialog.Description>目标固定为已检查 APK。Android 仍会要求用户确认并执行系统校验；安装可能退出或重启应用。完成后核对实际包名、版本和签名，不自动重放安装。</Dialog.Description></Dialog.Header>
    {#if installReview}<p>{installReview.packageName} · {installReview.version} ({installReview.code})</p><p>SHA-256：{installReview.sha256 ?? '—'}</p>{/if}
    <Dialog.Footer><Button variant="outline" onclick={() => installReview = null}>返回</Button><Button disabled={!ready || snapshot?.state !== 'ready' || snapshot.operationId !== installReview?.operationId} onclick={install}>准备系统安装会话</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  .app-update { margin-top:18px; border-top:1px solid var(--border); padding-top:8px; }
  h2 { font-size:1rem; font-weight:500; } p,dt { color:var(--muted-foreground); font-size:.85rem; line-height:1.6; }
  dl div { display:flex; justify-content:space-between; gap:12px; margin:8px 0; } dd { margin:0; text-align:right; overflow-wrap:anywhere; min-width:0; }
  p[role='alert'] { color:var(--destructive); }
  .app-update :global(button) { min-height:44px; width:100%; white-space:normal; margin-top:10px; }
  :global(.android-install-review) { max-height:calc(100dvh - 32px); overflow:auto; overflow-wrap:anywhere; }
  :global(.android-install-review button) { min-height:44px; white-space:normal; }
</style>
