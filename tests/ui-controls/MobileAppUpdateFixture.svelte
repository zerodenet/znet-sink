<script lang="ts">
  import MobileAppUpdate from '$lib/components/mobile/MobileAppUpdate.svelte';
  const empty = new URLSearchParams(window.location.search).has('empty');
  const release = { tagName: 'v1.1.0', version: '1.1.0', channel: 'stable' as const, kernelChannel: null, publishedAt: null, notes: null, notesTruncated: false, releaseUrl: 'https://github.com/zerodenet/znet-sink/releases/tag/v1.1.0', androidAssets: empty ? [] : [{ assetId: '10', name: 'client-x86.apk', sizeBytes: '100', sha256: 'a'.repeat(64), url: 'https://github.com/zerodenet/znet-sink/releases/download/v1.1.0/client-x86.apk' }] };
</script>
<div style="max-width:404px;margin:auto;padding:14px;min-width:0"><MobileAppUpdate {release} channel="stable" catalogCurrent={true} /></div>
