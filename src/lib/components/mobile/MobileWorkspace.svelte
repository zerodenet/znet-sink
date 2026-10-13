<script lang="ts">
  import { onMount } from 'svelte';
  import ConfigEditorPanel from '$lib/components/settings/ConfigEditorPanel.svelte';
  import { configEditor } from '$lib/services/config-workspace.svelte';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  const navigation = useMobileNavigation();
  let confirmLeave = $state(false);
  let continueNavigation: (() => void) | null = null;
  onMount(() => navigation.guardBeforeLeave(next => {
    if (navigation.current?.page !== 'workspace' || !configEditor.dirty) return true;
    continueNavigation = next;
    confirmLeave = true;
    return false;
  }));
  function discard() {
    configEditor.reset();
    confirmLeave = false;
    const next = continueNavigation;
    continueNavigation = null;
    next?.();
  }
</script>
<ConfigEditorPanel />
<Dialog.Root bind:open={confirmLeave}>
  <Dialog.Content><Dialog.Header><Dialog.Title>放弃未保存的修改？</Dialog.Title><Dialog.Description>配置工作区有未保存草稿。继续编辑会保留草稿；放弃后返回已确认的编辑内容。</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={() => confirmLeave = false}>继续编辑</Button><Button variant="destructive" onclick={discard}>放弃修改</Button></Dialog.Footer></Dialog.Content>
</Dialog.Root>
