import {invoke} from '@tauri-apps/api/core';
import type {DnsMigrationPorts,DnsMigrationSnapshot,DnsMigrationPlan,DnsMigrationStatus} from './types';
export const dnsMigrationPorts:DnsMigrationPorts={
 snapshot:profileId=>invoke<DnsMigrationSnapshot>('gui_dns_migration_snapshot',{profileId}),
 preview:input=>invoke<DnsMigrationPlan>('gui_dns_migration_preview',{input}),
 save:planId=>invoke<DnsMigrationStatus>('gui_dns_migration_save',{planId}),
 status:planId=>invoke<DnsMigrationStatus>('gui_dns_migration_status',{planId}),
 discard:planId=>invoke('gui_dns_migration_discard',{planId}),
};
