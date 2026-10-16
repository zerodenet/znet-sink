import type {FlowCloseInventory,FlowCloseItem,FlowCloseOutcome,FlowClosePorts,FlowClosePreview,FlowCloseResult,FlowCloseTarget,FlowCloseReceipt,FlowCloseView} from '$lib/features/flow-close/types';
const initial=():FlowCloseView=>({receipts:[],phase:'idle',inventory:null,selectedKeys:[],plan:null,items:[],message:null,checking:false,attempted:false});
const text=(v:unknown):v is string=>typeof v==='string' && v.length>0 && v.length<=2048;
const u64=(v:unknown):v is string=>typeof v==='string' && /^(0|[1-9]\d{0,19})$/.test(v) && BigInt(v)<=18446744073709551615n;
const outcomes:FlowCloseOutcome[]=['closed','ended','rejected','unknown','stale','not_submitted'];
export const outcomeLabel:Record<FlowCloseOutcome,string>={closed:'已关闭',ended:'已结束',rejected:'已拒绝',unknown:'结果未确认',stale:'已失效',not_submitted:'未提交'};
function validTarget(t:FlowCloseTarget){return t && u64(t.flowId) && text(t.targetKey) && u64(t.startedAtUnixMs) && text(t.network) && typeof t.destination==='string' && t.destination.length<=4096 && typeof t.canClose==='boolean' && (t.reason===null || typeof t.reason==='string');}
function inventory(value:FlowCloseInventory){if(!value || !text(value.inventoryId) || !text(value.coreInstanceId) || !u64(value.configRevision) || !u64(value.clientScopeGeneration) || value.clientScopeGeneration==='0' || !u64(value.observedAtUnixMs) || value.limit!==500 || typeof value.truncated!=='boolean' || typeof value.notice!=='string' || !Array.isArray(value.targets) || value.targets.length>500 || !value.targets.every(validTarget) || new Set(value.targets.map(t=>t.targetKey)).size!==value.targets.length || new Set(value.targets.map(t=>t.flowId)).size!==value.targets.length)throw Error('invalid inventory');return structuredClone(value);}
function failure(error:unknown){const code=(error as {code?:string}|null)?.code;return code==='unsupported'?'当前平台或内核未接入真实连接关闭能力。':code==='permission_denied'||code==='mode_restricted'?'当前没有关闭连接的权限；原连接未因此确认关闭。':code==='expired'?'原预览已过期，请重新读取实际连接并明确选择。':code==='busy'?'宿主仍在处理原操作，请只读核对其状态。':'无法完成本次核对；原操作和连接状态均未据此确认。';}
export function displayTime(value:string){if(!u64(value) || BigInt(value)>8640000000000000n)return '时间未提供';return new Date(Number(value)).toLocaleString();}
/** UI projection of one host-owned operation. It never queues/replays close commands. */
export class FlowCloseSession {
 view=initial();private epoch=0;private listeners=new Set<(view:FlowCloseView)=>void>();private ports:FlowClosePorts;
 constructor(ports:FlowClosePorts){this.ports=ports;}
 subscribe(listener:(view:FlowCloseView)=>void){this.listeners.add(listener);listener(this.view);return()=>this.listeners.delete(listener);}
 private emit(){this.view={...this.view};for(const listener of this.listeners)listener(this.view);}
 get busy(){return ['loading','previewing','submitting'].includes(this.view.phase)||this.view.checking;}
 private retained():FlowCloseReceipt[]{const values=[...this.view.receipts];if(this.view.attempted&&this.view.plan){const item={plan:structuredClone(this.view.plan),items:structuredClone(this.view.items),message:this.view.message};const pool=[...values.filter(v=>v.plan.planId!==item.plan.planId),item];while(pool.length>8){const complete=pool.findIndex(r=>!r.items.some(i=>i.outcome==='unknown'));if(complete<0)break;pool.splice(complete,1);}return pool;}return values;}
 showReceipt(planId:string){if(this.busy)return;const receipts=this.retained(),item=receipts.find(r=>r.plan.planId===planId);if(!item)return;const disposable=!this.view.attempted?(this.view.plan?.planId??this.view.inventory?.inventoryId):null;if(disposable)void this.ports.discard(disposable).catch(()=>{});++this.epoch;this.view={...initial(),receipts,phase:'results',plan:structuredClone(item.plan),items:structuredClone(item.items),message:item.message,attempted:true};this.emit();}
 async start(){if(this.busy)return;const previous=this.view,receipts=this.retained();if(receipts.filter(r=>r.items.some(i=>i.outcome==='unknown')).length>=8){this.view.receipts=receipts;this.view.message='已保留 8 次结果未确认的原操作，记录预算已满；请先只读核对，不能丢弃原证据继续提交。';this.emit();return;}const epoch=++this.epoch;this.view={...initial(),receipts};this.view.phase='loading';this.emit();
  // Releasing a read/unused plan never retracts an already submitted command.
  const disposable=!previous.attempted?(previous.plan?.planId??previous.inventory?.inventoryId):null;
  if(disposable)void this.ports.discard(disposable).catch(()=>{});
  try{const value=inventory(await this.ports.inventory());if(this.epoch!==epoch)return;this.view.inventory=value;this.view.phase='selecting';this.view.message='请从本次真实核对列表明确选择。旧活动列表、暂停内容及原勾选不会转成控制权限。';}
  catch(error){if(this.epoch!==epoch)return;this.view.phase='error';this.view.message=failure(error);}this.emit();
 }
 toggle(key:string){if(this.view.phase!=='selecting')return;const target=this.view.inventory?.targets.find(t=>t.targetKey===key && t.canClose);if(!target)return;const keys=new Set(this.view.selectedKeys);if(keys.has(key))keys.delete(key);else keys.add(key);this.view.selectedKeys=[...keys];this.emit();}
 selectAll(keys:string[]){if(this.view.phase!=='selecting')return;const allowed=new Set(this.view.inventory?.targets.filter(t=>t.canClose).map(t=>t.targetKey));this.view.selectedKeys=[...new Set(keys)].filter(k=>allowed.has(k));this.emit();}
 async preview(){const source=this.view.inventory;if(this.view.phase!=='selecting'||!source||!this.view.selectedKeys.length)return;const epoch=this.epoch;const fixed=source.targets.filter(t=>this.view.selectedKeys.includes(t.targetKey)).map(t=>({flowId:t.flowId,targetKey:t.targetKey}));this.view.phase='previewing';this.view.message=null;this.emit();
  try{const plan=await this.ports.preview({inventoryId:source.inventoryId,targets:structuredClone(fixed)});if(this.epoch!==epoch)return;
   if(!plan || !text(plan.planId)||plan.coreInstanceId!==source.coreInstanceId||plan.configRevision!==source.configRevision||!u64(plan.expiresAtUnixMs)||!Array.isArray(plan.targets)||plan.targets.length!==fixed.length||plan.targets.some(t=>!validTarget(t)||!['active','ended','unavailable'].includes(t.state)||!fixed.some(f=>f.flowId===t.flowId&&f.targetKey===t.targetKey))||new Set(plan.targets.map(t=>t.flowId)).size!==fixed.length)throw Error('invalid preview');
   this.view.plan=structuredClone(plan);this.view.phase='confirming';this.view.message='确认只提交此处固定目标；后续新连接和选择变化不会加入。';
  }catch(error){if(this.epoch!==epoch)return;this.view.phase='error';this.view.message=failure(error);}this.emit();
 }
 expired(now=Date.now()){const expires=this.view.plan?.expiresAtUnixMs;return !expires || !u64(expires) || BigInt(expires)<=BigInt(now);}
 async submit(){const plan=this.view.plan;if(this.view.phase!=='confirming'||!plan||this.view.attempted)return;if(this.expired()){this.view.message='原预览已过期；未发出关闭命令，请重新核对并选择。';this.emit();return;}const epoch=this.epoch;this.view.attempted=true;this.view.phase='submitting';this.emit();
  try{const result=await this.ports.submit(plan.planId);if(this.epoch!==epoch)return;this.deliver(result,plan);}
  catch{if(this.epoch!==epoch)return;this.unknown(plan,'原提交回执未确认；只读核对原操作，不会重放关闭。');}this.emit();
 }
 private deliver(value:FlowCloseResult,plan:FlowClosePreview){if(!value||value.planId!==plan.planId||value.state!=='finished'||!Array.isArray(value.items)||value.items.length>500||value.items.some(i=>!plan.targets.some(t=>t.flowId===i.flowId))){this.unknown(plan,'回执与原操作不一致；结果未确认，不会重放。');return;}
  this.view.items=plan.targets.map(t=>{const found=value.items.filter(i=>i.flowId===t.flowId);const item=found.length===1?found[0]:null;return item&&outcomes.includes(item.outcome)?{flowId:t.flowId,outcome:item.outcome,reason:typeof item.reason==='string'?item.reason:'宿主未提供原因'}:{flowId:t.flowId,outcome:'unknown',reason:'原回执缺少此目标或包含重复目标，未确认其结果。'};});
  this.view.phase='results';this.view.message='逐项结果来自原宿主操作。未确认项不会自动重试；活动列表仍由真实观测更新。';
 }
 private unknown(plan:FlowClosePreview,message:string){this.view.items=plan.targets.map(t=>({flowId:t.flowId,outcome:'unknown',reason:'未取得原目标的最终确认'}));this.view.phase='results';this.view.message=message;}
 async check(){const plan=this.view.plan;if(!this.view.attempted||!plan||this.view.checking||this.view.phase==='submitting')return;const epoch=this.epoch;this.view.checking=true;this.emit();
  try{const status=await this.ports.status(plan.planId);if(this.epoch!==epoch)return;if(!status||status.planId!==plan.planId||!['prepared','submitted','finished'].includes(status.state)||(status.state==='finished'?!status.result:status.result!==null))throw Error('wrong status');if(status.state==='finished' && status.result)this.deliver(status.result,plan);else this.view.message=status.state==='submitted'?'原操作仍在执行；尚未取得逐项最终回执，不会重复提交。':'原操作未有最终确认；此检查不会补发或重放关闭。';}
  catch{if(this.epoch!==epoch)return;this.view.message='原操作状态仍无法确认；保留原结果，不会重放。';}finally{if(this.epoch===epoch)this.view.checking=false;this.emit();}
 }
 cancel(){if(this.busy)return false;const id=!this.view.attempted?(this.view.plan?.planId??this.view.inventory?.inventoryId):null;if(id){const receipts=this.retained();++this.epoch;this.view={...initial(),receipts};void this.ports.discard(id).catch(()=>{});this.emit();}return true;}
}
