import type { AndroidUpdateGateway, AndroidUpdateSnapshot, AndroidReleaseAsset, MobileReleaseChannel } from '$lib/types/mobile-release';

const states = new Set(['idle', 'queued', 'downloading', 'paused', 'verifying', 'ready', 'preparing-install', 'awaiting-user', 'installing', 'installed-confirmed', 'cancelled', 'failed', 'unknown', 'store-managed']);
export function validAndroidUpdate(value: AndroidUpdateSnapshot): boolean {
  const decimal = (v: unknown) => typeof v === 'string' && /^(0|[1-9]\d{0,19})$/.test(v) && BigInt(v) <= 18446744073709551615n;
  return !!value && states.has(value.state) && typeof value.packageName === 'string' && value.packageName.length <= 256
    && typeof value.installedVersion === 'string' && value.installedVersion.length <= 256 && decimal(value.installedVersionCode)
    && decimal(value.downloadedBytes) && (value.sizeBytes === null || decimal(value.sizeBytes) && BigInt(value.sizeBytes) <= 536870912n)
    && (value.sizeBytes === null || BigInt(value.downloadedBytes) <= BigInt(value.sizeBytes))
    && (value.operationId === null || /^[0-9a-fA-F-]{36}$/.test(value.operationId))
    && (value.reason === null || /^[a-z0-9-]{1,128}$/.test(value.reason))
    && ['allowed', 'blocked', 'system-confirmation'].includes(value.installPermission) && typeof value.confirmationAvailable === 'boolean'
    && (value.targetVersionCode === null || decimal(value.targetVersionCode))
    && (value.targetVersion === null || typeof value.targetVersion === 'string' && value.targetVersion.length <= 256)
    && (value.assetName === null || typeof value.assetName === 'string' && value.assetName.length <= 180)
    && (value.sha256 === null || /^[0-9a-f]{64}$/.test(value.sha256))
    && (value.installerStatus === null || Number.isSafeInteger(value.installerStatus));
}
export interface AndroidUpdateView { snapshot: AndroidUpdateSnapshot | null; phase: 'reading' | 'ready' | 'sending' | 'unknown' | 'error'; error: string | null }
export class AndroidUpdateController {
  state: AndroidUpdateView = { snapshot: null, phase: 'reading', error: null };
  private generation = 0;
  private disposed = false;
  private gateway: AndroidUpdateGateway;
  private notify: (view: AndroidUpdateView) => void;
  constructor(gateway: AndroidUpdateGateway, notify: (view: AndroidUpdateView) => void) { this.gateway = gateway; this.notify = notify; }
  private publish(next: AndroidUpdateView) { if (!this.disposed) { this.state = next; this.notify(next); } }
  async read() {
    if (this.disposed || this.state.phase === 'sending') return;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'reading', error: null });
    try {
      const snapshot = await this.gateway.status();
      if (generation !== this.generation || this.disposed) return;
      if (!validAndroidUpdate(snapshot)) throw Error('invalid-receipt');
      this.publish({ snapshot, phase: 'ready', error: null });
    } catch {
      if (generation !== this.generation || this.disposed) return;
      this.publish({ ...this.state, phase: 'error', error: '安装状态读取失败；旧状态只供参考，请重新核对。' });
    }
  }
  async download(channel: MobileReleaseChannel, tag: string, asset: AndroidReleaseAsset, operationId: string) {
    await this.mutate(() => this.gateway.download(channel, tag, asset, operationId));
  }
  async action(action: 'install' | 'confirm' | 'cancel' | 'settings' | 'store', expectedOperationId?: string) {
    const id = this.state.snapshot?.operationId ?? null;
    if (expectedOperationId !== undefined && id !== expectedOperationId) {
      this.publish({ ...this.state, error: '更新目标已变化，请关闭面板并重新审阅。' }); return;
    }
    if (!['settings', 'store'].includes(action) && !id) return;
    await this.mutate(() => this.gateway.action(action, ['settings', 'store'].includes(action) ? null : id));
  }
  private async mutate(run: () => Promise<AndroidUpdateSnapshot>) {
    if (this.disposed || this.state.phase !== 'ready') return;
    const generation = ++this.generation;
    this.publish({ ...this.state, phase: 'sending', error: null });
    try {
      const snapshot = await run();
      if (generation !== this.generation || this.disposed) return;
      if (!validAndroidUpdate(snapshot)) throw Error('invalid-receipt');
      this.publish({ snapshot, phase: 'ready', error: null });
    } catch {
      if (generation !== this.generation || this.disposed) return;
      this.publish({ ...this.state, phase: 'unknown', error: '操作回执未确认。请读取原生状态；不会自动重试下载、提交或安装。' });
    }
  }
  destroy() { this.disposed = true; ++this.generation; }
}
export const androidUpdateLabel: Record<AndroidUpdateSnapshot['state'], string> = {
  idle: '尚未发起更新', queued: '等待系统调度下载', downloading: '正在下载', paused: '下载中断，等待系统重试', verifying: '正在检查 APK', ready: '检查通过，待确认安装', 'preparing-install': '正在准备系统安装会话', 'awaiting-user': '等待用户完成系统确认', installing: '系统安装中，尚未确认', 'installed-confirmed': '已核对安装版本与签名', cancelled: '已取消或系统终止', failed: '更新失败', unknown: '结果未确认，请核对', 'store-managed': '由 Google Play 管理更新',
};
export function androidUpdateReason(reason: string): string {
  const labels: Record<string, string> = {
    'package-mismatch': 'APK 包名与已安装应用不同（Debug 与正式包不能互相替换）',
    'version-not-newer': 'APK 版本代码没有高于已安装版本',
    'version-name-mismatch': 'APK 版本与所选发行不一致',
    'sdk-incompatible': 'APK 不支持当前 Android API',
    'abi-incompatible': 'APK 不包含此设备支持的 ABI',
    'signer-mismatch': 'APK 签名证书与已安装应用不一致，保留当前版本',
    'digest-mismatch': 'APK 的 SHA-256 与发行目录不一致',
    'unknown-source-permission-required': '系统尚未允许此应用请求安装',
    'system-user-confirmation-required': '等待用户在系统安装器确认',
    'system-user-cancelled-or-verification-aborted': '用户取消或系统验证终止了安装',
    'system-install-blocked': '系统策略或安装许可阻止了安装',
    'system-storage-unavailable': '系统报告存储不足或不可用',
    'storage-budget-unavailable': '私有存储空间不足，尚未下载',
    'download-network-interrupted': '下载网络中断；系统可在预算内重试',
    'installed-package-version-signer-confirmed': '已从系统核对目标包名、版本及当前签名',
    'installed-version-awaiting-confirmation': '系统报告安装成功，仍需核对实际安装版本',
    'session-missing-installed-version-unconfirmed': '安装会话已不可读，实际版本尚未确认',
    'install-preparation-interrupted': '安装准备被进程重建中断；请核对或取消原会话',
    'commit-result-unknown': '系统安装会话的提交结果未确认，不会自动重新提交',
    'system-confirmation-lost-recheck-or-cancel': '原系统确认票据已失效；请核对或取消原操作',
    'os-download-job-missing': '系统下载作业已不可读；请核对或取消原操作',
  };
  return labels[reason] ?? reason;
}
