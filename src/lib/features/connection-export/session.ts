import type {DebugFrameQuery} from '$lib/types/debug';
import type {DocumentDelivery,NativeDocumentReceipt} from '$lib/services/native-documents';
import type {ConnectionExportGateway,ConnectionExportPreview} from './types';
/** A confirmation submits one fixed host snapshot exactly once. */
export class ConnectionExportSession {
 plan:ConnectionExportPreview|null=null;receipt:NativeDocumentReceipt|null=null;error:unknown=null;busy=false;submitted=false;
 private generation=0;
 private alive=true;
 private gateway:ConnectionExportGateway;
 constructor(gateway:ConnectionExportGateway){this.gateway=gateway;}
 async preview(query:DebugFrameQuery){if(!this.alive)return;const generation=++this.generation;this.plan=null;this.receipt=null;this.error=null;this.submitted=false;this.busy=true;try{const plan=await this.gateway.preview(structuredClone(query));if(generation===this.generation)this.plan=plan;}catch(error){if(generation===this.generation)this.error=error;}finally{if(generation===this.generation)this.busy=false;}}
 async deliver(delivery:DocumentDelivery,acknowledged:boolean){if(!this.alive||!this.plan||this.busy||this.submitted||!acknowledged)return;const generation=this.generation,plan=this.plan;this.submitted=true;this.busy=true;this.error=null;try{const receipt=await this.gateway.deliver(plan.planId,delivery);if(generation===this.generation)this.receipt=receipt;}catch(error){if(generation===this.generation){const code=error&&typeof error==='object'&&'code' in error?String(error.code):'';const refused=['conflict','invalid_argument','permission_denied','not_found','unsupported'].includes(code);this.receipt={operationId:plan.planId,status:refused?'failed':'result-unknown',content:null,bytesRead:null,reason:refused?'预览或系统导出被拒绝，请关闭并重新预览；原记录保留':'系统导出结果未确认，请检查目标；不会自动重放'};}}finally{if(generation===this.generation)this.busy=false;}}
 dispose(){this.alive=false;this.generation++;this.busy=false;}
}
