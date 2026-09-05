import { check, Update, type DownloadEvent } from '@tauri-apps/plugin-updater';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';
import { info, warning } from '$lib/services/toast.svelte';
import { appendLog } from '$lib/services/core';
import { tracedOperation } from '$lib/services/telemetry';
import {
  shouldShowProminentUpdate,
  type AppRelease,
} from '$lib/services/app-update-policy';
import { RELEASE_CHECK_INTERVAL_MS } from '$lib/services/release-check-policy';

export type UpdaterStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'downloading' | 'ready-to-install' | 'error' | 'unsupported' | 'restart-required';

export const UPDATE_CHECK_INTERVAL_MS = RELEASE_CHECK_INTERVAL_MS;
const INITIAL_UPDATE_CHECK_DELAY_MS = 3000;

interface AppUpdateMetadata {
  rid: number;
  currentVersion: string;
  version: string;
  date?: string;
  body?: string;
  rawJson: Record<string, unknown>;
}

class UpdaterService {
  updateAvailable = $state(false);
  latestVersion = $state<string | null>(null);
  currentVersion = $state<string>('');
  releaseNotes = $state<string | null>(null);
  checking = $state(false);
  downloading = $state(false);
  installing = $state(false);
  restartRequired = $state(false);

  get busy(): boolean {
    return this.checking || this.downloading || this.installing;
  }
  readyToInstall = $state(false);
  lastError = $state<string | null>(null);
  /** Bytes downloaded so far in the current `downloadAndInstall` run. */
  downloaded = $state(0);
  /** Total bytes to download, or null when the server omitted Content-Length. */
  total = $state<number | null>(null);
  /** Granular status for UI rendering. */
  status = $state<UpdaterStatus>('idle');
  selectedTag = $state<string | null>(null);
  private lastCheckAt = 0;
  private pendingUpdate: Update | null = null;
  private initialCheckTimer: ReturnType<typeof setTimeout> | null = null;
  private periodicCheckTimer: ReturnType<typeof setInterval> | null = null;
  private scheduleListenersAttached = false;

  /** Download progress as 0–100, or null when total size is unknown (indeterminate). */
  get progressPct(): number | null {
    if (this.total == null || this.total <= 0) return null;
    return Math.min(100, Math.round((this.downloaded / this.total) * 100));
  }

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
    if (this.busy || this.readyToInstall || this.restartRequired) return false;
    this.checking = true;
    this.lastError = null;
    this.status = 'checking';

    // Ensure currentVersion is resolved before we log.
    if (!this.currentVersion || this.currentVersion === '') {
      await this.initVersion();
    }

