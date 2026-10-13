<script lang="ts">
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { store } from '$lib/services/store.svelte';
  import { applyTheme, type ThemeMode } from '$lib/services/theme.svelte';
  const themes: { value: ThemeMode; label: string }[] = [{ value: 'system', label: '跟随系统' }, { value: 'light', label: '浅色' }, { value: 'dark', label: '深色' }];
  async function chooseTheme(theme: ThemeMode) { applyTheme(theme); await store.persistTheme(theme); }
</script>
<section class="preferences" aria-label="应用偏好">
  <h2>外观</h2>
  <SegmentedControl.Root value={store.selectedTheme} onValueChange={value => chooseTheme(value as ThemeMode)} aria-label="外观主题" class="w-full">
    {#each themes as theme}<SegmentedControl.Item value={theme.value}>{theme.label}</SegmentedControl.Item>{/each}
  </SegmentedControl.Root>
  <h2>自动恢复</h2><p>平台运行服务及恢复意图尚未适配。系统撤权或主动断开后不会由页面发起自动恢复。</p>
  <h2>通知</h2><p>移动系统通知授权尚未适配。应用偏好不能代替系统授权，VPN 服务必需提示由平台宿主持有。</p>
</section>
<style>
  .preferences { padding:0 14px 24px; }
  h2 { margin:16px 0 8px; font-size:1rem; font-weight:500; }
  p { font-size:.85rem; color:var(--muted-foreground); line-height:1.6; }
</style>
