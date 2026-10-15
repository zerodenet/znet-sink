/** All identities/counters are host-issued strings, never legacy GUI numbers. */
export interface FlowCloseTarget {flowId:string;targetKey:string;startedAtUnixMs:string;network:string;destination:string;canClose:boolean;reason:string|null}
export interface FlowCloseInventory {inventoryId:string;coreInstanceId:string;configRevision:string;clientScopeGeneration:string;observedAtUnixMs:string;targets:FlowCloseTarget[];limit:number;truncated:boolean;notice:string}
export interface FlowClosePreview {planId:string;coreInstanceId:string;configRevision:string;expiresAtUnixMs:string;targets:(FlowCloseTarget & {state:'active'|'ended'|'unavailable'})[];notice:string}
export type FlowCloseOutcome='closed'|'ended'|'rejected'|'unknown'|'stale'|'not_submitted';
export interface FlowCloseItem {flowId:string;outcome:FlowCloseOutcome;reason:string}
export interface FlowCloseResult {planId:string;state:'finished';items:FlowCloseItem[];counts:{closed:number;ended:number;rejected:number;unknown:number;stale:number;notSubmitted:number}}
export interface FlowCloseStatus {planId:string;state:'prepared'|'submitted'|'finished';result:FlowCloseResult|null}
export interface FlowClosePorts {inventory():Promise<FlowCloseInventory>;preview(input:{inventoryId:string;targets:{flowId:string;targetKey:string}[]}):Promise<FlowClosePreview>;submit(planId:string):Promise<FlowCloseResult>;status(planId:string):Promise<FlowCloseStatus>;discard(id:string):Promise<unknown>}
export interface FlowCloseReceipt {plan:FlowClosePreview;items:FlowCloseItem[];message:string|null}
export interface FlowCloseView {receipts:FlowCloseReceipt[];phase:'idle'|'loading'|'selecting'|'previewing'|'confirming'|'submitting'|'results'|'error';inventory:FlowCloseInventory|null;selectedKeys:string[];plan:FlowClosePreview|null;items:FlowCloseItem[];message:string|null;checking:boolean;attempted:boolean}
