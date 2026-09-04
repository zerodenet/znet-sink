import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import {
  createDefaultDnsConfig,
  createDnsServer,
  projectDnsSettings,
  readDnsSettings,
  setDnsMode,
} from '../src/lib/services/dns-config.ts';
import { projectClientKernelFeatures } from '../src/lib/services/kernel-capabilities.ts';

const outputDirectory = process.argv[2];
if (!outputDirectory) {
  throw new Error('usage: generate-dns-contract-fixtures.mjs <output-directory>');
}
mkdirSync(outputDirectory, { recursive: true });

const features = projectClientKernelFeatures({
  available: true,
  features: [
    'tun_dual_stack',
    'tun_dns_hijack',
    'tun_dns_system_auto',
    'dns_split_dispatch',
    'dns_address_family_policy',
    'dns_fake_ip_dual_stack',
    'dns_real_reverse_mapping',
  ],
  buildFeatures: [],
  contracts: { capabilities: { minimumSupported: 1, current: 1 } },
});

function baseConfig(runtime = {}) {
  return {
    runtime,
    inbounds: [],
    outbounds: [{ tag: 'direct', protocol: { type: 'direct' } }],
    mode: { type: 'rule' },
    route: { rules: [], final: { type: 'direct' } },
  };
}

function tun(dnsHijack) {
  return {
    name: 'ZeroTun',
    addr: '10.66.0.1/24',
    secondary_addr: 'fd66::1/64',
    tag: 'tun-in',
    auto_route: true,
    dual_stack: true,
    strict_route: true,
    dns_hijack: dnsHijack,
  };
}

function write(name, config) {
  const fileName = `${name}.json`;
  writeFileSync(path.join(outputDirectory, fileName), `${JSON.stringify(config, null, 2)}\n`);
  return fileName;
}

const fake = createDefaultDnsConfig('fake_ip', {
  features,
  addressFamily: 'prefer_ipv4',
});
const source = { enabled: true, dnsHijack: true, config: fake };
const loaded = readDnsSettings(source, false);
const disabled = projectDnsSettings(source, setDnsMode(loaded, 'disabled', { features }));
const real = projectDnsSettings(source, setDnsMode(loaded, 'real', {
  features,
  addressFamily: 'prefer_ipv4',
}));
const restoredFake = projectDnsSettings(real, setDnsMode(readDnsSettings(real, false), 'fake_ip', {
  features,
  addressFamily: 'prefer_ipv4',
}));

if (disabled.enabled || disabled.dnsHijack) {
  throw new Error('disabled DNS projection must not enable DNS or TUN hijack');
}
if (real.config?.answer.type !== 'real') {
  throw new Error('real DNS projection did not produce a real answer');
}
if (restoredFake.config?.answer.type !== 'fake_ip'
  || restoredFake.config.answer.ipv6_cidr !== 'fd00::/96') {
  throw new Error('capability-driven Fake-IP projection lost FakeIPv6');
}

const doh = createDefaultDnsConfig('real', { features, addressFamily: 'prefer_ipv4' });
doh.servers = { doh: createDnsServer('doh') };
doh.default_server = 'doh';

const doq = createDefaultDnsConfig('real', { features, addressFamily: 'prefer_ipv4' });
doq.servers = {
  doq: {
    ...createDnsServer('doq'),
    host: '1.1.1.1',
    port: 853,
    server_name: 'cloudflare-dns.com',
  },
};
doq.default_server = 'doq';

const valid = [
  write('disabled-no-tun', baseConfig({})),
  write('real-system-no-tun', baseConfig({ dns: real.config })),
  write('real-doh-no-tun', baseConfig({ dns: doh })),
  write('real-doq-no-tun', baseConfig({ dns: doq })),
  write('fake-ip-no-tun', baseConfig({ dns: restoredFake.config })),
  write('fake-ip-dual-stack-tun', baseConfig({
    tun: tun(true),
    dns: restoredFake.config,
  })),
];

const conflicting = structuredClone(baseConfig({
  tun: tun(true),
  dns: restoredFake.config,
}));
conflicting.runtime.tun.addr = '198.18.0.1/24';
const invalid = [
  write('invalid-fake-ip-tun-overlap', conflicting),
];

writeFileSync(
  path.join(outputDirectory, 'manifest.json'),
  `${JSON.stringify({ valid, invalid }, null, 2)}\n`,
);
console.log(JSON.stringify({ valid, invalid }));
