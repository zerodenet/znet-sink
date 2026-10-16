export const networkKeys=['tun.mtu','runtime.udpUpstreamIdleTimeoutSeconds','localProxy.host','localProxy.port'] as const;
export type NetworkKey=typeof networkKeys[number];
export interface NetworkListener {tag:string;protocol:'mixed'|'http'|'socks5';host:string;port:number;exposure:'loopback'|'all-interfaces'|'specific-address'}
export interface NetworkValues {mtu:number;mtuOrigin:'client'|'source-tun'|'source-network'|'default';udpIdleSeconds:string;udpOrigin:'client'|'source'|'default';listener:NetworkListener|null;listenerReason:string|null}
export interface NetworkSnapshot {profileId:string|null;profileName:string|null;sourceUpdatedAtUnixMs:string|null;binding:string|null;readOnlyReason:string|null;values:NetworkValues|null;editedFields:NetworkKey[];dialLeaves:{tag:string;addressFamily:'auto'|'only_ipv4'|'only_ipv6';interface:string|null;sourceIp:string|null}[];pendingApply:boolean;notice:string}
export interface NetworkInput {profileId:string;binding:string;changes:Partial<Record<NetworkKey,number|string>>;reset:NetworkKey[]}
export interface NetworkPlan {planId:string;profileId:string;before:NetworkValues;after:NetworkValues;changedFields:NetworkKey[];resetFields:NetworkKey[];lanExposureWarning:boolean;expiresAtUnixMs:string;state:'prepared';notice:string}
export interface NetworkStatus {planId:string;profileId:string;state:'prepared'|'rejected'|'submitted_unknown'|'saved_pending_apply';reason:string;snapshot:NetworkSnapshot|null}
export interface NetworkPorts {snapshot():Promise<NetworkSnapshot>;preview(input:NetworkInput):Promise<NetworkPlan>;save(planId:string):Promise<NetworkStatus>;status(planId:string):Promise<NetworkStatus>;discard(planId:string):Promise<unknown>}
export interface NetworkDraft {mtu:string;udpIdleSeconds:string;host:string;port:string}
export interface NetworkView {phase:'idle'|'loading'|'editing'|'previewing'|'confirming'|'saving'|'unknown'|'saved'|'error';snapshot:NetworkSnapshot|null;draft:NetworkDraft;baseline:NetworkDraft;plan:NetworkPlan|null;receipt:NetworkStatus|null;message:string|null;checking:boolean;submitting:boolean;stale:boolean}
