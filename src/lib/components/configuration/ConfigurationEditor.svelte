<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { importProxyConfig, upsertProxyConfig } from '$lib/services/config';
  import { getAppErrorMessage } from '$lib/services/core';
  import type { ProxyConfigProfile } from '$lib/types/domain';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  let { profile = null, onsaved, onclose }: {
    profile?: ProxyConfigProfile | null; onsaved: () => Promise<void>; onclose: () => void;
  } = $props();
  let mode = $state('inline');
  let name = $state('');
  let path = $state('');
  let content = $state('{}');
  let saving = $state(false);
  let error = $state('');
  $effect(() => {
    name = profile?.name ?? ''; path = profile?.path ?? '';
    mode = profile?.path ? 'file' : 'inline';
    content = JSON.stringify(profile?.content ?? {}, null, 2);
  });
  async function save() {
    if (saving) return;
    saving = true; error = '';
    try {
      if (!name.trim()) throw new Error('请输入配置名称');
      const input = { id: profile?.id, name: name.trim(), active: profile?.active ?? false,
        kernel: profile?.kernel, format: profile?.format };
      if (mode === 'file') {
        if (!path.trim()) throw new Error('请选择配置文件');
        await importProxyConfig({ ...input, path: path.trim() });
      } else {
        const parsed = JSON.parse(content);
        if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('配置必须是 JSON 对象');
        await upsertProxyConfig({ ...input, content: parsed });
      }
      await onsaved(); onclose();
    } catch (cause) { error = getAppErrorMessage(cause, '保存配置失败'); }
    finally { saving = false; }
  }
</script>
<Dialog.Root open onOpenChange={(value) => { if (!value && !saving) onclose(); }}>
  <Dialog.Content>
    <Dialog.Header><Dialog.Title>{profile ? '编辑本地配置' : '添加本地配置'}</Dialog.Title><Dialog.Description>保存来源内容，客户端全局设置仍按原有优先级应用。</Dialog.Description></Dialog.Header>
    <Dialog.Body class="grid gap-4">
      <label class="grid gap-2">名称<Input bind:value={name} disabled={saving} /></label>
      <FieldSelect bind:value={mode} options={[{ value: 'inline', label: 'JSON 内容' }, { value: 'file', label: '本地文件' }]} disabled={saving} aria-label="本地配置来源" />
      {#if mode === 'file'}
        <label class="grid gap-2">文件路径<Input bind:value={path} disabled={saving} /></label>
        <Button variant="outline" disabled={saving} onclick={async () => {
          const result = await open({ multiple: false, directory: false, title: '选择配置文件' });
          if (typeof result === 'string') path = result;
        }}>选择文件</Button>
      {:else}<label class="grid gap-2">配置 JSON<Textarea bind:value={content} rows={12} disabled={saving} class="font-mono" /></label>{/if}
      {#if error}<p role="alert" class="text-destructive">{error}</p>{/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" disabled={saving} onclick={onclose}>取消</Button><Button disabled={saving} onclick={save}>{saving ? '保存中…' : '保存配置'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
