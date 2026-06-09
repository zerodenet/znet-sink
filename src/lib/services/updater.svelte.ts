import { check } from '@tauri-apps/plugin-updater';
import { getVersion } from '@tauri-apps/api/app';
import { info, warning } from '$lib/services/toast.svelte';
import { appendLog } from '$lib/services/core';

export type UpdaterStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'downloading' | 'error' | 'unsupported';

class UpdaterService {
  updateAvailable = $state(false);
  latestVersion = $state<string | null>(null);
  currentVersion = $state<string>('');
  releaseNotes = $state<string | null>(null);
  checking = $state(false);
  downloading = $state(false);
  lastError = $state<string | null>(null);
  /** Granular status for UI rendering. */
  status = $state<UpdaterStatus>('idle');

  constructor() {
    // Resolve actual app version from Tauri (falls back to "unknown" in browser / dev).
    this.initVersion();
  }

  private async initVersion() {
    try {
      this.currentVersion = await getVersion();
    } catch {
      this.currentVersion = 'dev';
    }
  }

  /** Check for updates — call on startup and manually from About panel. */
  async checkForUpdate(): Promise<boolean> {
    if (this.checking) return false;
    this.checking = true;
    this.lastError = null;
    this.status = 'checking';

    // Ensure currentVersion is resolved before we log.
    if (!this.currentVersion || this.currentVersion === '') {
      await this.initVersion();
    }

    try {
      void appendLog({ source: 'app', level: 'info', message: `正在检查更新… (当前 v${this.currentVersion})` });
      const update = await check();
      if (update) {
        this.updateAvailable = true;
        this.latestVersion = update.version;
        this.currentVersion = update.currentVersion;
        this.releaseNotes = update.body ?? null;
        this.status = 'available';
        void appendLog({ source: 'app', level: 'info', message: `发现新版本 v${update.version}（当前 v${update.currentVersion}）` });
        return true;
      } else {
        this.updateAvailable = false;
        this.latestVersion = null;
        // Distinguish "no update needed" from "endpoint missing / dev mode".
        // check() returns null both when up-to-date AND when the updater
        // cannot reach the endpoint in some environments.  Log the
        // current version so the user can tell which case it is.
        this.status = 'up-to-date';
        void appendLog({ source: 'app', level: 'info', message: `已是最新版本 v${this.currentVersion}` });
        return false;
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      this.lastError = msg;
      this.status = 'error';
      void appendLog({ source: 'app', level: 'warn', message: `更新检查失败: ${msg}` });
      return false;
    } finally {
      this.checking = false;
    }
  }

  /** Download and install the update. */
  async downloadAndInstall(): Promise<boolean> {
    if (this.downloading) return false;
    this.downloading = true;
    this.status = 'downloading';
    try {
      const update = await check();
      if (!update) {
        this.downloading = false;
        this.status = 'up-to-date';
        return false;
      }

      let downloaded = 0;
      let total: number | undefined;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            total = event.data.contentLength ?? undefined;
            info('开始下载更新…');
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            break;
          case 'Finished':
            info('下载完成，应用即将重启…');
            break;
        }
      });

      // The app will restart after install
      this.downloading = false;
      this.status = 'up-to-date';
      return true;
    } catch (e) {
      warning(`更新失败: ${e instanceof Error ? e.message : String(e)}`);
      this.downloading = false;
      this.status = 'error';
      return false;
    }
  }

  /** Manually dismiss the update notification. */
  dismissUpdate() {
    this.updateAvailable = false;
    this.latestVersion = null;
    this.releaseNotes = null;
    this.status = 'up-to-date';
  }
}

export const updater = new UpdaterService();
