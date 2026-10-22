import { invoke } from '@tauri-apps/api/core';

export type DocumentDelivery = 'save' | 'share';
export type DocumentStatus = 'read' | 'saved' | 'share-accepted' | 'cancelled' | 'failed' | 'result-unknown';
export interface NativeDocumentReceipt {
  operationId: string;
  status: DocumentStatus;
  content: string | null;
  bytesRead: number | null;
  reason: string | null;
}
export interface NativeDocumentCapabilities {
  platform: 'android' | 'ios' | 'desktop';
  available: boolean;
  minimumAndroidApi: number;
  adaptationState: string;
  reason: string | null;
  importMaximumBytes: number;
  settingsMaximumBytes: number;
}
export function getNativeDocumentCapabilities(): Promise<NativeDocumentCapabilities> {
  return invoke('native_document_capabilities');
}
export function pickNativeDocument(purpose: 'settings' | 'configuration'): Promise<NativeDocumentReceipt> {
  return invoke('native_document_pick', { purpose });
}
export function exportNativeSettings(delivery: DocumentDelivery, expectedRevision: string): Promise<NativeDocumentReceipt> {
  return invoke('native_document_export_settings', { delivery, expectedRevision });
}
export function exportNativeProfile(id: string, expectedUpdatedAt: number, delivery: DocumentDelivery, credentialsAcknowledged: boolean): Promise<NativeDocumentReceipt> {
  return invoke('native_document_export_profile', { id, expectedUpdatedAt, delivery, credentialsAcknowledged });
}
export function exportNativeDiagnostics(delivery: DocumentDelivery): Promise<NativeDocumentReceipt> {
  return invoke('native_document_export_diagnostics', { delivery });
}
export function documentReceiptMessage(receipt: NativeDocumentReceipt): string {
  switch (receipt.status) {
    case 'read': return '文件已读取，请校验后保存';
    case 'saved': return '系统文件写入已确认';
    case 'share-accepted': return '分享面板已接受；接收方保存结果未确认';
    case 'cancelled': return '已取消，原有草稿和数据保留';
    case 'result-unknown': return '系统操作结果未确认，请先检查目标；不会自动重试';
    case 'failed': return receipt.reason ?? '系统文件操作失败，原有数据保留';
  }
}
