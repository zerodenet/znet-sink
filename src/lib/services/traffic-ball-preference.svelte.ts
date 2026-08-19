import { browser } from '$app/environment';
import { getAppConfig, updateAppConfig } from './core';

class TrafficBallPreference {
  enabled = $state(true);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);

  private loadPromise: Promise<void> | null = null;

  load(): Promise<void> {
    if (this.loadPromise) return this.loadPromise;

    this.loadPromise = (async () => {
      this.loading = true;
      this.error = null;
      try {
        const config = await getAppConfig();
        this.enabled = config.ui.trafficBallEnabled ?? true;
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error);
      } finally {
        this.loading = false;
        this.loadPromise = null;
      }
    })();

    return this.loadPromise;
  }

  async setEnabled(enabled: boolean): Promise<void> {
    if (this.saving || enabled === this.enabled) return;

    const previous = this.enabled;
    this.enabled = enabled;
    this.saving = true;
    this.error = null;

    try {
      const updated = await updateAppConfig({
        ui: { trafficBallEnabled: enabled },
      });
      this.enabled = updated.ui.trafficBallEnabled ?? enabled;
    } catch (error) {
      this.enabled = previous;
      this.error = error instanceof Error ? error.message : String(error);
      throw error;
    } finally {
      this.saving = false;
    }
  }
}

export const trafficBallPreference = new TrafficBallPreference();

if (browser) {
  void trafficBallPreference.load();
}
