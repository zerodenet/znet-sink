import { closeReason, targetOf } from '$lib/features/packet/types';
import type { PacketGateway, PacketRoute, PacketSnapshot, PacketTarget } from '$lib/features/packet/types';
export interface PacketView {
  snapshot: PacketSnapshot | null; loading: boolean; stale: boolean; busy: string | null;
  detail: PacketRoute | null; error: unknown; refreshError: unknown; message: string | null; unconfirmed: string[];
}
// Page owns only its read subscriptions. Native execution continues off page.
export class PacketSession {
  view: PacketView = { snapshot: null, loading: false, stale: true, busy: null, detail: null, error: null, refreshError: null, message: null, unconfirmed: [] };
  private epoch = 0;
  private disposed = false;
  private gateway: PacketGateway;
  private changed: (view: PacketView) => void;
  constructor(gateway: PacketGateway, changed: (view: PacketView) => void) { this.gateway = gateway; this.changed = changed; }
  private publish(patch: Partial<PacketView>) { if (!this.disposed) { this.view = { ...this.view, ...patch }; this.changed(this.view); } }
  private current(epoch: number) { return !this.disposed && this.epoch === epoch; }
  invalidate() { this.epoch++; this.publish({ snapshot: null, detail: null, stale: true, loading: false, busy: null, error: null, refreshError: null, message: '配置或内核实例已变化，请重新检查', unconfirmed: [] }); }
  dispose() { this.disposed = true; this.epoch++; }
  async refresh(clearError = false) {
    if (this.disposed || this.view.loading || this.view.busy) return;
    const epoch = ++this.epoch;
    this.publish({ loading: true, stale: true, error: clearError ? null : this.view.error, refreshError: null });
    try {
      const snapshot = await this.gateway.snapshot();
      if (!this.current(epoch)) return;
      const previous = this.view.snapshot;
      const changed = previous && (previous.coreInstanceId !== snapshot.coreInstanceId || previous.configRevision !== snapshot.configRevision || previous.profileId !== snapshot.profileId);
      const selected = this.view.detail;
      const detail = !changed && selected ? snapshot.routes.find(r => r.routeId === selected.routeId && r.coreInstanceId === selected.coreInstanceId && r.configRevision === selected.configRevision) ?? null : null;
      this.publish({ snapshot, detail, stale: false, unconfirmed: changed ? [] : this.view.unconfirmed.filter(id => snapshot.routes.some(r => r.routeId === id && r.state !== 'closed')) });
    } catch (error) { if (this.current(epoch)) this.publish({ refreshError: error, stale: true }); }
    finally { if (this.current(epoch)) this.publish({ loading: false }); }
  }
  target(route: PacketRoute): PacketTarget | null {
    const snapshot = this.view.snapshot;
    const current = snapshot?.routes.find(r => r.routeId === route.routeId);
    if (!snapshot || this.view.stale || !current || current.coreInstanceId !== route.coreInstanceId || current.configRevision !== route.configRevision) return null;
    return targetOf(snapshot, route);
  }
  async inspect(route: PacketRoute) {
    const target = this.target(route);
    if (!target || this.view.busy) return;
    const epoch = ++this.epoch;
    this.publish({ busy: route.routeId, detail: null, error: null });
    try { const detail = await this.gateway.detail(target); if (this.current(epoch)) this.publish({ detail }); }
    catch (error) { if (this.current(epoch)) this.publish({ error }); }
    finally { if (this.current(epoch)) this.publish({ busy: null }); }
  }
  async close(target: PacketTarget) {
    const route = this.view.snapshot?.routes.find(r => r.routeId === target.routeId);
    if (this.view.busy || !route || this.view.unconfirmed.includes(target.routeId)) return;
    const current = this.target(route);
    if (!current || JSON.stringify(current) !== JSON.stringify(target) || closeReason(this.view.snapshot, route, this.view.stale)) {
      this.publish({ error: { message: '确认面板目标已失效，请刷新后重新选择' } }); return;
    }
    const epoch = ++this.epoch;
    this.publish({ busy: target.routeId, stale: true, error: null, message: null });
    try {
      const result = await this.gateway.close(target);
      if (!this.current(epoch)) return;
      this.publish({ message: result.message, detail: result.route, unconfirmed: result.status === 'unconfirmed' ? [...this.view.unconfirmed, target.routeId] : this.view.unconfirmed });
    } catch (error) { if (this.current(epoch)) this.publish({ error }); }
    finally { if (this.current(epoch)) { this.publish({ busy: null }); await this.refresh(false); } }
  }
}
