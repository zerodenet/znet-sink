import type {LogEntry,LogLevel,LogSource} from './logs';
import type {NativeDocumentReceipt} from '$lib/services/native-documents';
export interface LogExportRequest {
 items:{id:string;occurredAtUnixMs:string}[];
 source:LogSource|null;level:LogLevel|null;searchApplied:boolean;
 fromUnixMs:string|null;untilUnixMs:string|null;partial:boolean;
}
export interface LogExportPreview {
 planId:string;createdAtUnixMs:string;expiresAtUnixMs:string;bytes:number;count:number;
 classifiedCount:number;unclassifiedCount:number;partial:boolean;
 scope:{profileId:string|null;configRevision:string;clientInstanceId:string};
}
export interface LogExportReceipt {status:'copied'|'system-receipt';document:NativeDocumentReceipt|null;}
export function fixedLogSelection(logs:LogEntry[],source:LogSource|'all',level:LogLevel|'all',search:string,partial:boolean):LogExportRequest {
 if(!logs.length||logs.length>2000||logs.some(row=>!Number.isSafeInteger(row.id)||row.id<0||!Number.isSafeInteger(row.occurredAtUnixMs)||row.occurredAtUnixMs<0))throw new Error('当前选择须为1–2000条有效日志，请缩小范围');
 return {items:logs.map(row=>({id:String(row.id),occurredAtUnixMs:String(row.occurredAtUnixMs)})),source:source==='all'?null:source,level:level==='all'?null:level,searchApplied:!!search.trim(),fromUnixMs:null,untilUnixMs:null,partial};
}
