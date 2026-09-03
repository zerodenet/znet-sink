<script lang="ts">
  import RulesTab from '$lib/components/tabs/RulesTab.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Choice } from '$lib/components/ui/choice';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Switch } from '$lib/components/ui/switch';
  import { onMount } from 'svelte';
  let dark = $state(false);
  let showDialog = $state(false);
  let saved = $state('');
  let choice = $state('0');
  let radio = $state('keep');
  let checked = $state(false);
  const options = Array.from({ length: 80 }, (_, index) => ({ value: String(index), label: `选项 ${index}` }));
  $effect(() => { document.documentElement.classList.toggle('dark', dark); });
  onMount(() => {
    const save = (event: Event) => { saved = JSON.stringify((event as CustomEvent).detail); };
    window.addEventListener('fixture-save', save);
    return () => window.removeEventListener('fixture-save', save);
  });
</script>

<main class="flex min-h-screen flex-col gap-4 p-4">
  <section class="flex flex-wrap items-center gap-3" aria-label="基础控件">
    <Button onclick={() => dark = !dark}>切换主题</Button>
    <Button variant="outline" onclick={() => showDialog = true}>更多选项</Button>
    <Button disabled>禁用操作</Button>
    <Input class="w-36" aria-label="测试数字" type="number" min={0} value={10} />
    <Input class="w-36" aria-label="禁用输入" disabled value="不可编辑" />
    <Textarea class="max-w-48" aria-label="多行文本" />
    <label class="flex items-center gap-2"><Choice bind:checked />保留来源</label>
    <label class="flex items-center gap-2"><Choice type="radio" name="delete-mode" checked={radio === 'keep'} onchange={() => radio = 'keep'} />保留配置</label>
    <label class="flex items-center gap-2"><Choice type="radio" name="delete-mode" checked={radio === 'remove'} onchange={() => radio = 'remove'} />删除配置</label>
    <Switch aria-label="测试开关" />
  </section>
  <div class="flex h-[650px] min-h-0 flex-col"><RulesTab /></div>
  <output aria-label="保存结果">{saved}</output>
</main>

<Dialog.Root bind:open={showDialog}>
  <Dialog.Content>
    <Dialog.Header><Dialog.Title>长菜单测试</Dialog.Title><Dialog.Description>只验证本地控件</Dialog.Description></Dialog.Header>
    <Dialog.Body><FieldSelect bind:value={choice} {options} aria-label="长菜单" /></Dialog.Body>
  </Dialog.Content>
</Dialog.Root>
