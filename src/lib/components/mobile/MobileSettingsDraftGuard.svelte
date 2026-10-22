<script lang="ts">
  import { onMount } from 'svelte';
  import { optionalMobileNavigation, optionalMobileRoute } from '$lib/features/mobile/context';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  let { dirty, busy, label, onDiscard }: { dirty: boolean; busy: boolean; label: string; onDiscard: () => void } = $props();
  const navigation = optionalMobileNavigation();
  const route = optionalMobileRoute();
  let open = $state(false);
  let waiting = $state(false);
  let pending: (() => void) | null = null;
  const active = () => !!navigation && !!route && navigation.activeTab === route.tab && navigation.current?.key === route.key;
  function cancel() { open = false; pending = null; waiting = false; }
  onMount(() => navigation?.guardBeforeLeave(next => {
    if (!active() || (!dirty && !busy)) return true;
    waiting = busy;
    pending = busy ? null : next;
    open = true;
    return false;
  }));
  $effect(() => { if (open && waiting && !busy) cancel(); });
  function discard() {
    if (busy || !active()) return;
    const next = pending;
    onDiscard();
    cancel();
    next?.();
  }
</script>
<Dialog.Root {open} onOpenChange={value => { if (!value) cancel(); }}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{waiting ? '设置正在提交' : '放弃未保存的设置？'}</Dialog.Title>
      <Dialog.Description>{waiting ? `${label}的原操作仍在等待结果。不会取消或重放，也不会在结果返回后自动离开。` : `${label}有未保存的草稿。继续编辑会保留输入；放弃只丢弃本地草稿，不会提交或恢复运行设置。`}</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer><Button variant="outline" onclick={cancel}>继续编辑</Button>{#if !waiting}<Button variant="destructive" disabled={busy} onclick={discard}>放弃修改</Button>{/if}</Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
