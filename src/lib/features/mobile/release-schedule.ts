import { validReleaseCatalog } from './release-browser';
import type { MobileReleaseGateway, ReleaseScheduleSnapshot } from '$lib/types/mobile-release';
export interface ReleaseScheduleState { phase: 'idle' | 'reading' | 'ready' | 'error'; snapshot: ReleaseScheduleSnapshot | null; error: string | null }
export function releaseTime(raw: string | null) {
  if (!raw || raw.length > 20 || !/^(0|[1-9]\d*)$/.test(raw)) return '—';
  const value = Number(raw);
  return Number.isSafeInteger(value) && value > 0 && value <= 8640000000000000 ? new Date(value).toLocaleString() : '—';
}
const u64 = (v: unknown): v is string => typeof v === 'string' && v.length <= 20 && /^(0|[1-9]\d*)$/.test(v) && BigInt(v) <= 18446744073709551615n;
export function releaseOutcome(outcome: string) {
  const labels: Record<string, string> = { never: '尚未执行', checking: '正在检查', success: '检查成功', failed: '检查失败', invalidated: '原检查身份已失效', interrupted: '上次执行已中断', 'preferences-unconfirmed': '偏好保存尚未确认', 'storage-error': '调度记录存储异常' };
  return labels[outcome] ?? '执行状态未确认';
}
function valid(s: ReleaseScheduleSnapshot) {
  return !!s && typeof s.hostSessionId === 'string' && s.hostSessionId.length > 0 && s.hostSessionId.length <= 128 && u64(s.revision) && ['stable', 'preview', 'test'].includes(s.clientChannel)
    && [s.foreground, s.checking, s.enabled, s.storageError, s.cacheCurrent].every(v => typeof v === 'boolean')
    && [s.lastAttemptedAtUnixMs, s.lastSuccessAtUnixMs, s.nextDueAtUnixMs, s.retryAtUnixMs].every(v => v === null || u64(v))
    && ['native-lifecycle', 'not-native-host'].includes(s.foregroundScheduling) && s.backgroundScheduling === 'not-adapted'
    && typeof s.outcome === 'string' && (s.reason === null || typeof s.reason === 'string')
    && (!s.cacheCurrent || !!s.cachedCatalog)
    && (s.cachedCatalog === null || (validReleaseCatalog(s.cachedCatalog) && s.cachedCatalog.source === 'client' && s.cachedCatalog.channel === s.clientChannel));
}
/** Host-owned scheduling; this observer never starts a check or owns a timer. */
export class MobileReleaseSchedule {
  state: ReleaseScheduleState = { phase: 'idle', snapshot: null, error: null };
  private disposed = false;
  private reading = false;
  private pending = false;
  private generation = 0;
  constructor(private gateway: Pick<MobileReleaseGateway, 'schedule'>, private notify: (value: ReleaseScheduleState) => void) {}
  private publish(value: ReleaseScheduleState) { if (!this.disposed) { this.state = value; this.notify(value); } }
  async read() {
    if (this.disposed) return;
    ++this.generation;
    if (this.reading) { this.pending = true; return; }
    this.reading = true;
    do {
      this.pending = false;
      const generation = this.generation;
      this.publish({ ...this.state, phase: 'reading', error: null });
      try {
        const snapshot = await this.gateway.schedule();
        if (this.disposed) return;
        if (generation !== this.generation) continue;
        if (!valid(snapshot)) throw new Error('invalid schedule');
        const before = this.state.snapshot;
        if (before && before.hostSessionId === snapshot.hostSessionId && BigInt(snapshot.revision) < BigInt(before.revision)) throw new Error('stale schedule');
        this.publish({ phase: 'ready', snapshot, error: null });
      } catch {
        if (this.disposed) return;
        if (generation !== this.generation) continue;
        this.publish({ ...this.state, phase: 'error', error: '前台检查状态读取失败；旧结果仅供历史查看，未触发目录检查。' });
      }
    } while (this.pending && !this.disposed);
    this.reading = false;
  }
  invalidate() { this.generation++; }
  destroy() { this.disposed = true; this.generation++; this.pending = false; }
}
