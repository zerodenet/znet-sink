declare module 'virtual:znet-product' {
  import type { Component } from 'svelte';
  export const diagnosticTools: readonly { id: string; panel: Component }[];
}
declare module 'virtual:znet-product-metadata' {
  export const productName: string;
  export const moduleCatalog: readonly { id: string; title: string }[];
  export const diagnosticCatalog: readonly { id: string; title: string }[];
}
declare module 'virtual:znet-node-probes' {
  import type { NodeScreenSnapshot } from '$lib/types/gui-api';
  import type { ProbeJobsState } from '$lib/features/node-probes/jobs.svelte';
  export const createProbeJobs: ((query: (reason: string) => Promise<NodeScreenSnapshot>) => ProbeJobsState) | null;
}
