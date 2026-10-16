// Legacy copies are metadata-only. They cannot establish host provenance from
// UI-supplied fields/markers. Logs use the host-prepared v3 plan for safe text.
import type {DebugFrame} from '$lib/types/debug';
import type {LogEntry,LogLevel,LogSource} from '$lib/types/logs';
export interface LogCopyContext {source:LogSource|'all';level:LogLevel|'all';search?:string;hasMore:boolean;copiedAtUnixMs?:number;}
export interface DebugFrameCopyContext {frameType:string;hasMore:boolean;copiedAtUnixMs?:number;}
const unsigned=(value:unknown)=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0?String(value):null;
const source=(value:unknown)=>typeof value==='string'&&['app','core','plugin'].includes(value)?value:null;
const level=(value:unknown)=>typeof value==='string'&&['trace','debug','info','warn','error'].includes(value)?value:null;
export function logCopyRecord(log:LogEntry):Record<string,unknown>{return {id:unsigned(log.id),occurredAtUnixMs:unsigned(log.occurredAtUnixMs),source:source(log.source),level:level(log.level),messageOmitted:true,rawFieldsOmitted:true,omittedReason:'metadata_only_without_host_prepared_classification'};}
export const serializeLogForClipboard=(log:LogEntry):string=>JSON.stringify(logCopyRecord(log),null,2);
export function serializeLogsForClipboard(logs:LogEntry[],context:LogCopyContext):string{return JSON.stringify({schemaId:'znet.clipboard.logs.v2',copiedAtUnixMs:unsigned(context.copiedAtUnixMs??Date.now()),source:source(context.source),level:level(context.level),searchOmitted:true,count:logs.length,partial:context.hasMore===true,items:logs.map(logCopyRecord)},null,2);}
export function debugFrameCopyRecord(frame:DebugFrame):Record<string,unknown>{return {id:unsigned(frame.id),atMs:unsigned(frame.atMs),direction:['rx','tx'].includes(frame.direction)?frame.direction:null,elapsedMs:unsigned(frame.elapsedMs),payloadOmitted:true,errorOmitted:true,omittedReason:'unclassified_frame_body_not_public_telemetry'};}
export const serializeDebugFrameForClipboard=(frame:DebugFrame):string=>JSON.stringify(debugFrameCopyRecord(frame),null,2);
export function serializeDebugFramesForClipboard(frames:DebugFrame[],context:DebugFrameCopyContext):string{return JSON.stringify({schemaId:'znet.clipboard.ipc-debug.v2',copiedAtUnixMs:unsigned(context.copiedAtUnixMs??Date.now()),filterApplied:true,count:frames.length,partial:context.hasMore===true,items:frames.map(debugFrameCopyRecord)},null,2);}
