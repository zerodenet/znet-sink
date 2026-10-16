<script lang="ts">
  import { useMobileNavigation } from '$lib/features/mobile/context';
  let { name }: { name: string } = $props();
  const navigation = useMobileNavigation();
  let query = $state('');
</script>
<div class="fixture-page">
  <h1>{name}</h1>
  <label>搜索{name}<input aria-label={`搜索${name}`} bind:value={query} /></label>
  <button onclick={() => navigation.open('settings')}>打开设置</button>
  <div class="long-content">保留滚动位置与输入</div>
  <p>页面末尾</p>
</div>
<style>
  .fixture-page { overflow:auto; padding:14px; flex:1; min-height:0; }
  label,input { display:block; width:100%; } input { box-sizing:border-box; }
  .long-content { height:1200px; padding-top:40px; }
  button { min-height:44px; }
</style>
