import type { DebugFrameQuery } from '$lib/types/debug';
import type { DocumentDelivery,NativeDocumentReceipt } from '$lib/services/native-documents';
export interface ConnectionExportPreview { planId:string; records:number; truncated:boolean; bytes:number; expiresAtUnixMs:string; filtersFingerprint:string; omissions:string[] }
export interface ConnectionExportGateway {preview(query:DebugFrameQuery):Promise<ConnectionExportPreview>;deliver(planId:string,delivery:DocumentDelivery):Promise<NativeDocumentReceipt>}
