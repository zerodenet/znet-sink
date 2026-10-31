import type { Component } from 'svelte';
import type { UserEndpoint } from '$lib/features/endpoints/library';
import { createWireGuardEndpoint } from '$lib/features/endpoints/protocols/wireguard';
import WireGuardForm from './WireGuardForm.svelte';

// Presentation adapters only. Available protocol choices come from Rust.
export const endpointForms: Record<string, {
  component: Component<{ endpoint: UserEndpoint; disabled?: boolean }>;
  create: () => UserEndpoint;
}> = {
  wireguard: { component: WireGuardForm, create: createWireGuardEndpoint },
};
