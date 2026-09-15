<script lang="ts">
  import { onMount } from 'svelte';
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

  async function load() {
    loading = true;
    error = null;
    try {
      snapshot = await getProfileSettings();
      udpIdleSeconds = String(snapshot.settings.runtime?.udpUpstreamIdleTimeoutSeconds ?? DEFAULT_UDP_IDLE_SECONDS);
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载连接超时设置失败');
    } finally {
      loading = false;
    }
  }

  async function save() {
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
      snapshot = await saveProfileSettings(snapshot, {
        runtime: { udpUpstreamIdleTimeoutSeconds: value },
      });
      udpIdleSeconds = String(snapshot.settings.runtime.udpUpstreamIdleTimeoutSeconds);
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
      snapshot = await restoreProfileSettings(snapshot, 'runtime.udpUpstreamIdleTimeoutSeconds');
      await load();
    } catch (cause) {
      error = getAppErrorMessage(cause, '恢复 UDP 空闲时间失败');
    } finally {
      saving = false;
    }
  }

  onMount(() => void load());
</script>

<div class="config-section">
  <div class="config-section-title">连接生命周期</div>
  <p class="source-note">{isLocallyEdited(snapshot, 'runtime.udpUpstreamIdleTimeoutSeconds') ? '本地修改 · 仅当前配置' : '来自配置 · 缺失时使用 Zero 默认值 30 秒'}</p>
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
