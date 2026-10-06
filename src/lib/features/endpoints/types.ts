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
}
export interface EndpointDetails {
  endpoint_id: string; generation: number | null;
  schema_id: string; schema_version: number; details: unknown;
}
export type EndpointAction =
  | { operation: 'set_state'; enabled: boolean }
  | { operation: 'set_directions'; directions: EndpointDirections }
  | { operation: 'restart' };
export interface EndpointControl {
  profileId: string; configRevision: number; endpointId: string; coreInstanceId: string;
  expectedIntentRevision: number; action: EndpointAction;
}
export interface EndpointGateway {
  catalog(): Promise<EndpointCatalog>;
  control(input: EndpointControl): Promise<NetworkEndpoint>;
  details(endpointId: string, coreInstanceId: string): Promise<EndpointDetails>;
}
