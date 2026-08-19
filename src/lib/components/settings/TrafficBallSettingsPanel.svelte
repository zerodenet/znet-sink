<script lang="ts">
  import { Switch } from '$lib/components/ui/switch';
  import { trafficBallPreference } from '$lib/services/traffic-ball-preference.svelte';

  let saveError = $state<string | null>(null);

  async function toggleTrafficBall(enabled: boolean) {
    saveError = null;
    try {
      await trafficBallPreference.setEnabled(enabled);
    } catch (error) {
      saveError = `更新流量悬浮球设置失败：${error instanceof Error ? error.message : String(error)}`;
    }
  }
</script>

<div class="config-section">
  <div class="config-section-title">窗口行为</div>

  <div class="config-row">
    <div class="config-row-label">
      <span class="label-text">流量悬浮球</span>
      <span class="label-desc">开启后，最小化或关闭主窗口时显示实时流量悬浮球；关闭后恢复普通窗口行为。</span>
    </div>
    <Switch
      checked={trafficBallPreference.enabled}
      onCheckedChange={(checked) => void toggleTrafficBall(checked)}
      disabled={trafficBallPreference.loading || trafficBallPreference.saving}
      aria-label="流量悬浮球"
    />
  </div>

  {#if saveError || trafficBallPreference.error}
    <div class="settings-error" role="alert">{saveError ?? `加载流量悬浮球设置失败：${trafficBallPreference.error}`}</div>
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
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    padding: 0 0 8px;
    opacity: 0.7;
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
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .label-text {
    font-size: 13px;
    font-weight: 500;
    color: var(--foreground);
  }

  .label-desc {
    max-width: 520px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--muted-foreground);
    opacity: 0.8;
  }

  .config-separator {
    height: 1px;
    background: var(--border);
    margin: 16px 0;
  }

  .settings-error {
    margin-top: 8px;
    padding: 8px 10px;
    border: 1px solid rgba(239, 68, 68, 0.22);
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.07);
    color: var(--destructive);
    font-size: 11.5px;
  }
</style>
