<script lang="ts">
  import { onMount } from 'svelte';
  import { Database, Plus, RefreshCw, ShieldCheck, Trash2, X } from '@lucide/svelte';
  import { listRuleSets, removeRuleSet, updateAllRuleSets, updateRuleSet, upsertRuleSet } from '$lib/services/config';
  import type { RuleSetProfile, ZeroRule, ZeroRuleType } from '$lib/types/domain';

  const RULE_TYPES: { value: ZeroRuleType; label: string; placeholder: string }[] = [
    { value: 'domain_exact', label: '精确域名', placeholder: 'api.example.com' },
    { value: 'domain_suffix', label: '域名后缀', placeholder: 'example.com' },
    { value: 'domain_keyword', label: '域名关键词', placeholder: 'special' },
    { value: 'ipv4_cidr', label: 'IPv4 CIDR', placeholder: '10.0.0.0/8' },
    { value: 'ipv6_cidr', label: 'IPv6 CIDR', placeholder: 'fd00::/8' }
  ];

  let items = $state<RuleSetProfile[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let updatingId = $state<string | null>(null);
  let updatingAll = $state(false);
  let error = $state('');
  let showEditor = $state(false);
  let editingId = $state<string | undefined>();
  let name = $state('');
  let mode = $state<'visual' | 'subscription'>('visual');
  let rules = $state<ZeroRule[]>([{ type: 'domain_suffix', value: '' }]);
  let sourceUrl = $state('');
  let sourceFormat = $state<'auto' | 'zero-rule-ir-v1' | 'clash-classical-yaml'>('auto');
  let updateIntervalSecs = $state(0);
  let userAgent = $state('');
  let retainSource = $state(false);

  onMount(load);

  async function load() {
    loading = true;
    try { items = await listRuleSets(); error = ''; }
    catch (cause) { error = String(cause); }
    finally { loading = false; }
  }

  function openNew() {
    editingId = undefined; name = ''; mode = 'visual';
    rules = [{ type: 'domain_suffix', value: '' }];
    sourceUrl = ''; sourceFormat = 'auto'; updateIntervalSecs = 0; userAgent = '';
    retainSource = false; error = ''; showEditor = true;
  }

  function openEdit(item: RuleSetProfile) {
    editingId = item.id; name = item.name; mode = 'visual';
    rules = item.semanticIr.rules.map((rule) => ({ ...rule }));
    sourceUrl = item.source?.url ?? ''; sourceFormat = item.source?.format ?? 'auto';
    updateIntervalSecs = item.source?.updateIntervalSecs ?? 0;
    userAgent = item.source?.userAgent ?? '';
    retainSource = !!item.source; error = ''; showEditor = true;
  }

  function addRule() { rules = [...rules, { type: 'domain_suffix', value: '' }]; }
  function removeRule(index: number) { rules = rules.filter((_, current) => current !== index); }
  function setRuleType(index: number, type: ZeroRuleType) { rules[index].type = type; rules = [...rules]; }
  function setRuleValue(index: number, value: string) { rules[index].value = value; rules = [...rules]; }
  function placeholder(type: ZeroRuleType) { return RULE_TYPES.find((item) => item.value === type)?.placeholder ?? ''; }

  async function save() {
    saving = true; error = '';
    try {
      if (mode === 'subscription' && !editingId) {
        await upsertRuleSet({ name: name.trim(), enabled: true, source: { url: sourceUrl.trim(), format: sourceFormat, updateIntervalSecs: updateIntervalSecs || undefined, userAgent: userAgent.trim() || undefined } });
      } else {
        const cleanRules = rules.map((rule) => ({ ...rule, value: rule.value.trim() })).filter((rule) => rule.value);
        await upsertRuleSet({
          id: editingId, name: name.trim(), enabled: true,
          semanticIr: { version: 1, name: name.trim(), rules: cleanRules },
          source: retainSource && sourceUrl ? { url: sourceUrl, format: sourceFormat, updateIntervalSecs: updateIntervalSecs || undefined, userAgent: userAgent.trim() || undefined } : undefined
        });
      }
      showEditor = false; await load();
    } catch (cause) { error = String(cause); }
    finally { saving = false; }
  }

  async function update(id: string) {
    updatingId = id; error = '';
    try { await updateRuleSet(id); }
    catch (cause) { error = String(cause); }
    finally { updatingId = null; await load(); }
  }

  async function remove(id: string) {
    try { await removeRuleSet(id); await load(); }
    catch (cause) { error = String(cause); }
  }

  async function updateAll() {
    updatingAll = true; error = '';
    try {
      const outcome = await updateAllRuleSets();
      await load();
      error = outcome.failed ? `${outcome.failed} 个来源更新失败，旧产物已保留` : '';
    } catch (cause) { error = String(cause); }
    finally { updatingAll = false; }
  }

  function formatBytes(value?: number) {
    if (!value) return '—';
    return value < 1024 ? `${value} B` : `${(value / 1024).toFixed(1)} KiB`;
  }
</script>

<div class="rules-page animate-fade-in">
  <header>
    <div><h2>规则资产</h2><p>以 Zero 内核语义统一管理；外部订阅只作为导入适配器。</p></div>
    <div class="header-actions"><button onclick={updateAll} disabled={updatingAll}><RefreshCw size={14} class={updatingAll ? 'spin' : ''} /> 更新全部</button><button class="primary" onclick={openNew}><Plus size={14} /> 新建规则资产</button></div>
  </header>

  <div class="boundary"><ShieldCheck size={17} /><div><strong>管理模型与运行产物分离</strong><span>可视化规则保存为 Zero Rule IR；每次保存或同步都会构建并完整校验新的不可变 ZRS 文件。</span></div></div>
  {#if error}<div class="error">{error}</div>{/if}

  {#if showEditor}
    <section class="editor">
      <div class="editor-title"><strong>{editingId ? '编辑规则资产' : '创建规则资产'}</strong><button class="icon" onclick={() => showEditor = false} aria-label="关闭"><X size={15} /></button></div>
      <label>资产名称<input bind:value={name} placeholder="例如：AI 服务" /></label>

      {#if !editingId}
        <div class="mode-tabs"><button class:active={mode === 'visual'} onclick={() => mode = 'visual'}>可视化创建</button><button class:active={mode === 'subscription'} onclick={() => mode = 'subscription'}>从订阅导入</button></div>
      {/if}

      {#if mode === 'subscription' && !editingId}
        <div class="source-grid">
          <label>订阅地址<input bind:value={sourceUrl} placeholder="https://example.com/rules.yaml" /></label>
          <label>来源适配器<select bind:value={sourceFormat}><option value="auto">自动识别</option><option value="zero-rule-ir-v1">Zero Rule IR v1</option><option value="clash-classical-yaml">Clash Classical YAML</option></select></label>
          <label>自动更新<select bind:value={updateIntervalSecs}><option value={0}>手动</option><option value={3600}>每小时</option><option value={21600}>每 6 小时</option><option value={86400}>每天</option></select></label>
          <label>User-Agent<input bind:value={userAgent} placeholder="留空使用客户端默认值" /></label>
        </div>
        <p class="hint">来源格式只用于下载转换。导入后统一显示为下面五种内核语义，不会交给内核。</p>
      {:else}
        <div class="rules-heading"><strong>内核语义规则</strong><button onclick={addRule}><Plus size={13} /> 添加一条</button></div>
        <div class="rule-list">
          {#each rules as rule, index (index)}
            <div class="rule-row">
              <select value={rule.type} onchange={(event) => setRuleType(index, event.currentTarget.value as ZeroRuleType)}>{#each RULE_TYPES as type}<option value={type.value}>{type.label}</option>{/each}</select>
              <input value={rule.value} oninput={(event) => setRuleValue(index, event.currentTarget.value)} placeholder={placeholder(rule.type)} />
              <button class="icon danger" onclick={() => removeRule(index)} aria-label="删除规则"><Trash2 size={14} /></button>
            </div>
          {/each}
        </div>
        {#if sourceUrl}
          <label class="retain"><input type="checkbox" bind:checked={retainSource} /> 保留订阅关联（下次同步会用订阅内容覆盖当前规则）</label>
        {/if}
      {/if}
      <div class="editor-actions"><button onclick={() => showEditor = false}>取消</button><button class="primary" onclick={save} disabled={saving || !name.trim() || (mode === 'subscription' && !sourceUrl.trim())}>{saving ? '转换并构建 ZRS…' : '保存并构建 ZRS'}</button></div>
    </section>
  {/if}

  {#if loading}
    <div class="empty">加载中…</div>
  {:else if items.length === 0}
    <div class="empty"><Database size={28} /><strong>尚无规则资产</strong><span>可视化创建内核语义规则，或从外部订阅导入。</span></div>
  {:else}
    <div class="list">
      {#each items as item (item.id)}
        <div class="asset-row" onclick={() => openEdit(item)} onkeydown={(event) => event.key === 'Enter' && openEdit(item)} role="button" tabindex="0">
          <div class="main">
            <div class="title"><strong>{item.name}</strong><span class:ready={!!item.artifact}>{item.artifact ? 'ZRS 已验证' : '无运行产物'}</span>{#if item.source}<span>订阅</span>{:else}<span>本地</span>{/if}</div>
            <div class="meta"><span>{item.semanticIr.rules.length} 条源规则</span><span>→</span><span>{item.artifact?.entryCount ?? 0} 个索引项</span><span>{formatBytes(item.artifact?.fileSize)}</span>{#if item.artifact}<code>CRC32 {item.artifact.checksum.toString(16).padStart(8, '0')}</code>{/if}{#if item.sourceState.contentBytes}<span>来源 {formatBytes(item.sourceState.contentBytes)}</span>{/if}</div>
            {#if item.source}<div class="source" title={item.source.url}>{item.source.url}</div>{/if}
            {#if item.lastError}<div class="item-error">同步失败，继续使用上一版 ZRS：{item.lastError}</div>{/if}
          </div>
          <div class="actions">
            {#if item.source}<button onclick={(event) => { event.stopPropagation(); update(item.id); }} disabled={updatingId === item.id} title="下载并重建"><RefreshCw size={14} class={updatingId === item.id ? 'spin' : ''} /></button>{/if}
            <button class="danger" onclick={(event) => { event.stopPropagation(); remove(item.id); }} title="删除"><Trash2 size={14} /></button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .rules-page{display:flex;flex:1;min-height:0;flex-direction:column;background:var(--background)}header{display:flex;align-items:center;justify-content:space-between;padding:14px 16px;border-bottom:1px solid var(--border)}.header-actions{display:flex;gap:7px}.header-actions button{display:inline-flex;align-items:center;gap:5px}h2{margin:0;font-size:15px}p{margin:3px 0 0;color:var(--muted-foreground);font-size:11px}button,input,select{font:inherit}button{border:1px solid var(--border);background:var(--muted);color:var(--foreground);border-radius:7px;padding:6px 10px;cursor:pointer}button:disabled{opacity:.45;cursor:not-allowed}.primary{display:inline-flex;align-items:center;gap:5px;background:var(--primary);color:var(--primary-foreground);border-color:transparent}.boundary{display:flex;gap:9px;margin:12px 16px 0;padding:10px 12px;border:1px solid color-mix(in srgb,var(--primary) 28%,var(--border));background:color-mix(in srgb,var(--primary) 7%,transparent);border-radius:9px;color:var(--primary)}.boundary div{display:flex;flex-direction:column;gap:2px;font-size:11px}.boundary span{color:var(--muted-foreground)}.error,.item-error{color:var(--destructive);font-size:11px}.error{margin:10px 16px 0;padding:8px;background:color-mix(in srgb,var(--destructive) 9%,transparent);border-radius:7px}.editor{margin:12px 16px 0;padding:13px;border:1px solid var(--border);border-radius:10px;background:var(--surface);display:flex;flex-direction:column;gap:10px}.editor-title,.editor-actions,.rules-heading{display:flex;align-items:center;justify-content:space-between}.icon{padding:5px}.danger:hover{color:var(--destructive)}label{display:flex;flex-direction:column;gap:4px;font-size:11px;color:var(--muted-foreground)}input,select{width:100%;box-sizing:border-box;border:1px solid var(--border);background:var(--background);color:var(--foreground);border-radius:7px;padding:7px 8px;outline:none}.mode-tabs{display:flex;gap:5px}.mode-tabs button.active{background:var(--primary);color:var(--primary-foreground);border-color:transparent}.source-grid{display:grid;grid-template-columns:2fr 1fr;gap:10px}.hint{margin:0}.rule-list{display:flex;flex-direction:column;gap:6px;max-height:260px;overflow:auto}.rule-row{display:grid;grid-template-columns:150px 1fr 32px;gap:7px}.retain{flex-direction:row;align-items:center}.retain input{width:auto}.editor-actions{justify-content:flex-end;gap:7px}.list{overflow:auto;padding:12px 16px;display:flex;flex-direction:column;gap:7px}.asset-row{display:flex;align-items:center;gap:10px;padding:11px 12px;border:1px solid var(--border);border-radius:9px;cursor:pointer}.asset-row:hover{background:var(--muted)}.main{flex:1;min-width:0}.title,.meta,.actions{display:flex;align-items:center;gap:7px}.title{font-size:12px}.title span{font-size:9px;padding:2px 5px;border-radius:4px;background:var(--muted);color:var(--muted-foreground)}.title span.ready{color:var(--success);background:color-mix(in srgb,var(--success) 12%,transparent)}.meta,.source{margin-top:5px;font-size:10px;color:var(--muted-foreground)}.source{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.actions button{padding:6px}.empty{flex:1;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:6px;color:var(--muted-foreground);font-size:11px}:global(.spin){animation:spin .8s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:720px){.source-grid{grid-template-columns:1fr}.rule-row{grid-template-columns:120px 1fr 32px}header{align-items:flex-start;gap:8px}}
</style>
