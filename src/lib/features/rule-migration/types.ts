export type ReferenceKind='route_rule'|'bypass'|'dns_dispatch'|'route_overlay'|'global_dns'|'legacy_dns';
export interface RuleReference { profileId:string|null; fieldPath:string; tag:string; kind:ReferenceKind }
export interface RuleBlocker { profileId:string|null; profileName:string|null; reasonCode:string; reason:string }
export interface MigrationTarget { id:string; name:string; tag:string; unavailableReason:string|null }
export interface MigrationSnapshot {
 profileId:string; profileName:string; ruleSetId:string; ruleSetName:string|null; resourceExists:boolean;
 tag:string; binding:string|null; references:RuleReference[]; blockers:RuleBlocker[]; targets:MigrationTarget[];
 commonBindingRemoved:boolean; notice:string; pendingApply:boolean;
}
export interface MigrationPlan {
 planId:string; profileId:string; ruleSetId:string; ruleSetName:string; targetId:string|null; targetName:string|null;
 fromTag:string; toTag:string|null; references:RuleReference[]; commonBindingRemoved:boolean;
 expiresAtUnixMs:string; state:'prepared'; notice:string;
}
export interface MigrationStatus {
 planId:string; profileId:string; ruleSetId:string; state:'prepared'|'rejected'|'submitted_unknown'|'saved_pending_apply';
 reason:string; appConfigPublished:boolean; ruleSetRemoved:boolean; afterBinding:string|null;
}
export interface MigrationPorts {
 snapshot(profileId:string,ruleSetId:string):Promise<MigrationSnapshot>;
 preview(input:{profileId:string;ruleSetId:string;binding:string;targetId:string|null}):Promise<MigrationPlan>;
 save(planId:string):Promise<MigrationStatus>; status(planId:string):Promise<MigrationStatus>; discard(planId:string):Promise<unknown>;
}
export interface MigrationView {
 phase:'idle'|'loading'|'selecting'|'previewing'|'confirming'|'saving'|'unknown'|'saved'|'error';
 snapshot:MigrationSnapshot|null; sourceConfirmed:boolean; targetId:string|null; plan:MigrationPlan|null;
 receipt:MigrationStatus|null; message:string|null; checking:boolean; conflict:boolean;
}
