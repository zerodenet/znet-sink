<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import MobileSettingsDraftGuard from '$lib/components/mobile/MobileSettingsDraftGuard.svelte';
  import { settingsDraftKey, captureSettingsDraftRead, canAcceptSettingsDraftRead } from '$lib/features/mobile/settings-draft';

  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getAppErrorMessage, getProfileSettings } from '$lib/services/core';
  import { isLocallyEdited, restoreProfileSettings, saveProfileSettings, type ProfileSettings } from '$lib/services/profile-settings';

  const DEFAULT_UDP_IDLE_SECONDS = 30;
  let snapshot = $state<ProfileSettings | null>(null);
  let udpIdleSeconds = $state(String(DEFAULT_UDP_IDLE_SECONDS));
  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state<string | null>(null);

  function formValue() { return { udpIdleSeconds: String(udpIdleSeconds) }; }
  const initialForm = formValue();
  let acceptedForm = $state.raw<ReturnType<typeof formValue> | null>(null);
  let readNotice = $state<string | null>(null);
  let alive = true, readGeneration = 0;
  const dirty = $derived(settingsDraftKey(formValue()) !== settingsDraftKey(acceptedForm ?? initialForm));
  function accept(next: ProfileSettings) {
    snapshot = next;
    udpIdleSeconds = String(next.settings.runtime?.udpUpstreamIdleTimeoutSeconds ?? DEFAULT_UDP_IDLE_SECONDS);
    acceptedForm = formValue(); readNotice = null;
  }
  function discardDraft() { udpIdleSeconds = (acceptedForm ?? initialForm).udpIdleSeconds; error = null; saved = false; readNotice = null; }
  onDestroy(() => { alive = false; readGeneration++; });

  async function load() {
    const generation = ++readGeneration;
    const read = captureSettingsDraftRead(acceptedForm ?? initialForm, formValue());
    loading = true; error = null;
    try {
      const next = await getProfileSettings();
      if (!alive || generation !== readGeneration) return;
      if (canAcceptSettingsDraftRead(read, acceptedForm ?? initialForm, formValue())) accept(next);
      else readNotice = '来源已刷新，未覆盖未保存草稿；请继续编辑或明确放弃。';
    } catch (cause) { if (alive && generation === readGeneration) error = getAppErrorMessage(cause, '加载连接超时设置失败'); }
    finally { if (alive && generation === readGeneration) loading = false; }
  }

  async function save() {
    if (saving || loading) return;
    const value = Number(udpIdleSeconds);
    saved = false;
    error = null;
    if (!Number.isSafeInteger(value) || value <= 0) {
      error = 'UDP 上游空闲时间必须是大于 0 的整数秒';
      return;
    }
    if (!snapshot) return;
    saving = true;
    try {
      const next = await saveProfileSettings(snapshot, { runtime: { udpUpstreamIdleTimeoutSeconds: value } });
      if (!alive) return;
      accept(next);
      saved = true;
    } catch (cause) {
      error = getAppErrorMessage(cause, '保存 UDP 空闲时间失败');
    } finally {
      saving = false;
    }
  }

  async function restore() {
    if (!snapshot || saving) return;
    saving = true;
    saved = false;
    error = null;
    try {
      const next = await restoreProfileSettings(snapshot, 'runtime.udpUpstreamIdleTimeoutSeconds');
      if (alive) accept(next);
    } catch (cause) {
      error = getAppErrorMessage(cause, '恢复 UDP 空闲时间失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => void load());
</script>
<MobileSettingsDraftGuard {dirty} busy={saving} label="运行网络设置" onDiscard={discardDraft} />
{#if readNotice}<p role="status">{readNotice}</p>{/if}

<div class="config-section">
  <div class="config-section-title">连接生命周期</div>
  <p class="source-note">{isLocallyEdited(snapshot, 'runtime.udpUpstreamIdleTimeoutSeconds') ? '客户端设置 · 全局生效' : '来自配置 · 缺失时使用 Zero 默认值 30 秒'}</p>
  {#if loading}
    <div class="config-loading">加载配置中...</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">UDP 上游空闲时间</span>
        <span class="label-desc">上游 UDP 会话在没有收发数据达到该时长后回收。值太小会让间歇通信频繁重建，值太大会让闲置会话占用更久。</span>
      </div>
      <div class="editor">
        <div class="field"><Input type="number" min="1" step="1" class="w-24 font-mono" aria-label="UDP 上游空闲时间" bind:value={udpIdleSeconds} oninput={() => (saved = false)} disabled={saving} /><span>秒</span></div>
        <div class="actions"><Button variant="outline" size="sm" onclick={restore} disabled={saving || !isLocallyEdited(snapshot, 'runtime.udpUpstreamIdleTimeoutSeconds')}>恢复配置值</Button><Button size="sm" onclick={save} disabled={saving}>{saving ? '保存中...' : saved ? snapshot?.applied === false ? '已保存，启动后生效' : '已应用' : '应用'}</Button></div>
      </div>
    </div>
  {/if}
  {#if error}<div class="settings-error" role="alert">{error}</div>{/if}
</div>

<style>
  .config-section { display: flex; flex-direction: column; gap: 2px; }
  .config-section-title { padding-bottom: 8px; color: var(--muted-foreground); font-size: 11px; font-weight: 700; letter-spacing: .07em; text-transform: uppercase; opacity: .7; }
  .source-note, .label-desc { color: var(--muted-foreground); font-size: 11.5px; line-height: 1.5; opacity: .8; }
  .source-note { margin: 0; }
  .config-loading { padding: 14px 0; color: var(--muted-foreground); font-size: 12px; text-align: center; opacity: .6; }
  .config-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 0; }
  .config-row-label { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 2px; }
  .label-text { color: var(--foreground); font-size: 13px; font-weight: 500; }
  .label-desc { max-width: 620px; }
  .editor, .field, .actions { display: flex; flex-shrink: 0; align-items: center; gap: 6px; }
  .field span { color: var(--muted-foreground); font-family: var(--font-mono); font-size: 11.5px; }
  .settings-error { margin: 10px 0; padding: 8px 10px; border: 1px solid rgba(239,68,68,.22); border-radius: 8px; background: rgba(239,68,68,.07); color: var(--destructive); font-size: 11.5px; }
  @media (max-width: 900px) { .config-row, .editor { align-items: stretch; flex-direction: column; } .editor { width: 100%; } .actions { justify-content: flex-end; } }
</style>
