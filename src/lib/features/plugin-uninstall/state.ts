import type { PluginUninstallItemState, PluginUninstallPorts, PluginUninstallPreview, PluginUninstallStatus, PluginUninstallView } from './types';
const initial = (): PluginUninstallView => ({phase:'idle',preview:null,status:null,operationId:null,acknowledged:false,message:null,records:[],recordsError:null});
const text = (value: unknown, max = 4096): value is string => typeof value === 'string' && value.length > 0 && value.length <= max;
const nullableText = (value: unknown) => value === null || (typeof value === 'string' && value.length <= 4096);
const u64 = (value: unknown): value is string => typeof value === 'string' && /^(0|[1-9]\d{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;
const id = (value: unknown): value is string => typeof value === 'string' && /^[0-9a-f]{32}$/.test(value);
const itemStates: PluginUninstallItemState[] = ['pending','executing','completed','residual','unknown','manual'];
export const itemStateLabel: Record<PluginUninstallItemState,string> = {pending:'尚未处理',executing:'正在处理',completed:'原操作已确认完成',residual:'仍有剩余',unknown:'结果未确认',manual:'需手动处理'};
export function uninstallTime(value: string | null) { return u64(value) && BigInt(value) > 0n && BigInt(value) <= 8640000000000000n ? new Date(Number(value)).toLocaleString() : '—'; }
function preview(value: PluginUninstallPreview, pluginId: string) {
  if (!value || value.pluginId !== pluginId || !text(value.planId,256) || !text(value.pluginId,256) || !text(value.pluginName)
    || !text(value.version,256) || !text(value.packageDigest,256) || !text(value.publisherId,256) || !text(value.publisherFingerprint,512)
    || !text(value.hostSessionId,256) || !text(value.binding,512) || !u64(value.expiresAtUnixMs) || typeof value.canExecute !== 'boolean'
    || typeof value.requiresManualSwitch !== 'boolean' || value.requiresStop !== false || typeof value.notice !== 'string' || value.notice.length > 8192
    || !Array.isArray(value.blockedReasons) || value.blockedReasons.length > 128 || !value.blockedReasons.every(v=>text(v))
    || !(value.activeProfile === null || (text(value.activeProfile.id,256) && text(value.activeProfile.name)))
    || !Array.isArray(value.replacementCandidates) || value.replacementCandidates.length > 128 || !value.replacementCandidates.every(v=>text(v.id,256) && text(v.name))
    || new Set(value.replacementCandidates.map(v=>v.id)).size !== value.replacementCandidates.length
    || !Array.isArray(value.resources) || !value.resources.length || value.resources.length > 128
    || !value.resources.every(v=>text(v.resourceId,256) && text(v.kind,128) && text(v.name)
      && ['verified-package','exact-managed-source','verified-component','publisher-namespace'].includes(v.ownership)
      && Array.isArray(v.dependencies) && v.dependencies.length <= 128 && v.dependencies.every(d=>text(d)) && nullableText(v.protectedReason))
    || new Set(value.resources.map(v=>v.resourceId)).size !== value.resources.length) throw Error('invalid preview');
  return structuredClone(value);
}
function status(value: PluginUninstallStatus, operationId: string, source: PluginUninstallPreview | null, previousItems: PluginUninstallStatus['items'] | null = null) {
  if (!value || value.operationId !== operationId || !id(value.operationId) || !text(value.planId,256) || !text(value.pluginId,256)
    || !text(value.pluginName) || !text(value.version,256) || !text(value.packageDigest,256) || !text(value.publisherFingerprint,512)
    || !['prepared','executing','completed','residual','unknown','manual'].includes(value.state) || value.canRetry !== false
    || !nullableText(value.reason) || typeof value.notice !== 'string' || value.notice.length > 8192
    || ![value.startedAtUnixMs,value.finishedAtUnixMs].every(v=>v === null || u64(v)) || !Array.isArray(value.items) || value.items.length > 128
    || !value.items.every(v=>text(v.resourceId,256) && text(v.kind,128) && text(v.name) && itemStates.includes(v.state) && nullableText(v.reason))) throw Error('invalid status');
  if (source && (value.planId !== source.planId || value.pluginId !== source.pluginId || value.version !== source.version
    || value.packageDigest !== source.packageDigest || value.publisherFingerprint !== source.publisherFingerprint
    || value.items.some(i=>!source.resources.some(r=>r.resourceId === i.resourceId)))) throw Error('borrowed status');
  const copied = structuredClone(value);
  const fixedResources = source?.resources ?? previousItems;
  if (fixedResources) {
    if(copied.items.some(i=>!fixedResources.some(r=>r.resourceId===i.resourceId)))throw Error('expanded resource set');
    copied.items = fixedResources.map(resource=>{
      const found = copied.items.filter(i=>i.resourceId === resource.resourceId);
      return found.length === 1 && found[0].kind === resource.kind && found[0].name === resource.name ? found[0]
        : {resourceId:resource.resourceId,kind:resource.kind,name:resource.name,state:'unknown',reason:'原回执缺少此固定资源或身份重复，未确认清理结果。'};
    });
  } else if (!copied.items.length || new Set(copied.items.map(i=>i.resourceId)).size !== copied.items.length) throw Error('unverifiable records');
  if (copied.state === 'completed' && (!copied.finishedAtUnixMs || copied.items.some(i=>i.state !== 'completed'))) {
    copied.state = 'unknown'; copied.reason = '原回执尚未逐项确认全部固定资源；不能将目录为空或部分完成视为全部完成。';
  }
  return copied;
}
function safeFailure(error: unknown) {
  const code = (error as {code?:unknown}|null)?.code;
  return code === 'unsupported' ? '当前平台尚未接入真实插件卸载能力；未执行模拟清理。'
    : code === 'forbidden' || code === 'permission_denied' || code === 'mode_restricted' ? '当前没有此宿主管理权限，原材料与操作记录保留。'
    : '宿主核对未完成；原材料和原操作状态均不能据此确认，请稍后重试只读核对。';
}
/** One UI projection. Host recent/status owns durable records and all actual cleanup. */
export class PluginUninstallSession {
  view = initial();
  private epoch = 0;
  private pending = new Map<string, Pick<PluginUninstallView,'preview'|'status'|'operationId'>>();
  private listeners = new Set<(value: PluginUninstallView)=>void>();
  constructor(private ports: PluginUninstallPorts, private nextOperationId = ()=>crypto.randomUUID().replaceAll('-','')) {}
  subscribe(listener:(value:PluginUninstallView)=>void) { this.listeners.add(listener); listener(this.view); return ()=>{ this.listeners.delete(listener); }; }
  private retainCurrent() {
    const pluginId = this.view.preview?.pluginId ?? this.view.status?.pluginId;
    if(!pluginId || !this.view.operationId)return;
    if(this.view.status?.state==='completed') {
      if(this.pending.get(pluginId)?.operationId===this.view.operationId)this.pending.delete(pluginId);
    } else this.pending.set(pluginId,{preview:this.view.preview,status:this.view.status,operationId:this.view.operationId});
  }
  private emit() { this.retainCurrent(); this.view = {...this.view}; for(const listener of this.listeners) listener(this.view); }
  get busy() { return ['loading','submitting','checking'].includes(this.view.phase); }
  get unresolved() { return !!this.view.operationId && this.view.status?.state !== 'completed'; }
  get canExecute() { const p=this.view.preview; return this.view.phase === 'review' && !this.view.operationId && this.view.acknowledged
    && !!p?.canExecute && !p.requiresManualSwitch && !p.blockedReasons.length && !p.resources.some(r=>r.protectedReason !== null); }
  expired(now=Date.now()) { const deadline=this.view.preview?.expiresAtUnixMs; return !deadline || BigInt(deadline) <= BigInt(now); }
  acknowledge(value:boolean) { if(this.view.phase === 'review' && !this.view.operationId) { this.view.acknowledged=value; this.emit(); } }
  async open(pluginId:string) {
    if(this.busy || !text(pluginId,256)) return;
    this.retainCurrent();
    const original=this.pending.get(pluginId);
    if(original) { if(this.view.preview&&!this.view.operationId)void this.ports.discard(this.view.preview.planId).catch(()=>{});++this.epoch;this.view={...initial(),...original,records:this.view.records,phase:'result',message:'此插件的原卸载操作仍需核对；不会生成新计划或重复卸载。'};this.emit();return; }
    if(this.pending.size>=16){this.view.message='已保留 16 项原操作记录，预算已满；请先只读核对，不能丢弃未知证据继续提交。';this.emit();return;}
    const old=this.view.preview; const epoch=++this.epoch;
    if(old && !this.view.operationId) void this.ports.discard(old.planId).catch(()=>{});
    this.view={...initial(),records:this.view.records,phase:'loading'};this.emit();
    try { const p=preview(await this.ports.preview(pluginId),pluginId); if(epoch !== this.epoch) { void this.ports.discard(p.planId).catch(()=>{});return; }
      this.view.preview=p;this.view.phase='review';this.view.message='以下为本次宿主固定目标；同名普通用户资源不因名称相同加入清理。';
    } catch(error) { if(epoch !== this.epoch)return;this.view.phase='error';this.view.message=safeFailure(error); } this.emit();
  }
  invalidateReview() {
    if(this.view.operationId || !['loading','review'].includes(this.view.phase)) return;
    ++this.epoch;const p=this.view.preview;if(p)void this.ports.discard(p.planId).catch(()=>{});
    this.view.phase='error';this.view.acknowledged=false;this.view.message='原预览环境已变化；请明确重新读取，旧目标不会继续提交。';this.emit();
  }
  cancelUnsubmitted() {
    if(this.busy || this.view.operationId) return false;
    const p=this.view.preview;++this.epoch;if(p)void this.ports.discard(p.planId).catch(()=>{});
    this.view={...initial(),records:this.view.records};this.emit();return true;
  }
  private unknown(message:string) {
    const p=this.view.preview;if(!p || !this.view.operationId)return;
    this.view.status={operationId:this.view.operationId,planId:p.planId,pluginId:p.pluginId,pluginName:p.pluginName,version:p.version,packageDigest:p.packageDigest,publisherFingerprint:p.publisherFingerprint,
      state:'unknown',reason:message,startedAtUnixMs:null,finishedAtUnixMs:null,items:p.resources.map(r=>({resourceId:r.resourceId,kind:r.kind,name:r.name,state:'unknown',reason:'未取得原资源最终清理回执。'})),canRetry:false,notice:'保留原操作编号；当前安装列表或目录是否为空不能替代原清理回执。'};
    this.view.phase='result';this.view.message=message;
  }
  async execute() {
    const p=this.view.preview;if(!p || !this.canExecute)return;
    if(this.expired()){this.invalidateReview();return;}
    const operationId=this.nextOperationId();if(!id(operationId)){this.view.message='无法安全生成原操作编号，未提交卸载。';this.emit();return;}
    const epoch=this.epoch;this.view.operationId=operationId;this.view.phase='submitting';this.emit();
    try{const result=await this.ports.execute({planId:p.planId,operationId,confirmOwnedResources:true});if(epoch !== this.epoch)return;
      this.view.status=status(result,operationId,p);this.view.phase='result';this.view.message='这里只显示原操作逐项回执；运行与当前资源状态另行核对。';
    }catch(error){if(epoch !== this.epoch)return;
      if((error as {code?:unknown}|null)?.code==='plugin_uninstall_preflight_rejected'){
        void this.ports.discard(p.planId).catch(()=>{});if(this.pending.get(p.pluginId)?.operationId===operationId)this.pending.delete(p.pluginId);this.view.operationId=null;this.view.acknowledged=false;this.view.phase='error';
        this.view.message='宿主在开始卸载前明确拒绝了原预览；未执行停用或删除，请重新明确预览。';
      }else this.unknown('卸载提交结果未确认；只读核对原编号，不会重新执行或生成新编号。');
    }this.emit();
  }
  async check() {
    const operationId=this.view.operationId;if(!operationId || this.busy)return;
    const source=this.view.preview, before=this.view.status;const epoch=this.epoch;this.view.phase='checking';this.emit();
    try{const result=status(await this.ports.status(operationId),operationId,source,before?.items ?? null);if(epoch !== this.epoch)return;
      if(before && (result.planId !== before.planId || result.pluginId !== before.pluginId || result.packageDigest !== before.packageDigest || result.version !== before.version || result.publisherFingerprint !== before.publisherFingerprint))throw Error('changed operation');
      this.view.status=result;this.view.message='已只读核对原操作；没有重新执行清理，也未以当前目录替代回执。';
    }catch(error){if(epoch !== this.epoch)return;this.view.message=`原操作状态仍未确认，保留原逐项记录。${safeFailure(error)}`;}
    if(epoch === this.epoch){this.view.phase='result';this.emit();}
  }
  async recent() {
    if(this.busy)return;const old=this.view.preview;if(old && !this.view.operationId)void this.ports.discard(old.planId).catch(()=>{});
    const epoch=++this.epoch;this.view.phase='loading';this.view.acknowledged=false;this.view.recordsError=null;this.emit();
    try{const records=await this.ports.recent();if(epoch !== this.epoch)return;
      if(!Array.isArray(records) || records.length>16 || new Set(records.map(r=>r.operationId)).size !== records.length)throw Error('invalid recent');
      this.view.records=records.map(r=>status(r,r.operationId,null));this.view.phase='records';this.view.message='以下来自宿主持久记录；记录缺失或列表为空不代表清理成功。';
    }catch(error){if(epoch !== this.epoch)return;this.view.phase='records';this.view.recordsError=`卸载记录读取失败；旧记录仅供历史核对。${safeFailure(error)}`;}this.emit();
  }
  showRecord(operationId:string) {
    if(this.busy)return;const record=this.view.records.find(r=>r.operationId === operationId);if(!record)return;
    this.retainCurrent();++this.epoch;this.view.preview=null;this.view.operationId=operationId;this.view.status=structuredClone(record);this.view.phase='result';this.view.acknowledged=false;
    this.view.message='已打开原卸载记录；需要更新结果时只读核对原操作，不能重放清理。';this.emit();
  }
}
