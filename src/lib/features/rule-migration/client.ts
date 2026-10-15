import {invoke} from '@tauri-apps/api/core';
import type {MigrationPorts,MigrationSnapshot,MigrationPlan,MigrationStatus} from './types';
export const ruleMigrationPorts:MigrationPorts={
 snapshot:(profileId,ruleSetId)=>invoke<MigrationSnapshot>('gui_rule_migration_snapshot',{profileId,ruleSetId}),
 preview:input=>invoke<MigrationPlan>('gui_rule_migration_preview',{input}),
 save:planId=>invoke<MigrationStatus>('gui_rule_migration_save',{planId}),
 status:planId=>invoke<MigrationStatus>('gui_rule_migration_status',{planId}),
 discard:planId=>invoke('gui_rule_migration_discard',{planId}),
};
