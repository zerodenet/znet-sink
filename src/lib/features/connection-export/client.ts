import { invoke } from '@tauri-apps/api/core';
import type {ConnectionExportGateway} from './types';
export const connectionExportGateway:ConnectionExportGateway={preview:query=>invoke('native_connection_export_preview',{query}),previewActive:query=>invoke('native_active_connection_export_preview',{query}),deliver:(planId,delivery)=>invoke('native_connection_export_deliver',{planId,delivery})};
