<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import ProfilesTab from '$lib/components/tabs/ProfilesTab.svelte';
  import SubscriptionsTab from '$lib/components/tabs/SubscriptionsTab.svelte';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  const navigation = useMobileNavigation();
  let segment = $state<'profiles' | 'subscriptions'>('profiles');
</script>
<section class="mobile-configuration" aria-label="配置与订阅">
  <nav class="configuration-segments" aria-label="配置页面分段">
    <SegmentedControl.Root value={segment} onValueChange={value=>segment=value as typeof segment} aria-label="配置页面分段" class="w-full"><SegmentedControl.Item value="profiles" class="flex-1">配置</SegmentedControl.Item><SegmentedControl.Item value="subscriptions" class="flex-1">订阅</SegmentedControl.Item></SegmentedControl.Root>
  </nav>
  <div class="configuration-tools"><Button variant="outline" onclick={() => navigation.open('rules')}>路由与规则集</Button><Button variant="outline" onclick={() => navigation.open('workspace')}>配置工作区</Button></div>
  <div class:hidden={segment!=='profiles'} class="configuration-page"><ProfilesTab mobile={true}/></div>
  <div class:hidden={segment!=='subscriptions'} class="configuration-page"><SubscriptionsTab mobile={true}/></div>
</section>
<style>
 .mobile-configuration { display:flex; flex-direction:column; min-width:0; min-height:0; height:100%; }
 .configuration-segments { display:flex; gap:4px; padding:4px; margin:8px 12px; border-radius:9px; background:var(--muted); }
 .configuration-segments :global(button) { flex:1; min-height:44px; border-radius:7px; }
 .configuration-segments :global(button[data-state='on']) { background:var(--card); color:var(--foreground); font-weight:600; }
 .configuration-tools { display:flex; flex-wrap:wrap; gap:8px; padding:0 12px 8px; }
 .configuration-tools :global(button) { min-height:44px; padding:8px 12px; color:var(--primary); border:1px solid var(--border); border-radius:9px; }
 .configuration-page { display:flex; flex:1; min-height:0; min-width:0; }
 .configuration-page :global(.profiles-root) { flex:1; min-width:0; padding:8px 12px; }
 .configuration-page.hidden { display:none; }
</style>
