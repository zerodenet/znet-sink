import { closeReason, targetOf, physicalKey, pageIdentity, validatePacketPage, PACKET_PAGE_LIMIT, PACKET_CLOSE_BUDGET } from '$lib/features/packet/types';
import type { PacketGateway, PacketRoute, PacketSnapshot, PacketTarget, PacketPhysicalTarget, PacketPageQuery, PacketCloseRecord } from '$lib/features/packet/types';
export interface PacketView {
  snapshot: PacketSnapshot | null; loading: boolean; stale: boolean; busy: string | null;
  detail: PacketRoute | null; error: unknown; refreshError: unknown; message: string | null;
  unconfirmed: string[]; blockedTargets: PacketPhysicalTarget[]; failedOffset: number | null;
}
// Read epochs own page eligibility. Physical close barriers survive every epoch.
// Durable host records survive reconstruction; their metadata never grants a new close.
export class PacketSession {
  view: PacketView = { snapshot: null, loading: false, stale: true, busy: null, detail: null, error: null, refreshError: null, message: null, unconfirmed: [], blockedTargets: [], failedOffset: null };
  private epoch = 0;
  private disposed = false;
  private local = new Map<string, { target: PacketPhysicalTarget }>();
  private records: PacketCloseRecord[] = [];
  private host: PacketPhysicalTarget[] = [];
  private gateway: PacketGateway;
  private changed: (view: PacketView) => void;
  constructor(gateway: PacketGateway, changed: (view: PacketView) => void) { this.gateway = gateway; this.changed = changed; }
  private publish(patch: Partial<PacketView>) {
    if (this.disposed) return;
    const targets = new Map(this.host.map(target => [physicalKey(target), target]));
    for (const [key, entry] of this.local) targets.set(key, entry.target);
    const blockedTargets = [...targets.values()];
    this.view = { ...this.view, ...patch, blockedTargets, unconfirmed: blockedTargets.map(target => target.routeId) };
    this.changed(this.view);
  }
  private current(epoch: number) { return !this.disposed && this.epoch === epoch; }
  invalidate() { this.epoch++; this.publish({ snapshot: null, detail: null, stale: true, loading: false, busy: null, error: null, refreshError: null, failedOffset: null, message: '配置或内核实例代次已变化，请重新检查；原未知关闭屏障保留' }); }
  dispose() { this.disposed = true; this.epoch++; }
  async refresh(clearError = false) { await this.read(this.view.snapshot?.offset ?? 0, clearError); }
  async retry() { await this.read(this.view.failedOffset ?? this.view.snapshot?.offset ?? 0, true); }
  async first() { await this.read(0, true, false); }
  async next() { const offset = this.view.snapshot?.nextOffset; if (offset !== null && offset !== undefined && !this.view.stale) await this.read(offset, true); }
  async previous() { const snapshot = this.view.snapshot; if (snapshot && snapshot.offset > 0 && !this.view.stale) await this.read(Math.max(0, snapshot.offset - snapshot.limit), true); }
  private async read(offset: number, clearError: boolean, pinned = true) {
    if (this.disposed || this.view.loading || this.view.busy || !Number.isInteger(offset) || offset < 0 || offset > 0xffffffff) return;
    const previous = this.view.snapshot;
    const query: PacketPageQuery = { offset, limit: previous?.limit ?? PACKET_PAGE_LIMIT, expected: pinned && previous ? pageIdentity(previous) : null };
    if (offset > 0 && !query.expected) return;
    const epoch = ++this.epoch;
    const moved = offset !== previous?.offset || !pinned;
    this.publish({ loading: true, stale: true, detail: moved ? null : this.view.detail, error: clearError ? null : this.view.error, refreshError: null });
    try {
      const snapshot = await this.gateway.snapshot(query);
      validatePacketPage(snapshot, query);
      if (!this.current(epoch)) return;
      this.host = snapshot.blockedTargets; this.records = snapshot.closeRecords;
      const selected = this.view.detail;
      const detail = selected ? snapshot.routes.find(route => physicalKey(route) === physicalKey(selected)) ?? null : null;
      this.publish({ snapshot, detail, stale: false, failedOffset: null });
    } catch (error) { if (this.current(epoch)) this.publish({ refreshError: error, stale: true, failedOffset: offset }); }
    finally { if (this.current(epoch)) this.publish({ loading: false }); }
  }
  target(route: PacketRoute): PacketTarget | null {
    const snapshot = this.view.snapshot;
    const current = snapshot?.routes.find(row => physicalKey(row) === physicalKey(route));
    return snapshot && !this.view.stale && current ? targetOf(snapshot, route) : null;
  }
  reason(route: PacketRoute): string | null {
    if (this.view.blockedTargets.some(target => physicalKey(target) === physicalKey(route))) return '原物理会话关闭仍在途或未确认，请只读核对；不会重放';
    if (this.view.snapshot?.closeLedgerRecovery.state === 'blocked') return this.view.snapshot.closeLedgerRecovery.reason ?? '关闭账本恢复未确认，请重新检查';
    if (this.view.blockedTargets.length >= PACKET_CLOSE_BUDGET || this.view.snapshot?.closeLedgerFull) return '128个关闭目标预算已满，请先只读核对原目标；仍可翻页';
    return closeReason(this.view.snapshot, route, this.view.stale);
  }
  async inspect(route: PacketRoute) {
    const target = this.target(route);
    if (target) await this.inspectTarget(this.withRecord(target));
  }
  private withRecord(target: PacketTarget): PacketTarget {
    const record = this.records.find(record => physicalKey(record.target) === physicalKey(target));
    return record ? { ...target, closeOperationId: record.operationId } : target;
  }
  async reconcile(physical: PacketPhysicalTarget) {
    const snapshot = this.view.snapshot;
    if (!snapshot || this.view.stale || snapshot.coreInstanceId !== physical.coreInstanceId) return;
    await this.inspectTarget(this.withRecord(targetOf(snapshot, physical)));
  }
  private async inspectTarget(target: PacketTarget) {
    if (this.view.busy || this.view.loading) return;
    const epoch = ++this.epoch, local = this.local.get(physicalKey(target));
    this.publish({ busy: target.routeId, detail: null, error: null });
    try {
      const detail = await this.gateway.detail(target);
      if (physicalKey(detail) !== physicalKey(target)) throw new Error('原会话核对响应身份不一致');
      if (target.closeOperationId && detail.closeOperationId !== target.closeOperationId) throw new Error('原关闭操作核对回执不匹配；屏障保留');
      if (detail.state === 'closed') this.release(target, detail.closeOperationId ?? null, local);
      if (this.current(epoch)) this.publish({ detail, message: detail.state === 'closed' ? '原物理会话已确认结束' : '已核对原会话；尚未确认结束时保留关闭屏障' });
    } catch (error) {
      const ended = error && typeof error === 'object' && 'code' in error && error.code === 'not_found' && 'details' in error ? (error.details as { packetTargetEnded?: PacketPhysicalTarget } | null)?.packetTargetEnded : null;
      const receiptId = error && typeof error === 'object' && 'details' in error ? (error.details as { closeOperationId?: string | null } | null)?.closeOperationId ?? null : null;
      if (ended && physicalKey(ended) === physicalKey(target) && (!target.closeOperationId || receiptId === target.closeOperationId)) {
        this.release(target, receiptId, local);
        if (this.current(epoch)) this.publish({ message: '同一内核实例已确认原物理会话不再存在', error: null });
      } else if (this.current(epoch)) this.publish({ error });
    } finally { if (this.current(epoch)) this.publish({ busy: null }); }
  }
  private release(target: PacketPhysicalTarget, operationId: string | null, local: { target: PacketPhysicalTarget } | undefined) {
    const key = physicalKey(target), record = this.records.find(record => physicalKey(record.target) === key);
    // A late original receipt may not remove a replacement record with the same physical key.
    if (record && record.operationId !== operationId) return;
    if (local && this.local.get(key) === local) this.local.delete(key);
    this.host = this.host.filter(row => physicalKey(row) !== key);
    this.records = this.records.filter(row => physicalKey(row.target) !== key);
    if (this.view.snapshot) this.view = { ...this.view, snapshot: { ...this.view.snapshot, closeRecords: this.records } };
  }
  async close(target: PacketTarget) {
    if (target.closeOperationId) { this.publish({ error: { message: '恢复记录仅供核对，不能作为新关闭请求' } }); return; }
    const route = this.view.snapshot?.routes.find(row => physicalKey(row) === physicalKey(target));
    if (this.view.busy || this.view.loading || !route) return;
    const current = this.target(route);
    if (!current || JSON.stringify(current) !== JSON.stringify(target) || this.reason(route)) { this.publish({ error: { message: '确认面板目标已失效、已有未知关闭或预算已满，请只读核对后重新选择' } }); return; }
    const epoch = ++this.epoch;
    const key = physicalKey(target);
    const local = { target }; this.local.set(key, local);
    this.publish({ busy: target.routeId, stale: true, error: null, message: null });
    try {
      const result = await this.gateway.close(target);
      // Physical receipts can resolve a barrier even after its page retired.
      if (result.status === 'confirmed') this.release(target, result.operationId ?? null, local);
      if (this.current(epoch)) this.publish({ message: result.message, detail: result.route });
    } catch (error) {
      const code = error && typeof error === 'object' && 'code' in error ? error.code : null;
      if (['permission_denied','conflict','invalid_argument','unsupported','feature_disabled'].includes(String(code)) && this.local.get(key) === local) this.local.delete(key);
      if (this.current(epoch)) this.publish({ error });
    } finally {
      if (this.current(epoch)) { this.publish({ busy: null }); await this.refresh(false); }
    }
  }
}
