export interface RouteRow { id:string; enabled:boolean; ruleJson:string }
export interface RouteTarget { kind:'direct'|'reject'|'route'; tag:string|null; label:string }
export interface RouteSnapshot { profileId:string; profileName:string; binding:string|null; readOnlyReason:string|null; staleReason:string|null; hasLocalOverlay:boolean; rows:RouteRow[]; targets:RouteTarget[]; sourceUpdatedAtUnixMs:string; validationScope:string; pendingApply:boolean }
export type RouteEditMode='replace'|'restore_source';
export interface RoutePlan { planId:string; profileId:string; rows:RouteRow[]; mode:RouteEditMode; expiresAtUnixMs:string; state:'prepared'; notice:string }
export interface RouteStatus { planId:string; profileId:string; state:'saved_pending_apply'|'submitted_unknown'|'rejected'|'prepared'; reason:string; snapshot:RouteSnapshot|null }
export interface RoutePorts { snapshot(profileId:string,fromSource?:boolean):Promise<RouteSnapshot>; preview(input:{profileId:string;binding:string;rows:RouteRow[];mode:RouteEditMode}):Promise<RoutePlan>; save(planId:string):Promise<RouteStatus>; status(planId:string):Promise<RouteStatus>; discard(planId:string):Promise<unknown> }
export interface RouteView { phase:'idle'|'loading'|'editing'|'previewing'|'confirming'|'saving'|'saved'|'unknown'|'error'; snapshot:RouteSnapshot|null; rows:RouteRow[]; originalRows:RouteRow[]; dirty:boolean; conflict:boolean; plan:RoutePlan|null; message:string|null; checking:boolean }
