<script lang="ts">
  import { onMount } from 'svelte';
  import { getAppConfig, getAppErrorMessage, updateAppConfig } from '$lib/services/core';

  let host = $state('127.0.0.1');
  let port = $state('7890');
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  async function loadEndpoint() {
    loading = true;
    error = null;
    try {
      const config = await getAppConfig();
      host = config.localProxy.host;
      port = String(config.localProxy.port);
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载 Mixed 入站配置失败');
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
        localProxy: { host: normalizedHost, port: normalizedPort },
      });
      host = config.localProxy.host;
      port = String(config.localProxy.port);
      saved = true;
    } catch (cause) {
      error = getAppErrorMessage(cause, '保存 Mixed 入站配置失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void loadEndpoint();
  });
</script>

<div class="config-section">
  <div class="config-section-title">订阅入站</div>

  <div class="endpoint-panel">
    <div class="endpoint-copy">
      <span class="label-text">缺省 Mixed 监听</span>
      <span class="label-desc">
        当订阅没有可用的 mixed、HTTP 或 SOCKS5 入站，或历史托管入站仍使用旧端口时，GUI 会按此地址补充或覆盖。保存后重新同步订阅生效。
      </span>
    </div>

    {#if loading}
      <div class="endpoint-state">加载配置中...</div>
    {:else}
      <div class="endpoint-grid">
        <label class="endpoint-field">
          <span>监听地址</span>
          <input
            type="text"
            bind:value={host}
            disabled={saving}
            spellcheck="false"
            aria-label="Mixed 监听地址"
          />
        </label>
        <label class="endpoint-field port-field">
          <span>监听端口</span>
          <input
            type="number"
            min="1"
            max="65535"
            bind:value={port}
            disabled={saving}
            aria-label="Mixed 监听端口"
          />
        </label>
      </div>

      <div class="endpoint-actions">
        <button type="button" class="save-button" onclick={saveEndpoint} disabled={saving}>
          {saving ? '保存中...' : '保存'}
        </button>
      </div>
    {/if}

    {#if error}
      <div class="endpoint-message error" role="alert">{error}</div>
    {:else if saved}
      <div class="endpoint-message success">已保存；重新同步订阅后应用到托管入站。</div>
    {/if}
  </div>
</div>

<div class="config-separator"></div>

<style>
  .config-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .config-section-title {
    color: var(--foreground);
    font-size: 13px;
    font-weight: 650;
  }

  .config-separator {
    height: 1px;
    margin: 18px 0;
    background: var(--border);
    opacity: 0.65;
  }

  .endpoint-panel {
    display: grid;
    gap: 12px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: color-mix(in srgb, var(--muted) 45%, transparent);
  }

  .endpoint-copy {
    display: grid;
    gap: 4px;
  }

  .label-text {
    color: var(--foreground);
    font-size: 12.5px;
    font-weight: 600;
  }

  .label-desc,
  .endpoint-state {
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.6;
  }

  .endpoint-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 160px;
    gap: 10px;
  }

  .endpoint-field {
    display: grid;
    gap: 6px;
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .endpoint-field input {
    width: 100%;
    min-width: 0;
    height: 34px;
    padding: 0 10px;
    color: var(--foreground);
    background: var(--background);
    border: 1px solid var(--border);
    border-radius: 8px;
    outline: none;
  }

  .endpoint-field input:focus {
    border-color: var(--ring);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 18%, transparent);
  }

  .endpoint-actions {
    display: flex;
    justify-content: flex-end;
  }

  .save-button {
    height: 32px;
    padding: 0 14px;
    color: var(--primary-foreground);
    background: var(--primary);
    border: 0;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .save-button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .endpoint-message {
    font-size: 11.5px;
  }

  .endpoint-message.error {
    color: var(--destructive);
  }

  .endpoint-message.success {
    color: var(--primary);
  }

  @media (max-width: 640px) {
    .endpoint-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
