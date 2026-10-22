import { operationReason, sourceRestoreReason, restorationMessage, restorationUnresolved } from '$lib/features/endpoints/policy';
import type { EndpointAction, EndpointCatalog, EndpointDetails, EndpointGateway, NetworkEndpoint, RestorationPlan, RestorationReceipt, RestoreMode } from './types';
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
  restoration: RestorationReceipt | null;
}
// Discard publications from an old scope. A submitted kernel transaction still
// finishes at the host; cancellation here never claims to undo that operation.
export class EndpointSession {
  view: EndpointView = {
    catalog: null, loading: false, busy: null, stale: true, error: null,
    refreshError: null, result: null, details: null, detailsLoading: false, detailsError: null, traffic: {}, restoration:null,
  };
  private epoch = 0;
  private disposed = false;
  private restorationPlans = new Map<string,{endpoint:NetworkEndpoint;profileId:string}>();
  private selectedDetailId: string | null = null;
  private refreshRequest: number | null = null;
  private refreshSettled: Promise<void> | null = null;
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
    this.restorationPlans.clear();
    this.refreshRequest = null;
    this.selectedDetailId = null;
    // Last-known observations remain visible, but stale blocks all mutations.
    // The next authoritative catalog replaces identities and traffic scope.
    this.publish({ details: null, detailsLoading: false, detailsError: null,
      stale: true, loading: false, error: null, refreshError: null, result: null, restoration:null });
  }
  dispose() { this.restorationPlans.clear(); this.disposed = true; this.epoch++; }
  /** Await an existing directory read, then obtain a new authoritative navigation check. */
  async refreshForNavigation(): Promise<boolean> {
    if (this.refreshSettled) await this.refreshSettled;
    if (this.disposed || this.view.busy || this.view.detailsLoading || this.refreshRequest !== null) return false;
    await this.refresh(false);
    return !this.disposed && !this.view.stale && !this.view.loading && this.refreshRequest === null;
  }
  async refresh(clearError = true, background = false) {
    if (this.disposed || this.view.busy || this.refreshRequest !== null || this.view.detailsLoading) return;
    const epoch = ++this.epoch;
    let finish!: () => void;
    const settled = new Promise<void>(resolve => { finish = resolve; });
    this.refreshSettled = settled;
    this.refreshRequest = epoch;
    if (!background) this.publish({ loading: true });
    try {
      const catalog = await this.gateway.catalog();
      if (!this.current(epoch)) return;
      const old = this.view.catalog?.endpoints[0];
      const first = catalog.endpoints[0];
      const changedScope = old && first && (old.core_instance_id !== first.core_instance_id || old.config_revision !== first.config_revision);
      if (changedScope) this.selectedDetailId = null;
      const selected = catalog.endpoints.find(row => row.endpoint_id === this.selectedDetailId);
      const previousDetails = this.view.details;
      const details = selected?.supported.operations.includes('details') && previousDetails
        && selected.generation === previousDetails.generation
        && selected.endpoint_id === previousDetails.endpoint_id ? previousDetails : null;
      const traffic = Object.fromEntries(catalog.endpoints.map(endpoint => [endpoint.endpoint_id,
        sampleEndpointTraffic(this.view.traffic[endpoint.endpoint_id], endpoint)]));
      this.publish({ catalog, stale: false, error: clearError ? null : this.view.error,
        refreshError: null, result: changedScope ? null : this.view.result,
        details, detailsError: null, traffic });
      if (this.gateway.restorationReceipts) {
        const receipts=await this.gateway.restorationReceipts();
        if (!this.current(epoch)) return;
        // The host filters persisted records by exact current source digest,
        // source version and client instance, including missing apply replies.
        const relevant=receipts.filter(receipt=>receipt.target.profileId===catalog.profileId && catalog.endpoints.some(row=>row.endpoint_id===receipt.target.endpointId && row.tag===receipt.target.tag && row.protocol===receipt.target.protocol));
        const retained=relevant.find(receipt=>restorationUnresolved(receipt)) ?? relevant.find(receipt=>receipt.phase==='saved_pending') ?? relevant.find(receipt=>receipt.operationId===this.view.restoration?.operationId);
        if(retained)this.publish({restoration:retained,result:restorationMessage(retained)});
      }
      if (selected?.supported.operations.includes('details')) await this.readDetails(selected, epoch, background);
      else this.selectedDetailId = null;
    } catch (refreshError) {
      if (this.current(epoch)) this.publish({ stale: true, refreshError, details: null });
    } finally {
      finish();
      if (this.refreshSettled === settled) this.refreshSettled = null;
      if (this.refreshRequest === epoch) this.refreshRequest = null;
      if (this.current(epoch) && !background) this.publish({ loading: false });
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
    if (this.view.restoration?.target.profileId===this.view.catalog?.profileId && this.view.restoration?.target.endpointId===endpoint.endpoint_id && restorationUnresolved(this.view.restoration)) { this.publish({error:{message:'此端点有未核对的恢复事务，请先核对原操作'}}); return; }
    const reason = this.isCurrentEndpoint(endpoint)
      ? operationReason(this.view.catalog, endpoint, action) : '端点观测已变化，请刷新后重试';
    if (reason) { this.publish({ error: { message: reason } }); return; }
    const epoch = ++this.epoch;
    this.refreshRequest = null;
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
  async previewRestoration(endpoint:NetworkEndpoint):Promise<RestorationPlan|null> {
    if (this.disposed || this.view.busy || !this.gateway.previewRestoration) return null;
    const reason=this.isCurrentEndpoint(endpoint) ? sourceRestoreReason(this.view.catalog,endpoint) : '目标端点已变化，请刷新';
    if(reason){this.publish({error:{message:reason}});return null;}
    const binding=this.view.catalog!.restorationTargets!.find(target=>target.endpointId===endpoint.endpoint_id)!;
    const epoch=++this.epoch;this.refreshRequest=null;this.publish({busy:endpoint.endpoint_id,error:null,result:null});
    try {
      const plan=await this.gateway.previewRestoration(binding.bindingId);
      if(!this.current(epoch))return null;
      if(plan.target.endpointId!==endpoint.endpoint_id || plan.target.profileId!==this.view.catalog?.profileId || plan.target.tag!==endpoint.tag || plan.target.protocol!==endpoint.protocol)throw {message:'恢复预检目标不匹配'};
      this.restorationPlans.set(plan.planId,{endpoint,profileId:plan.target.profileId});return plan;
    }catch(error){if(this.current(epoch))this.publish({error});return null;}
    finally{if(this.current(epoch))this.publish({busy:null});}
  }
  async applyRestoration(plan:RestorationPlan,mode:RestoreMode):Promise<void> {
    if(this.disposed || this.view.busy || !this.gateway.applyRestoration)return;
    const retained=this.restorationPlans.get(plan.planId);
    if(!retained || retained.profileId!==this.view.catalog?.profileId || !this.isCurrentEndpoint(retained.endpoint)){this.publish({error:{message:'恢复确认所属配置或端点已变化，请重新预检'}});return;}
    this.restorationPlans.delete(plan.planId);
    const epoch=++this.epoch;this.refreshRequest=null;this.publish({busy:plan.target.endpointId,stale:true,error:null,result:null});
    try{const receipt=await this.gateway.applyRestoration(plan.planId,mode);if(!this.current(epoch))return;
      if(receipt.operationId!==plan.planId || receipt.target.endpointId!==plan.target.endpointId || receipt.target.profileId!==plan.target.profileId)throw {message:'恢复事务回执身份不匹配，结果未确认'};
      this.publish({restoration:receipt,result:restorationMessage(receipt)});
    }catch(error){if(this.current(epoch))this.publish({error,restoration:{operationId:plan.planId,target:plan.target,phase:'result_unknown',localSaved:false,runtimeConfirmed:false,applyIdentity:null,endpoint:null,message:'提交回执未能读取，请核对原操作；不会自动重放'},result:`${plan.target.tag}：结果未确认，请核对原恢复操作`});}
    finally{if(this.current(epoch)){this.publish({busy:null});await this.refresh(false);}}
  }
  async reconcileRestoration():Promise<void>{
    const receipt=this.view.restoration;
    if(this.disposed || this.view.busy || !receipt || !this.gateway.reconcileRestoration)return;
    const epoch=++this.epoch;this.publish({busy:receipt.target.endpointId,error:null});
    try{const next=await this.gateway.reconcileRestoration(receipt.operationId);if(this.current(epoch)){
      if(next.operationId!==receipt.operationId || next.target.endpointId!==receipt.target.endpointId || next.target.profileId!==receipt.target.profileId)throw {message:'原恢复事务回执身份不匹配'};
      this.publish({restoration:next,result:restorationMessage(next)});
    }}catch(error){if(this.current(epoch))this.publish({error});}
    finally{if(this.current(epoch)){this.publish({busy:null});await this.refresh(false);}}
  }
  private async readDetails(endpoint: NetworkEndpoint, epoch: number, background = false) {
    if (!background) this.publish({ detailsLoading: true, detailsError: null });
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
      if (this.current(epoch) && !background) this.publish({ detailsLoading: false });
    }
  }
  async inspect(endpoint: NetworkEndpoint) {
    if (this.disposed || this.view.busy || this.view.stale || this.view.loading || this.view.detailsLoading) return;
    if (!this.isCurrentEndpoint(endpoint)) { this.publish({ detailsError: { message: '端点观测已变化，请刷新详情' } }); return; }
    this.selectedDetailId = endpoint.endpoint_id;
    const epoch = ++this.epoch;
    this.refreshRequest = null;
    this.publish({ details: null });
    await this.readDetails(endpoint, epoch);
  }
}
