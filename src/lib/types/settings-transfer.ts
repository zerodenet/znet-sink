import type { ClientScope } from '$lib/types/gui-api';
export interface SettingsTransferExport { document: string; omittedFields: string[]; expectedRevision: string }
export interface SettingsTransferPreview {
  planId: string; expectedRevision: string; targetRevision: string; scope: ClientScope;
  sourceRevision?: string | null; changedFields: string[]; unchangedFields: string[]; expiresAtUnixMs: string;
  runtimeRequired: boolean; compatibilityPending: boolean; notice: string;
}
export interface SettingsTransferReceipt {
  planId: string; localState: 'saved' | 'not-committed' | 'unknown';
  runtimeState: 'pending-apply' | 'not-required' | 'unknown'; savedRevision: string; theme: string; notice: string;
}
export interface SettingsTransferGateway {
  export(): Promise<SettingsTransferExport>;
  preview(content: string): Promise<SettingsTransferPreview>;
  commit(planId: string): Promise<SettingsTransferReceipt>;
  status(planId: string): Promise<SettingsTransferReceipt>;
}
