<script lang="ts">
  import {Button} from '$lib/components/ui/button';
  import {provideMobileNavigation} from '$lib/features/mobile/context';
  import WorkspaceRouteFixture from './WorkspaceRouteFixture.svelte';
  import {workspaceFixture} from './workspace-core';
  const navigation = provideMobileNavigation();
  navigation.selectTab('more');
  navigation.open('workspace');
  function reopen() {
    navigation.selectTab('more');
    if (!navigation.current) navigation.open('workspace');
  }
</script>
<main class="workspace-fixture">
  <nav class="fixture-actions" aria-label="真实工作区导航">
    <Button variant="outline" onclick={() => navigation.back()}>返回上级</Button>
    <Button variant="outline" onclick={() => navigation.selectTab('nodes')}>切换节点</Button>
    <Button variant="outline" onclick={reopen}>回到工作区</Button>
  </nav>
  <output aria-label="导航位置">{navigation.activeTab}:{navigation.current?.page ?? 'root'}</output>
  {#if navigation.activeTab === 'more' && navigation.current?.page === 'workspace'}
    <div class="fixture-actions">
      <Button variant="outline" onclick={() => workspaceFixture.changeSource()}>外部同毫秒更新</Button>
      <Button variant="outline" onclick={() => workspaceFixture.changeSource(true)}>切换托管来源</Button>
      <Button variant="outline" onclick={() => { workspaceFixture.online=true; workspaceFixture.loseApply=true; }}>模拟回执丢失</Button>
    </div>
    {#key navigation.current.key}
      <WorkspaceRouteFixture route={{key:navigation.current.key,tab:'more',page:'workspace'}} />
    {/key}
  {:else}
    <p role="status">已离开配置工作区</p>
  {/if}
</main>
<style>.workspace-fixture{padding:12px;max-width:800px;margin:auto;min-width:0}.fixture-actions{display:flex;gap:6px;flex-wrap:wrap;margin-bottom:14px}.fixture-actions :global(button){min-height:44px}</style>
