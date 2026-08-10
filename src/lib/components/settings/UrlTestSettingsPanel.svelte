<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getAppConfig, getAppErrorMessage, updateAppConfig } from '$lib/services/core';

  const DEFAULT_TOLERANCE_MS = 50;

  let tolerance = $state(String(DEFAULT_TOLERANCE_MS));
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  function resetDefault() {
    tolerance = String(DEFAULT_TOLERANCE_MS);
    saved = false;
    error = null;
  }

  async function loadSetting() {
    loading = true;
    error = null;
    try {
      const config = await getAppConfig();
      tolerance = String(config.urlTest?.toleranceMs ?? DEFAULT_TOLERANCE_MS);
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载 URLTest 延迟容差失败');
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

    saving = true;
    try {
      const config = await updateAppConfig({
        urlTest: { toleranceMs: normalized },
      });
      tolerance = String(config.urlTest.toleranceMs);
      saved = true;
    } catch (cause) {
      error = getAppErrorMessage(cause, '保存 URLTest 延迟容差失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void loadSetting();
  });
</script>

<div class="config-section">
  <div class="config-section-title">自动选择</div>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">URLTest 延迟容差</span>
        <span class="label-desc">
          当前节点仍健康时，只有候选节点快超过该值才自动切换。0 ms 表示始终追求最低延迟；Zero 配置显式设置的 tolerance_ms 优先。
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
            class="tolerance-input"
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

  .tolerance-input {
    width: 96px;
    font-family: var(--font-mono);
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
