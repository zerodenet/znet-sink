import {invoke} from '@tauri-apps/api/core';
import type {LogExportPreview,LogExportReceipt,LogExportRequest} from '$lib/types/log-export';
export const previewLogExport=(request:LogExportRequest):Promise<LogExportPreview>=>invoke('log_export_preview',{request});
export const deliverLogExport=(planId:string,delivery:'copy'|'save'|'share'):Promise<LogExportReceipt>=>invoke('log_export_deliver',{planId,delivery});

export const discardLogExport=(planId:string):Promise<void>=>invoke('log_export_discard',{planId});
