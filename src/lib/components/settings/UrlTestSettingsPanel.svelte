<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getAppConfig, getAppErrorMessage, updateAppConfig } from '$lib/services/core';

  const DEFAULT_URL = 'http://www.gstatic.com/generate_204';
  let url = $state(DEFAULT_URL);

  const DEFAULT_TOLERANCE_MS = 50;

  let tolerance = $state(String(DEFAULT_TOLERANCE_MS));
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  function resetDefault() {
    url = DEFAULT_URL;
    tolerance = String(DEFAULT_TOLERANCE_MS);
    saved = false;
    error = null;
  }

  async function loadSetting() {
    loading = true;
    error = null;
    try {
      const config = await getAppConfig();
      url = config.urlTest?.url || DEFAULT_URL;
      tolerance = String(config.urlTest?.toleranceMs ?? DEFAULT_TOLERANCE_MS);
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载测速设置失败');
    } finally {
      loading = false;
    }
  }

  async function saveSetting() {
    const normalized = Number(tolerance);
    saved = false;
    error = null;

    if (!Number.isSafeInteger(normalized) || normalized < 0) {
      error = '延迟容差必须是大于或等于 0 的整数毫秒值';
      return;
    }

    let normalizedUrl: URL;
    try {
      normalizedUrl = new URL(url.trim());
      if (!['http:', 'https:'].includes(normalizedUrl.protocol) || !normalizedUrl.hostname || normalizedUrl.username || normalizedUrl.password || normalizedUrl.hash) throw new Error();
    } catch {
      error = '公共测速地址必须是完整的 HTTP(S) URL，不能包含账号密码或片段';
      return;
    }
    saving = true;
    try {
      const config = await updateAppConfig({
        urlTest: { url: normalizedUrl.toString(), toleranceMs: normalized },
      });
      url = config.urlTest.url;
      tolerance = String(config.urlTest.toleranceMs);
      saved = true;
    } catch (cause) {
      error = getAppErrorMessage(cause, '保存测速设置失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void loadSetting();
  });
</script>

<div class="config-section">
  <div class="config-section-title">公共测速与自动选择</div>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">公共测速地址</span>
        <span class="label-desc">手动节点测速和自动 URLTest 统一使用此地址，覆盖订阅／配置文件的测速地址。保存后应用到当前内核，后续测速使用新值。</span>
      </div>
      <Input class="w-full max-w-sm font-mono" bind:value={url} oninput={() => (saved = false)} disabled={saving} aria-label="公共测速地址" placeholder="http://www.gstatic.com/generate_204" />
    </div>
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">URLTest 延迟容差</span>
        <span class="label-desc">
          当前节点仍健康时，只有候选节点快超过该值才自动切换。0 ms 表示始终追求最低延迟；客户端设置覆盖配置文件中的 tolerance_ms。支持统一正式版 Zero v0.0.1；旧编号内核需要 v0.0.16-dev.3 或更高版本，不支持的内核不会注入该字段。
        </span>
      </div>

      <div class="tolerance-editor">
        <div class="tolerance-field">
          <Input
            type="number"
            min="0"
            step="1"
            bind:value={tolerance}
            oninput={() => (saved = false)}
            disabled={saving}
            class="w-24 font-mono"
            aria-label="URLTest 延迟容差"
          />
          <span class="unit">ms</span>
        </div>
        <div class="actions">
          <Button variant="outline" size="sm" onclick={resetDefault} disabled={saving}>
            恢复默认
          </Button>
          <Button size="sm" onclick={saveSetting} disabled={saving}>
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
    max-width: 620px;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.5;
    opacity: 0.8;
  }

  .tolerance-editor,
  .tolerance-field,
  .actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 6px;
  }

  .unit {
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 11.5px;
  }

  .settings-error {
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
    .tolerance-editor {
      align-items: stretch;
      flex-direction: column;
    }

    .tolerance-editor {
      width: 100%;
    }

    .actions {
      justify-content: flex-end;
    }
  }
</style>
