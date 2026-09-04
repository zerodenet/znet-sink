<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { getAppConfig, getAppErrorMessage, updateAppConfig } from '$lib/services/core';

  const DEFAULT_HOST = '127.0.0.1';
  const DEFAULT_PORT = 7890;

  let host = $state(DEFAULT_HOST);
  let port = $state(String(DEFAULT_PORT));
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  function resetDefault() {
    host = DEFAULT_HOST;
    port = String(DEFAULT_PORT);
    saved = false;
    error = null;
  }

  async function loadEndpoint() {
    loading = true;
    error = null;
    try {
      const config = await getAppConfig();
      if (config.localProxy.sourceProxyConfigId) {
        resetDefault();
      } else {
        host = config.localProxy.host || DEFAULT_HOST;
        port = String(config.localProxy.port || DEFAULT_PORT);
      }
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载代理端口配置失败');
    } finally {
      loading = false;
    }
  }

  async function saveEndpoint() {
    const normalizedHost = host.trim();
    const normalizedPort = Number.parseInt(port, 10);
    saved = false;
    error = null;

    if (!normalizedHost) {
      error = '监听地址不能为空';
      return;
    }
    if (!Number.isInteger(normalizedPort) || normalizedPort < 1 || normalizedPort > 65535) {
      error = '监听端口必须在 1 到 65535 之间';
      return;
    }

    saving = true;
    try {
      const config = await updateAppConfig({
        localProxy: {
          host: normalizedHost,
          port: normalizedPort,
          sourceProxyConfigId: null,
        },
      });
      host = config.localProxy.host;
      port = String(config.localProxy.port);
      saved = true;
    } catch (cause) {
      error = getAppErrorMessage(cause, '保存代理端口配置失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void loadEndpoint();
  });
</script>

<div class="config-section">
  <div class="config-section-title">代理端口</div>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">代理监听</span>
        <span class="label-desc">
          配置本地代理入口的监听地址和端口。当前用于自动补充 mixed 混合代理端口，默认使用 127.0.0.1:7890；订阅中完整定义的代理入站不会被覆盖。
        </span>
      </div>

      <div class="endpoint-editor">
        <div class="endpoint-fields">
          <Input
            class="w-[126px] font-mono"
            type="text"
            bind:value={host}
            oninput={() => (saved = false)}
            disabled={saving}
            spellcheck="false"
            aria-label="代理监听地址"
          />
          <span class="endpoint-colon">:</span>
          <Input
            class="w-[72px] font-mono"
            type="text"
            inputmode="numeric"
            bind:value={port}
            oninput={() => (saved = false)}
            disabled={saving}
            aria-label="代理监听端口"
          />
        </div>

        <div class="endpoint-actions">
          <Button variant="outline" size="sm" onclick={resetDefault} disabled={saving}>
            恢复默认
          </Button>
          <Button size="sm" onclick={saveEndpoint} disabled={saving}>
            {saving ? '保存中...' : saved ? '已保存' : '保存'}
          </Button>
        </div>
      </div>
    </div>
  {/if}

  {#if error}
    <div class="settings-error" role="alert">{error}</div>
  {/if}
</div>

<div class="config-separator"></div>

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
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.5;
    opacity: 0.8;
  }

  .endpoint-editor {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
  }

  .endpoint-fields,
  .endpoint-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .endpoint-colon {
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .settings-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin: 10px 0;
    padding: 8px 10px;
    border: 1px solid rgba(239, 68, 68, 0.22);
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.07);
    color: var(--destructive);
    font-size: 11.5px;
  }

  @media (max-width: 900px) {
    .config-row,
    .endpoint-editor {
      align-items: stretch;
      flex-direction: column;
    }

    .endpoint-editor {
      width: 100%;
    }

    .endpoint-actions {
      justify-content: flex-end;
    }
  }
</style>
