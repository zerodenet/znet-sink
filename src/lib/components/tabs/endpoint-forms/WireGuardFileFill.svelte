<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import type { UserEndpoint } from '$lib/features/endpoints/library';
  import { readWireGuardFile } from '$lib/features/endpoints/protocols/wireguard-file';
  let { endpoint = $bindable(), disabled, busy = $bindable(false), onFilled }: { endpoint: UserEndpoint; disabled: boolean; busy?: boolean; onFilled: () => void } = $props();
  let picker = $state<HTMLInputElement | null>(null);
  let error = $state('');
  let warnings = $state<string[]>([]);
  let alive = true;
  onMount(() => () => { alive = false; });
  async function select(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file || busy || disabled) return;
    busy = true; error = ''; warnings = [];
    const result = await readWireGuardFile(file);
    input.value = '';
    if (!alive) return;
    busy = false;
    if (!result.endpoint) { error = result.error; return; }
    warnings = result.warnings;
    endpoint = { ...result.endpoint, tag: endpoint.tag.trim() || result.endpoint.tag, enabled: endpoint.enabled };
    onFilled();
  }
</script>
<div class="grid gap-2">
  <Input bind:ref={picker} type="file" accept=".conf,.ini" class="sr-only" aria-label="从文件填写 WireGuard" disabled={disabled || busy} onchange={select} />
  <Button type="button" variant="outline" disabled={disabled || busy} onclick={() => picker?.click()}>从 .conf 文件填写</Button>
  <p class="text-xs text-muted-foreground">选择配置文件可替换下方连接参数，保留当前端点名称；保存前可继续修改。</p>
  {#if error}<p role="alert" class="text-xs text-destructive">{error}</p>{/if}
  {#each warnings as warning}<p role="note" class="text-xs text-muted-foreground">{warning}</p>{/each}
</div>
