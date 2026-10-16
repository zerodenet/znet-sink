export interface DnsReference {path:string;label:string}
export interface DnsMigrationServer {name:string;references:DnsReference[]}
export interface DnsMigrationSnapshot {profileId:string;profileName:string;binding:string|null;readOnlyReason:string|null;servers:DnsMigrationServer[];notice:string;pendingApply:true}
export interface DnsMigrationPlan {planId:string;profileId:string;serverName:string;replacementName:string|null;renameTo:string|null;references:DnsReference[];notice:string;state:'prepared';expiresAtUnixMs:string}
export interface DnsMigrationStatus {planId:string;profileId:string;state:'prepared'|'saved_pending_apply'|'submitted_unknown'|'rejected';reason:string;snapshot:DnsMigrationSnapshot|null}
export type DnsMigrationMode='delete'|'rename';
export interface DnsMigrationPorts {snapshot(profileId:string):Promise<DnsMigrationSnapshot>;preview(input:{profileId:string;binding:string;serverName:string;replacementName:string|null;renameTo:string|null}):Promise<DnsMigrationPlan>;save(planId:string):Promise<DnsMigrationStatus>;status(planId:string):Promise<DnsMigrationStatus>;discard(planId:string):Promise<unknown>}
export interface DnsMigrationView {phase:'idle'|'loading'|'choosing'|'previewing'|'confirming'|'saving'|'saved'|'unknown'|'error';mode:DnsMigrationMode;snapshot:DnsMigrationSnapshot|null;serverName:string|null;replacementName:string|null;renameTo:string|null;plan:DnsMigrationPlan|null;conflict:boolean;message:string|null;checking:boolean}
