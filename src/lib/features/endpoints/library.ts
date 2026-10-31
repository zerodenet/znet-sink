import { invoke } from '@tauri-apps/api/core';

/** Common persisted envelope. Protocol fields belong to authoring adapters. */
export type UserEndpoint = {
  tag: string;
  enabled?: boolean;
  directions?: { inbound: boolean; outbound: boolean };
  protocol: { type: string; [key: string]: unknown };
  [key: string]: unknown;
};
/** Client authoring support supplied by Rust; runtime support is checked on apply. */
export type EndpointProtocol = { type: string; label: string };
export type UserEndpointLibrary = { revision: string; endpoints: UserEndpoint[]; protocols: EndpointProtocol[] };
export const loadUserEndpoints = () => invoke<UserEndpointLibrary>('gui_user_endpoints');
export const saveUserEndpoints = (expectedRevision: string, endpoints: UserEndpoint[]) =>
  invoke<UserEndpointLibrary>('gui_user_endpoints_save', { expectedRevision, endpoints });
