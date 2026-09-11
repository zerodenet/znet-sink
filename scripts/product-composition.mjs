import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
export const products = JSON.parse(readFileSync(new URL('../products/desktop.json', import.meta.url), 'utf8'));
export const tools = [
  { feature: 'tool-dns', id: 'dns', title: 'DNS 与 Fake-IP', directory: 'dns', panel: true, files: ['Panel.svelte', 'state.svelte.ts', 'client.ts'], style: '.dns-attempts', commands: ['gui_dns_lookup', 'gui_dns_cache', 'gui_fakeip_lookup', 'gui_clear_fake_ip'] },
  { feature: 'tool-route', id: 'route-trace', title: '路由追踪', directory: 'routing', panel: true, files: ['Panel.svelte', 'state.svelte.ts', 'client.ts'], style: '.hop-table', commands: ['gui_trace_route'] },
  { feature: 'tool-node-probe', id: 'probe-jobs', title: '节点测速任务', directory: 'node-probes', panel: false, files: ['jobs.svelte.ts', 'client.ts'], commands: ['gui_probe_job_start'] },
];
export function resolveProduct(name = process.env.ZNET_PRODUCT || 'desktop') {
  if (!Object.hasOwn(products, name)) throw new Error(`Unknown ZNET_PRODUCT: ${name}`);
  const features = products[name];
  for (const feature of features) if (!tools.some(tool => tool.feature === feature)) throw new Error(`Unknown feature: ${feature}`);
  return {name, features, tools: tools.filter(tool => features.includes(tool.feature))};
}
/** @returns {import('vite').Plugin} */
export function productComposition() {
  const product = resolveProduct();
  const metadata = 'virtual:znet-product-metadata';
  const panels = 'virtual:znet-product';
  const probes = 'virtual:znet-node-probes';
  const diagnostics = 'virtual:znet-tool-diagnostics';
  const panelTools = product.tools.filter(tool => tool.panel);
  return {
    name: 'znet-product-composition',
    resolveId(id) { if ([metadata, panels, probes, diagnostics].includes(id)) return '\0' + id; },
    load(id) {
      if (id === '\0' + metadata) return `export const productName = ${JSON.stringify(product.name)}; export const diagnosticCatalog = ${JSON.stringify(panelTools.map(({id, title}) => ({id, title})))}; export const moduleCatalog = ${JSON.stringify(product.tools.map(({id, title}) => ({id, title})))};`;
      if (id === '\0' + probes) return product.features.includes('tool-node-probe') ? `import { ProbeJobsState } from '$lib/features/node-probes/jobs.svelte'; import { startProbeJob, cancelProbeJob } from '$lib/features/node-probes/client'; export const createProbeJobs = (query) => new ProbeJobsState(query, startProbeJob, cancelProbeJob);` : 'export const createProbeJobs = null;';
      if (id === '\0' + diagnostics) return product.features.includes('tool-node-probe') ? `export { probeDiagnostics as toolDiagnostics } from '$lib/features/node-probes/diagnostics';` : 'export const toolDiagnostics = [];';
      if (id === '\0' + panels) return panelTools.map((tool, i) => `import Panel${i} from '$lib/features/${tool.directory}/Panel.svelte';`).join('\n') + `\nexport const diagnosticTools = [${panelTools.map((tool, i) => `{id: ${JSON.stringify(tool.id)}, panel: Panel${i}}`).join(',')}];`;
    },
    generateBundle(_options, bundle) {
      const modules = [...new Set(Object.values(bundle).flatMap(item => item.type === 'chunk' ? Object.keys(item.modules) : []))].map(id => id.replaceAll('\\', '/').replace(fileURLToPath(new URL('../', import.meta.url)).replaceAll('\\', '/'), '/'));
      for (const tool of tools) {
        const included = modules.some(id => id.includes(`/features/${tool.directory}/Panel.svelte`));
        const ownerIncluded = modules.some(id => id.includes(`/features/${tool.directory}/state.svelte.ts`));
        const selected = product.features.includes(tool.feature);
        // SvelteKit also builds a server shell without client-only lazy pages.
        if (!selected && modules.some(id => id.includes(`/features/${tool.directory}/`))) this.error(`Trimmed tool ${tool.id} remains in module graph`);
        if (included && !ownerIncluded) this.error(`Tool ${tool.id} lost its owner`);
      }
      this.emitFile({type: 'asset', fileName: 'product-composition.json', source: JSON.stringify({schema: 'znet.product.v1', product: product.name, features: product.features, modules}, null, 2)});
    },
  };
}