    try {
      void appendLog({ source: 'app', level: 'info', message: `正在检查更新… (当前 v${this.currentVersion})` });
      const update = await tracedOperation('update', 'update.check', () => check(), {
        currentVersion: this.currentVersion,
      });
      if (update) {
        this.replacePendingUpdate(update);
        this.updateAvailable = true;
        this.latestVersion = update.version;
        this.currentVersion = update.currentVersion;
        this.releaseNotes = update.body ?? null;
        this.selectedTag = null;
        this.status = 'available';
        void appendLog({ source: 'app', level: 'info', message: `发现新版本 v${update.version}（当前 v${update.currentVersion}）` });
        return true;
      } else {
        this.replacePendingUpdate(null);
        this.updateAvailable = false;
        this.latestVersion = null;
        this.selectedTag = null;
        this.status = 'up-to-date';
        void appendLog({ source: 'app', level: 'info', message: `已是最新版本 v${this.currentVersion}` });
        return false;
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);

      this.lastError = isManifestUnavailable(msg)
        ? `更新清单不可用，无法确认是否有新版本：${msg}`
        : msg;
      this.status = 'error';
      void appendLog({ source: 'app', level: 'warn', message: `更新检查失败: ${msg}` });
      return false;
    } finally {
      this.lastCheckAt = Date.now();
      this.checking = false;
    }
  }

  /** Only stable-to-stable updates use the global banner and title-bar dot. */
  get prominentUpdateAvailable(): boolean {
    return this.updateAvailable
      && shouldShowProminentUpdate(this.currentVersion, this.latestVersion);
  }

  startPeriodicChecks() {
    if (this.periodicCheckTimer) return;

    this.initialCheckTimer = setTimeout(() => {
      this.initialCheckTimer = null;
      this.runScheduledCheck();
    }, INITIAL_UPDATE_CHECK_DELAY_MS);
    this.periodicCheckTimer = setInterval(() => {
      this.runScheduledCheck();
    }, UPDATE_CHECK_INTERVAL_MS);

    if (!this.scheduleListenersAttached) {
      document.addEventListener('visibilitychange', this.handleVisibilityChange);
      window.addEventListener('online', this.handleOnline);
      this.scheduleListenersAttached = true;
    }
  }

  stopPeriodicChecks() {
    if (this.initialCheckTimer) clearTimeout(this.initialCheckTimer);
    if (this.periodicCheckTimer) clearInterval(this.periodicCheckTimer);
    this.initialCheckTimer = null;
    this.periodicCheckTimer = null;

    if (this.scheduleListenersAttached) {
      document.removeEventListener('visibilitychange', this.handleVisibilityChange);
      window.removeEventListener('online', this.handleOnline);
      this.scheduleListenersAttached = false;
    }
  }

  private runScheduledCheck(force = false) {
    if (this.busy || this.updateAvailable || this.readyToInstall || this.restartRequired) return;
    if (!force && this.lastCheckAt > 0 && Date.now() - this.lastCheckAt < UPDATE_CHECK_INTERVAL_MS) return;
    void this.checkForUpdate();
  }

  private handleVisibilityChange = () => {
    if (document.visibilityState === 'visible') this.runScheduledCheck();
  };

  private handleOnline = () => {
    this.runScheduledCheck(this.status === 'error');
  };

  /** Prepare a specific GitHub release, including prereleases and rollbacks. */
  async selectRelease(release: AppRelease): Promise<boolean> {
    if (this.busy || this.restartRequired) return false;
    this.checking = true;
    this.lastError = null;
    this.status = 'checking';
    this.replacePendingUpdate(null);
    this.updateAvailable = false;
    this.latestVersion = null;
    this.releaseNotes = null;
    this.selectedTag = null;

    if (!this.currentVersion) await this.initVersion();

    try {
      const metadata = await tracedOperation(
        'update',
        'update.release.select',
        () => invoke<AppUpdateMetadata | null>('app_check_release', { tagName: release.tagName }),
        { currentVersion: this.currentVersion, selectedVersion: release.version },
      );
      if (!metadata) {
        this.status = 'up-to-date';
        return false;
      }

      const update = new Update(metadata);
      this.replacePendingUpdate(update);
      this.updateAvailable = true;
      this.latestVersion = update.version;
      this.currentVersion = update.currentVersion;
      this.releaseNotes = update.body ?? release.notes;
      this.selectedTag = release.tagName;
      this.status = 'available';
      void appendLog({
        source: 'app',
        level: 'info',
        message: `已选择应用版本 v${update.version}（当前 v${update.currentVersion}）`,
      });
      return true;
    } catch (error) {
      const message = updaterErrorMessage(error, '选择应用版本失败');
      const userMessage = isManifestUnavailable(message)
        ? `v${release.version} 暂无适用于当前系统的安装包；该版本可能仍在构建或发布不完整，请稍后刷新后重试。`
        : message;
      this.lastError = userMessage;
      this.status = 'error';
      void appendLog({ source: 'app', level: 'warn', message: `选择应用版本失败: ${userMessage}` });
      return false;
    } finally {
      this.checking = false;
      this.lastCheckAt = Date.now();
    }
  }

  /** Download and install the update. */
  async downloadAndInstall(): Promise<boolean> {
    if (this.busy || this.restartRequired) return false;
    this.downloading = true;
    this.status = 'downloading';
    this.downloaded = 0;
    this.total = null;
    try {
      const update = this.pendingUpdate ?? await tracedOperation(
        'update',
        'update.install.prepare',
        () => check(),
        { currentVersion: this.currentVersion },
      );
      if (!update) {
        this.downloading = false;
        this.status = 'up-to-date';
        return false;
      }

      this.replacePendingUpdate(update);
      await tracedOperation(
        'update',
        'update.download_install',
        () => update.downloadAndInstall((event) => {
          switch (event.event) {
            case 'Started':
              this.total = event.data.contentLength ?? null;
              info('开始下载更新…');
              break;
            case 'Progress':
              this.downloaded += event.data.chunkLength;
              break;
            case 'Finished':
              info('下载完成，应用即将重启…');
              break;
          }
        }),
        { fromVersion: this.currentVersion, toVersion: update.version },
      );

      this.restartRequired = true;
      this.readyToInstall = false;
      this.updateAvailable = false;
      this.status = 'restart-required';
      await tracedOperation(
        'update',
        'update.relaunch',
        () => relaunch(),
        { fromVersion: this.currentVersion, toVersion: update.version },
      );

      this.downloading = false;
      this.status = 'restart-required';
      return true;
    } catch (e) {
      this.lastError = e instanceof Error ? e.message : String(e);
      warning(`更新失败: ${this.lastError}`);
      this.downloading = false;
      this.status = this.restartRequired ? 'restart-required' : 'error';
      return false;
    }
  }

  /** Manually dismiss the update notification. */
  dismissUpdate() {
    if (this.busy || this.restartRequired) return;
    this.replacePendingUpdate(null);
    this.updateAvailable = false;
    this.latestVersion = null;
    this.releaseNotes = null;
    this.selectedTag = null;
    this.status = 'idle';
    this.lastCheckAt = Date.now();
  }

  /** Download a selected release without installing it. */
  async downloadUpdate(): Promise<boolean> {
    if (this.busy || this.restartRequired || this.readyToInstall || !this.pendingUpdate) return false;
    this.downloading = true;
    this.readyToInstall = false;
    this.lastError = null;
    this.status = 'downloading';
    this.downloaded = 0;
    this.total = null;

    try {
      const update = this.pendingUpdate;
      await tracedOperation(
        'update',
        'update.download',
        () => update.download((event) => this.applyDownloadEvent(event)),
        { fromVersion: this.currentVersion, toVersion: update.version },
      );
      this.readyToInstall = true;
      this.status = 'ready-to-install';
      info(`v${update.version} 下载完成，可随时安装`);
      return true;
    } catch (error) {
      this.lastError = updaterErrorMessage(error, '版本下载失败');
      warning(`版本下载失败: ${this.lastError}`);
      this.status = 'error';
      return false;
    } finally {
      this.downloading = false;
    }
  }

  /** Install a release previously downloaded with downloadUpdate(). */
  async installUpdate(): Promise<boolean> {
    if (this.busy || this.restartRequired || !this.pendingUpdate || !this.readyToInstall) return false;
    this.installing = true;
    this.lastError = null;
    try {
      const update = this.pendingUpdate;
      await tracedOperation(
        'update',
        'update.install',
        () => update.install(),
        { fromVersion: this.currentVersion, toVersion: update.version },
      );
      this.restartRequired = true;
      this.readyToInstall = false;
      this.updateAvailable = false;
      this.status = 'restart-required';
      await tracedOperation(
        'update',
        'update.relaunch',
        () => relaunch(),
        { fromVersion: this.currentVersion, toVersion: update.version },
      );
      this.readyToInstall = false;
      this.status = 'restart-required';
      return true;
    } catch (error) {
      this.lastError = updaterErrorMessage(error, '版本安装失败');
      warning(`版本安装失败: ${this.lastError}`);
      this.status = this.restartRequired ? 'restart-required' : 'error';
      return false;
    } finally {
      this.installing = false;
    }
  }

  async restartApp(): Promise<boolean> {
    if (this.busy || !this.restartRequired) return false;
    this.installing = true;
    this.lastError = null;
    try {
      await tracedOperation('update', 'update.relaunch.retry', () => relaunch());
      return true;
    } catch (error) {
      this.lastError = updaterErrorMessage(error, '重启失败，请退出后重新打开应用');
      return false;
    } finally {
      this.installing = false;
    }
  }

  private replacePendingUpdate(update: Update | null) {
    const previous = this.pendingUpdate;
    this.pendingUpdate = update;
    this.readyToInstall = false;
    this.downloaded = 0;
    this.total = null;
    if (previous && previous !== update) void previous.close().catch(() => {});
  }

  private applyDownloadEvent(event: DownloadEvent) {
    switch (event.event) {
      case 'Started':
        this.total = event.data.contentLength ?? null;
        break;
      case 'Progress':
        this.downloaded += event.data.chunkLength;
        break;
      case 'Finished':
        break;
    }
  }
}

