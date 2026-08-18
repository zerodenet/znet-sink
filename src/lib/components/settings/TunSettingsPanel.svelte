<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import { getAppConfig, getAppErrorMessage, updateAppConfig } from '$lib/services/core';
  import { guiState } from '$lib/services/gui-state.svelte';

  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state<string | null>(null);

  let name = $state('');
  let tag = $state('proxy');
  let addr = $state('10.0.0.1/24');
  let secondaryAddr = $state('');
  let mtu = $state('1500');
  let dualStack = $state(true);
  let dnsHijack = $state(false);

  const profileManaged = $derived(guiState.tunStatus?.configSource === 'profile');
  const profileSourceName = $derived(guiState.tunStatus?.configSourceName ?? '当前配置');
  // Profile-owned TUN does not consume these local defaults, so they remain
  // editable for the next profile that omits runtime.tun.
  const locked = $derived((guiState.isTunEnabled && !profileManaged) || saving);

  function markDirty() {
    saved = false;
    error = null;
  }

  async function load() {
    loading = true;
    error = null;
    try {
      const config = await getAppConfig();
      name = config.tun.name ?? '';
      tag = config.tun.tag;
      addr = config.tun.addr;
      secondaryAddr = config.tun.secondaryAddr ?? '';
      mtu = String(config.tun.mtu);
      dualStack = config.tun.dualStack;
      dnsHijack = config.tun.dnsHijack;
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载 TUN 配置失败');
    } finally {
      loading = false;
    }
  }

  async function save() {
    const normalizedAddr = addr.trim();
    const normalizedTag = tag.trim();
    const normalizedMtu = Number.parseInt(mtu, 10);
    const normalizedSecondary = secondaryAddr.trim();

    saved = false;
    error = null;

    if (!normalizedAddr || !normalizedAddr.includes('/')) {
      error = 'TUN 地址必须使用 CIDR 格式，例如 10.0.0.1/24';
      return;
    }
    if (!normalizedTag) {
      error = '入站标签不能为空';
      return;
    }
    if (!Number.isInteger(normalizedMtu) || normalizedMtu < 576 || normalizedMtu > 65535) {
      error = 'MTU 必须在 576 到 65535 之间';
      return;
    }
    if (normalizedSecondary && !normalizedSecondary.includes('/')) {
      error = '第二地址必须使用 CIDR 格式';
      return;
    }

    saving = true;
    try {
      const config = await updateAppConfig({
        tun: {
          name: name.trim() || null,
          tag: normalizedTag,
          addr: normalizedAddr,
          secondaryAddr: normalizedSecondary || null,
          mtu: normalizedMtu,
          dualStack,
          dnsHijack,
        },
      });
      name = config.tun.name ?? '';
      tag = config.tun.tag;
      addr = config.tun.addr;
      secondaryAddr = config.tun.secondaryAddr ?? '';
      mtu = String(config.tun.mtu);
      dualStack = config.tun.dualStack;
      dnsHijack = config.tun.dnsHijack;
      saved = true;
    } catch (cause) {
      error = getAppErrorMessage(cause, '保存 TUN 配置失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void Promise.allSettled([load(), guiState.refreshTunStatus()]);
  });
</script>

{#if profileManaged}
  <div class="settings-notice" role="status">
    {profileSourceName} 已显式定义 <code>runtime.tun</code>，当前运行时优先使用该配置。下方内容仅作为 ZNet-Sink 缺省值，在活动配置未定义 TUN 时生效。
  </div>
{:else if guiState.isTunEnabled}
  <div class="settings-notice" role="status">
    TUN 正在运行。当前启动参数已锁定，关闭 TUN 后可修改。
  </div>
{/if}

<div class="config-section">
  <div class="config-section-title">接口</div>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">网卡名称</span>
        <span class="label-desc">传递给 Zero 的 TUN interface name；留空时由 Zero 选择默认名称。</span>
      </div>
      <div class="field-control">
        <Input
          bind:value={name}
          oninput={markDirty}
          disabled={locked}
          placeholder="由 Zero 决定"
          spellcheck="false"
          aria-label="TUN 网卡名称"
        />
      </div>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">入站标签</span>
        <span class="label-desc">作为 TUN 流量进入 Zero 后使用的 inbound tag。</span>
      </div>
      <div class="field-control">
        <Input
          bind:value={tag}
          oninput={markDirty}
          disabled={locked}
          class="font-mono"
          spellcheck="false"
          aria-label="TUN 入站标签"
        />
      </div>
    </div>
  {/if}
</div>

<div class="config-separator"></div>

<div class="config-section">
  <div class="config-section-title">地址</div>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">主地址</span>
        <span class="label-desc">TUN 主接口地址，使用 CIDR 表示。</span>
      </div>
      <div class="field-control">
        <Input
          bind:value={addr}
          oninput={markDirty}
          disabled={locked}
          class="font-mono"
          placeholder="10.0.0.1/24"
          spellcheck="false"
          aria-label="TUN 主地址"
        />
      </div>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">第二地址</span>
        <span class="label-desc">双栈时可指定另一地址族的 CIDR；关闭双栈时保留该值但不会传给 Zero。</span>
      </div>
      <div class="field-control">
        <Input
          bind:value={secondaryAddr}
          oninput={markDirty}
          disabled={locked || !dualStack}
          class="font-mono"
          placeholder="自动"
          spellcheck="false"
          aria-label="TUN 第二地址"
        />
      </div>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">MTU</span>
        <span class="label-desc">传递给 Zero 的 TUN MTU，允许范围为 576–65535。</span>
      </div>
      <div class="field-control narrow">
        <Input
          type="number"
          min="576"
          max="65535"
          step="1"
          bind:value={mtu}
          oninput={markDirty}
          disabled={locked}
          class="font-mono"
          aria-label="TUN MTU"
        />
      </div>
    </div>
  {/if}
</div>

<div class="config-separator"></div>

<div class="config-section">
  <div class="config-section-title">接管</div>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">双栈接管</span>
        <span class="label-desc">让 Zero TUN 同时准备 IPv4 与 IPv6 接口地址和路由。</span>
      </div>
      <Switch
        checked={dualStack}
        onCheckedChange={(checked) => {
          dualStack = checked;
          markDirty();
        }}
        disabled={locked}
        aria-label="TUN 双栈接管"
      />
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">DNS 劫持</span>
        <span class="label-desc">让 Zero 接管经过 TUN 的 TCP/UDP 53；启用前需要在 Zero 配置中使用非系统 DNS。</span>
      </div>
      <Switch
        checked={dnsHijack}
        onCheckedChange={(checked) => {
          dnsHijack = checked;
          markDirty();
        }}
        disabled={locked}
        aria-label="TUN DNS 劫持"
      />
    </div>
  {/if}
</div>

{#if !loading}
  <div class="settings-actions">
    <Button size="sm" onclick={save} disabled={locked}>
      {saving ? '保存中...' : saved ? '已保存' : '保存'}
    </Button>
  </div>
{/if}

{#if error}
  <div class="settings-error" role="alert">{error}</div>
{/if}

<style>
  .config-section {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .config-section-title {
    padding: 0 0 8px;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    opacity: 0.7;
  }

  .config-separator {
    height: 1px;
    margin: 16px 0;
    background: var(--border);
  }

  .config-loading {
    padding: 14px 0;
    color: var(--muted-foreground);
    font-size: 12px;
    text-align: center;
    opacity: 0.6;
  }

  .config-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .config-row:last-child {
    border-bottom: none;
  }

  .config-row-label {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .label-text {
    color: var(--foreground);
    font-size: 13px;
    font-weight: 500;
  }

  .label-desc {
    max-width: 620px;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.5;
    opacity: 0.8;
  }

  .field-control {
    width: 220px;
    min-width: 0;
    flex-shrink: 0;
  }

  .field-control.narrow {
    width: 104px;
  }

  .settings-notice {
    margin-bottom: 16px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.45;
  }

  .settings-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }

  .settings-error {
    margin: 10px 0 0;
    padding: 8px 10px;
    border: 1px solid rgba(239, 68, 68, 0.22);
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.07);
    color: var(--destructive);
    font-size: 11.5px;
  }

  @media (max-width: 900px) {
    .config-row {
      align-items: stretch;
      flex-direction: column;
    }

    .field-control,
    .field-control.narrow {
      width: 100%;
    }
  }
</style>
