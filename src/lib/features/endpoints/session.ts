import { operationReason } from '$lib/features/endpoints/policy';
import type { EndpointAction, EndpointCatalog, EndpointDetails, EndpointGateway, NetworkEndpoint } from './types';
import { sampleEndpointTraffic, type EndpointTraffic } from '$lib/features/endpoints/traffic';

export interface EndpointView {
  catalog: EndpointCatalog | null;
  loading: boolean;
  busy: string | null;
  stale: boolean;
  error: unknown;
  refreshError: unknown;
  result: string | null;
  details: EndpointDetails | null;
  detailsLoading: boolean;
  detailsError: unknown;
  traffic: Record<string, EndpointTraffic>;
}
// Discard publications from an old scope. A submitted kernel transaction still
// finishes at the host; cancellation here never claims to undo that operation.
export class EndpointSession {
  view: EndpointView = {
    catalog: null, loading: false, busy: null, stale: true, error: null,
    refreshError: null, result: null, details: null, detailsLoading: false, detailsError: null, traffic: {},
  };
  private epoch = 0;
  private disposed = false;
  private selectedDetailId: string | null = null;
  private gateway: EndpointGateway;
  private changed: (view: EndpointView) => void;
  constructor(gateway: EndpointGateway, changed: (view: EndpointView) => void) {
    this.gateway = gateway;
    this.changed = changed;
  }
  private publish(patch: Partial<EndpointView>) {
    if (this.disposed) return;
    this.view = { ...this.view, ...patch };
    this.changed(this.view);
  }
  private current(epoch: number) { return !this.disposed && epoch === this.epoch; }
  invalidate() {
    this.epoch++;
    this.selectedDetailId = null;
    this.publish({ catalog: null, details: null, detailsLoading: false, detailsError: null,
      stale: true, loading: false, error: null, refreshError: null, result: null, traffic: {} });
  }
  dispose() { this.disposed = true; this.epoch++; }
  async refresh(clearError = true) {
    if (this.disposed || this.view.busy || this.view.loading || this.view.detailsLoading) return;
    const epoch = ++this.epoch;
    this.publish({ loading: true });
    try {
      const catalog = await this.gateway.catalog();
      if (!this.current(epoch)) return;
      const old = this.view.catalog?.endpoints[0];
      const first = catalog.endpoints[0];
      const changedScope = old && first && (old.core_instance_id !== first.core_instance_id || old.config_revision !== first.config_revision);
      if (changedScope) this.selectedDetailId = null;
      const traffic = Object.fromEntries(catalog.endpoints.map(endpoint => [endpoint.endpoint_id,
        sampleEndpointTraffic(this.view.traffic[endpoint.endpoint_id], endpoint)]));
      this.publish({ catalog, stale: false, error: clearError ? null : this.view.error,
        refreshError: null, result: changedScope ? null : this.view.result,
        details: null, detailsError: null, traffic });
      const selected = catalog.endpoints.find(row => row.endpoint_id === this.selectedDetailId);
      if (selected?.supported.operations.includes('details')) await this.readDetails(selected, epoch);
      else this.selectedDetailId = null;
    } catch (refreshError) {
      if (this.current(epoch)) this.publish({ stale: true, refreshError, details: null });
    } finally {
      if (this.current(epoch)) this.publish({ loading: false });
    }
  }
  private isCurrentEndpoint(endpoint: NetworkEndpoint): boolean {
    const current = this.view.catalog?.endpoints.find(row => row.endpoint_id === endpoint.endpoint_id);
    return !!current && current.core_instance_id === endpoint.core_instance_id
      && current.config_revision === endpoint.config_revision && current.intent_revision === endpoint.intent_revision
      && current.generation === endpoint.generation;
  }
  async act(endpoint: NetworkEndpoint, action: EndpointAction) {
    if (this.disposed || this.view.busy || this.view.stale) return;
    const reason = this.isCurrentEndpoint(endpoint)
      ? operationReason(this.view.catalog, endpoint, action) : '端点观测已变化，请刷新后重试';
    if (reason) { this.publish({ error: { message: reason } }); return; }
    const epoch = ++this.epoch;
    this.selectedDetailId = null;
    this.publish({ busy: endpoint.endpoint_id, stale: true, loading: false, error: null,
      refreshError: null, result: null, details: null, detailsLoading: false, detailsError: null });
    try {
      const updated = await this.gateway.control({ profileId: this.view.catalog!.profileId!, configRevision: endpoint.config_revision, endpointId: endpoint.endpoint_id,
        coreInstanceId: endpoint.core_instance_id, expectedIntentRevision: endpoint.intent_revision, action });
      if (this.current(epoch)) this.publish({
        catalog: { ...this.view.catalog!, endpoints: this.view.catalog!.endpoints.map(row => row.endpoint_id === updated.endpoint_id ? updated : row) },
        result: `${endpoint.tag}：内核已确认操作完成`,
      });
    } catch (error) {
      if (this.current(epoch)) this.publish({ error });
    } finally {
      this.publish({ busy: null });
      // Keep controls stale until a fresh outcome is known. Never replay a
      // timed-out mutation; it may already have applied or rolled back.
      await this.refresh(false);
    }
  }
  private async readDetails(endpoint: NetworkEndpoint, epoch: number) {
    this.publish({ detailsLoading: true, detailsError: null });
    try {
      const details = await this.gateway.details(endpoint.endpoint_id, endpoint.core_instance_id);
      if (!this.current(epoch)) return;
      if (details.endpoint_id !== endpoint.endpoint_id || details.generation !== endpoint.generation) {
        throw { message: '端点实例已变化，请刷新详情' };
      }
      this.publish({ details });
    } catch (detailsError) {
      if (this.current(epoch)) this.publish({ details: null, detailsError });
    } finally {
      if (this.current(epoch)) this.publish({ detailsLoading: false });
    }
  }
  async inspect(endpoint: NetworkEndpoint) {
    if (this.disposed || this.view.busy || this.view.stale || this.view.loading || this.view.detailsLoading) return;
    if (!this.isCurrentEndpoint(endpoint)) { this.publish({ detailsError: { message: '端点观测已变化，请刷新详情' } }); return; }
    this.selectedDetailId = endpoint.endpoint_id;
    const epoch = ++this.epoch;
    this.publish({ details: null });
    await this.readDetails(endpoint, epoch);
  }
}