export const updater = new UpdaterService();

/** Format a byte count as a compact human-readable string (B / KB / MB / GB). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/**
 * Detect updater errors that mean the published manifest is unusable —
 * rather than transport/network failures. Neither proves the app is current.
 *
 * Two families fall under this:
 *  1. Malformed manifest — the updater plugin's serde fails with
 *     "missing field" / "deserialize" / "parse" when the published
 *     `latest.json` is `{"platforms":{}}` with no `version` field (happens
 *     when a release was built without TAURI_SIGNING_PRIVATE_KEY).
 *  2. Platform not found — the manifest is structurally valid but carries
 *     no entry for the host platform ("none of the fallback platforms were
 *     found in the response platforms object"), e.g. a partial release or
 *     a manifest where every build job fell back to the placeholder.
 */
function isManifestUnavailable(message: string): boolean {
  const lower = message.toLowerCase();
  return lower.includes('missing field')
    || lower.includes('invalid type')
    || lower.includes('expected')
    || lower.includes('deserialize')
    || lower.includes('json')
    || lower.includes('parse')
    || lower.includes('fallback platforms')
    || lower.includes('platforms object');
}

function updaterErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === 'string' && error.trim()) return error;
  if (error && typeof error === 'object') {
    const message = (error as { message?: unknown }).message;
    if (typeof message === 'string' && message.trim()) return message;
  }
  return fallback;
}
