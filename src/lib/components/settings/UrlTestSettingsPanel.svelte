<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import MobileSettingsDraftGuard from '$lib/components/mobile/MobileSettingsDraftGuard.svelte';
  import { settingsDraftKey, captureSettingsDraftRead, canAcceptSettingsDraftRead } from '$lib/features/mobile/settings-draft';

  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getProfileSettings, getAppErrorMessage } from '$lib/services/core';

  import {saveProfileSettings, restoreProfileSettings, isLocallyEdited, type ProfileSettings} from '$lib/services/profile-settings';
  let snapshot = $state<ProfileSettings | null>(null);

  const DEFAULT_URL = 'http://www.gstatic.com/generate_204';
  let url = $state(DEFAULT_URL);

  const DEFAULT_TOLERANCE_MS = 50;

  let tolerance = $state(String(DEFAULT_TOLERANCE_MS));
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  function formValue() { return { url, tolerance: String(tolerance) }; }
  const initialForm = formValue();
  let acceptedForm = $state.raw<ReturnType<typeof formValue> | null>(null);
  let readNotice = $state<string | null>(null);
  let alive = true, readGeneration = 0;
  const dirty = $derived(settingsDraftKey(formValue()) !== settingsDraftKey(acceptedForm ?? initialForm));
  function accept(next: ProfileSettings) {
    snapshot = next; url = next.settings.urlTest?.url || DEFAULT_URL;
    tolerance = String(next.settings.urlTest?.toleranceMs ?? DEFAULT_TOLERANCE_MS);
    acceptedForm = formValue(); readNotice = null;
  }
  function discardDraft() { const previous = acceptedForm ?? initialForm; url = previous.url; tolerance = previous.tolerance; error = null; saved = false; readNotice = null; }
  onDestroy(() => { alive = false; readGeneration++; });

  async function resetDefault() {
    if (!snapshot || saving) return;
    saving = true; error = null; saved = false;
    try { const next = await restoreProfileSettings(snapshot, 'urlTest'); if (alive) accept(next); }
    catch (cause) { error = getAppErrorMessage(cause, '恢复配置值失败'); }
    finally { saving = false; }
  }

  async function loadSetting() {
    const generation = ++readGeneration;
    const read = captureSettingsDraftRead(acceptedForm ?? initialForm, formValue());
    loading = true; error = null;
    try {
      const next = await getProfileSettings();
      if (!alive || generation !== readGeneration) return;
      if (canAcceptSettingsDraftRead(read, acceptedForm ?? initialForm, formValue())) accept(next);
      else readNotice = '来源已刷新，未覆盖未保存草稿；请继续编辑或明确放弃。';
    } catch (cause) { if (alive && generation === readGeneration) error = getAppErrorMessage(cause, '加载测速设置失败'); }
    finally { if (alive && generation === readGeneration) loading = false; }
  }

  async function saveSetting() {
    if (saving || loading) return;
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
      if (!snapshot) return;
      const next = await saveProfileSettings(snapshot, { urlTest: { url: normalizedUrl.toString(), toleranceMs: normalized } });
      if (!alive) return;
      accept(next);
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
<MobileSettingsDraftGuard {dirty} busy={saving} label="节点延迟测试设置" onDiscard={discardDraft} />
{#if readNotice}<p role="status">{readNotice}</p>{/if}

<div class="config-section">
  <div class="config-section-title">公共测速与自动选择</div>

  <p class="text-xs text-muted-foreground">{isLocallyEdited(snapshot, 'urlTest') ? '客户端设置 · 全局生效' : '来自配置 · 缺失项使用默认值'}</p>

  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">公共测速地址</span>
        <span class="label-desc">修改后用于所有配置的公共测速；策略组已有的专用地址继续保留。</span>
      </div>
      <Input class="w-full max-w-sm font-mono" bind:value={url} oninput={() => (saved = false)} disabled={saving} aria-label="公共测速地址" placeholder="http://www.gstatic.com/generate_204" />
    </div>
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">URLTest 延迟容差</span>
        {#if snapshot?.groupTolerances && new Set(snapshot.groupTolerances).size > 1}
          <span class="label-desc">策略组当前分别使用 {Array.from(new Set(snapshot.groupTolerances)).join(' / ')} ms。输入框显示缺省补充值；修改此项后才统一设置。</span>
        {/if}
        <span class="label-desc">
          当前节点仍健康时，只有候选节点快超过该值才自动切换。0 ms 表示始终追求最低延迟；修改此项后统一所有配置的策略组容差，恢复后各组采用配置值。支持统一正式版 Zero v0.0.1；旧编号内核需要 v0.0.16-dev.3 或更高版本，不支持的内核不会注入该字段。
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
          <Button variant="outline" size="sm" onclick={resetDefault} disabled={saving || !isLocallyEdited(snapshot, 'urlTest')}>
            恢复配置值
          </Button>
          <Button size="sm" onclick={saveSetting} disabled={saving}>
            {saving ? '保存中...' : saved ? snapshot?.applied === false ? '已保存，启动后生效' : '已应用' : '应用'}
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
