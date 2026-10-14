import type { DebugFrameQuery } from '$lib/types/debug';
import type { DocumentDelivery,NativeDocumentReceipt } from '$lib/services/native-documents';
export type ConnectionExportSource='completed-history'|'active';
export type ActiveConnectionExportFilter=Pick<DebugFrameQuery,'search'|'protocol'|'outbound'>;
export interface ConnectionExportPreview { source:ConnectionExportSource; coreInstanceId:string|null; configRevision:string|null; observedAtUnixMs:string|null; planId:string; records:number; truncated:boolean; bytes:number; expiresAtUnixMs:string; filtersFingerprint:string; omissions:string[] }
export interface ConnectionExportGateway {preview(query:DebugFrameQuery):Promise<ConnectionExportPreview>;previewActive(query:ActiveConnectionExportFilter):Promise<ConnectionExportPreview>;deliver(planId:string,delivery:DocumentDelivery):Promise<NativeDocumentReceipt>}
