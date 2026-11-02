import type { Component } from 'svelte';
import type { UserEndpoint } from '$lib/features/endpoints/library';
import { createWireGuardEndpoint, validateWireGuardEndpoint, type EndpointFieldIssue } from '$lib/features/endpoints/protocols/wireguard';
import WireGuardForm from './WireGuardForm.svelte';
import WireGuardFileFill from './WireGuardFileFill.svelte';

// Presentation adapters only. Available protocol choices come from Rust.
export const endpointForms: Record<string, {
  component: Component<{ endpoint: UserEndpoint; disabled?: boolean; attempted?: boolean }>;
  create: () => UserEndpoint;
  validate: (endpoint: UserEndpoint) => EndpointFieldIssue[];
  importer: Component<{ endpoint: UserEndpoint; disabled: boolean; busy?: boolean; onFilled: () => void }>;
}> = {
  wireguard: { component: WireGuardForm, create: createWireGuardEndpoint, validate: validateWireGuardEndpoint, importer: WireGuardFileFill },
};
