import {invoke} from '@tauri-apps/api/core';
import type {NativeDocumentReceipt,DocumentDelivery} from '$lib/services/native-documents';
export interface DiagnosticPreview {
 planId:string;createdAtUnixMs:string;expiresAtUnixMs:string;bytes:number;
 scope:{profileId:string|null;configRevision:string;clientInstanceId:string};
 logCount:number;frameCount:number|null;taskCount:number;
 omissions:{category:string;count:number|null;reason:string}[];
}
export interface CaptureStatus {
 detailed:boolean;expiresAtMs:number;sessionId:string|null;startedAtMs:number;endedAtMs:number|null;
 durationSeconds:number;maximumBytes:number;storedBytes:number;storedFrames:number;queueDropsObserved:number;
 state:'idle'|'capturing'|'stopped'|'interrupted';endReason:'duration'|'capacity'|'user'|'cleanup'|'storage_failure'|'scope_changed'|'process_restart'|'journal_failure'|null;
 recoveryState:'confirmed'|'unavailable'|'result_unknown';recoveryReason:string|null;recovered:boolean;recoveryHistory:CaptureRecoveryRecord[];
 scope:unknown;
}
export interface CaptureRecoveryRecord {
 sessionId:string;scope:unknown;phase:'capturing'|'stopped'|'interrupted';confirmation:'confirmed'|'result_unknown';
 startedAtMs:number;expiresAtMs:number;endedAtMs:number|null;durationSeconds:number;maximumBytes:number;storedBytes:number;storedFrames:number;queueDropsObserved:number;
 endReason:CaptureStatus['endReason'];
}
const stoppedReasons={duration:'已达到时长预算',capacity:'已达到容量预算',user:'用户已停止',cleanup:'清理前已停止',storage_failure:'存储失败，捕获已停止',scope_changed:'所属配置或内核实例变化，捕获已失效',process_restart:'进程重建，旧捕获已中断，最终结果未确认',journal_failure:'恢复记录存盘未确认，详细捕获已停止'} as const;
export const captureHistoryReason=(record:CaptureRecoveryRecord):string=> {
 if(record.phase==='capturing')return '最后持久检查点为执行中，不能据此恢复捕获或确认最终结果';
 if(record.confirmation==='result_unknown'||record.phase==='interrupted')return '捕获已中断，最终结果未确认；不会恢复或重放';
 return record.endReason ? stoppedReasons[record.endReason] : '停止原因未提供';
};
export const captureReason=(status:CaptureStatus):string=> {
 if(status.state==='idle')return '尚未捕获';
 if(status.detailed)return '正在捕获真实 IPC 诊断记录';
 if(status.state==='interrupted')return status.endReason ? stoppedReasons[status.endReason] : '捕获已中断，最终结果未确认';
 return status.endReason ? stoppedReasons[status.endReason] : '已停止（停止原因未提供）';
};
export function validateCaptureBudget(duration:string|number,capacity:string|number):{durationSeconds:number;capacityMiB:number}|null{
 if(!/^\d+$/.test(String(duration).trim())||!/^\d+$/.test(String(capacity).trim()))return null;
 const durationSeconds=Number(duration),capacityMiB=Number(capacity);
 return Number.isSafeInteger(durationSeconds)&&durationSeconds>=1&&durationSeconds<=300&&Number.isSafeInteger(capacityMiB)&&capacityMiB>=1&&capacityMiB<=10 ? {durationSeconds,capacityMiB}:null;
}
export function previewNativeDiagnostics():Promise<DiagnosticPreview>{return invoke('native_document_preview_diagnostics');}
export function deliverNativeDiagnosticSnapshot(planId:string,delivery:DocumentDelivery):Promise<NativeDocumentReceipt>{return invoke('native_document_export_diagnostics',{planId,delivery});}
export function readNativeCapture():Promise<CaptureStatus>{return invoke('native_diagnostic_capture');}
export function startNativeCapture(durationSeconds:number,capacityMiB:number):Promise<CaptureStatus>{return invoke('native_diagnostic_capture',{request:{action:'start',durationSeconds,capacityMiB}});}
export function stopNativeCapture(expectedSessionId:string):Promise<CaptureStatus>{return invoke('native_diagnostic_capture',{request:{action:'stop',expectedSessionId}});}
