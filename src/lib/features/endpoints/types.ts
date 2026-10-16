import type { ClientScope } from '$lib/types/gui-api';
import type { GuiZeroCapabilities } from '$lib/types/gui-api';

// Public Zero V1 payloads retain their wire names; endpoint IDs are opaque.
export interface EndpointDirections { inbound: boolean; outbound: boolean }
export interface NetworkEndpoint {
  endpoint_id: string;
  tag: string;
  protocol: string;
  configuration?: { origin: string };
  inbound_tags: string[];
  outbound_tags: string[];
  supported: {
    directions: EndpointDirections;
    operations: string[];
    operation_capabilities?: Record<string, { live_direction_contraction?: EndpointDirections; preconditions?: string[]; persistence?: string[] }>;
    packet: boolean;
    stream: boolean;
    datagram: boolean;
    derived_stream: boolean;
    derived_datagram: boolean;
  };
  enabled: boolean;
  allowed: EndpointDirections;
  effective: EndpointDirections;
  state: 'stopped' | 'starting' | 'running' | 'stopping' | 'failed';
  health: 'unknown' | 'healthy' | 'degraded';
  state_source: 'config' | 'runtime_override';
  core_instance_id: string;
  config_revision: number;
  intent_revision: number;
  generation: number | null;
  observed_at_unix_ms: number;
  started_at_unix_ms: number | null;
  counters: Record<string, number | null>;
  last_error: { code?: string; message: string; cause?: string } | null;
}
export interface EndpointCatalog {
  capabilities: GuiZeroCapabilities; endpoints: NetworkEndpoint[];
  profileId: string | null; editableEndpointIds: string[]; localOverrideIds: string[];
  configuredAddresses?: Record<string, string[]>;
  restorationTargets?: RestorationTarget[];
  restorationUnavailableReason?: string | null;
}
export interface EndpointDetails {
  endpoint_id: string; generation: number | null;
  schema_id: string; schema_version: number; details: unknown;
}
export type EndpointAction =
  | { operation: 'set_state'; enabled: boolean }
  | { operation: 'set_directions'; directions: EndpointDirections }
  | { operation: 'restart' }
  | { operation: 'clear_overrides' };
export interface EndpointControl {
  profileId: string; configRevision: number; endpointId: string; coreInstanceId: string;
  expectedIntentRevision: number; action: EndpointAction;
}
export interface EndpointGateway {
  catalog(): Promise<EndpointCatalog>;
  control(input: EndpointControl): Promise<NetworkEndpoint>;
  previewRestoration?(bindingId: string): Promise<RestorationPlan>;
  applyRestoration?(planId: string, mode: RestoreMode): Promise<RestorationReceipt>;
  reconcileRestoration?(operationId: string): Promise<RestorationReceipt>;
  restorationReceipts?(): Promise<RestorationReceipt[]>;
  details(endpointId: string, coreInstanceId: string): Promise<EndpointDetails>;
}

export interface RestorationTarget { bindingId:string; endpointId:string; profileId:string; tag:string; protocol:string }
export interface RestorationPlan { planId:string; target:RestorationTarget; sourceDigest:string; sourceUpdatedAtUnixMs:string; previousEffectiveDigest:string; effectiveDigest:string; removedFields:string[]; runtimeAvailable:boolean; impact:string; clientScope:ClientScope; createdAtUnixMs:string; expiresAtUnixMs:string }
export type RestoreMode='save_pending'|'apply_runtime';
export type RestorePhase='prepared'|'saved_pending'|'applying'|'applied'|'clearing'|'runtime_confirmed'|'confirmed'|'result_unknown'|'rejected';
export interface RestorationReceipt { operationId:string; target:RestorationTarget; phase:RestorePhase; localSaved:boolean; runtimeConfirmed:boolean; applyIdentity:{coreInstanceId:string;configRevision:string}|null; endpoint:Partial<NetworkEndpoint>|null; message:string }
