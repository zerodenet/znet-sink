import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseWireGuardConfig } from '../src/lib/features/endpoints/protocols/wireguard-import.ts';
import { readWireGuardFile } from '../src/lib/features/endpoints/protocols/wireguard-file.ts';
import { validCidr, validateWireGuardEndpoint } from '../src/lib/features/endpoints/protocols/wireguard.ts';

const privateKey = Buffer.alloc(32, 1).toString('base64');
const publicKey = Buffer.alloc(32, 2).toString('base64');
const psk = Buffer.alloc(32, 3).toString('base64');
const conf = `\uFEFF[Interface]\r
PrivateKey = ${privateKey}\r
Address = 10.66.0.2/32, fd66::2/128 # assigned addresses\r
DNS = 1.1.1.1\r
MTU = 1380\r
[Peer]\r
PublicKey = ${publicKey}\r
PresharedKey = ${psk}\r
Endpoint = [2001:db8::1]:51820\r
AllowedIPs = 0.0.0.0/0, ::/0\r
PersistentKeepalive = 25\r
`;

test('wg-quick import retains connection parameters, all peers and repeated lists', () => {
  const { endpoint, warnings } = parseWireGuardConfig(conf.replace('MTU = 1380', 'MTU = 1380\nAddress = 10.67.0.2') + `
[Peer]
PublicKey = ${Buffer.alloc(32, 4).toString('base64')}
Endpoint = vpn.example:51820
AllowedIPs = 10.68.0.0/16
AllowedIPs = fd68::/64
PersistentKeepalive = off`, 'home.conf');
  assert.equal(endpoint.tag, 'home');
  assert.deepEqual(endpoint.directions, { inbound: false, outbound: true });
  assert.deepEqual(endpoint.protocol.addresses, ['10.66.0.2/32', 'fd66::2/128', '10.67.0.2/32']);
  assert.equal(endpoint.protocol.private_key, privateKey);
  assert.equal(endpoint.protocol.mtu, 1380);
  assert.equal(endpoint.protocol.peers.length, 2);
  assert.equal(endpoint.protocol.peers[0].pre_shared_key, psk);
  assert.equal(endpoint.protocol.peers[0].keepalive_secs, 25);
  assert.deepEqual(endpoint.protocol.peers[1].allowed_ips, ['10.68.0.0/16', 'fd68::/64']);
  assert.equal(endpoint.protocol.peers[1].keepalive_secs, 0);
  assert.match(warnings.join(), /DNS 未应用/);
  assert.equal('dns' in endpoint.protocol, false);
});

test('listen port is retained as the actual endpoint listener and allows roaming peers', () => {
  const { endpoint } = parseWireGuardConfig(conf.replace('MTU = 1380', 'ListenPort = 51820').replace('Endpoint = [2001:db8::1]:51820', ''), 'server.conf');
  assert.deepEqual(endpoint.listen, { address: '0.0.0.0', port: 51820 });
  assert.deepEqual(endpoint.directions, { inbound: true, outbound: true });
  assert.deepEqual(validateWireGuardEndpoint(endpoint), []);
});

test('bad addresses and keys fail with actionable diagnostics without echoing secrets', () => {
  for (const text of [conf.replace('10.66.0.2/32, fd66::2/128', privateKey), conf.replace(privateKey, 'SECRET'), conf.replace(publicKey, 'SECRET')]) {
    assert.throws(() => parseWireGuardConfig(text, 'home.conf'), error => !error.message.includes(privateKey) && !error.message.includes('SECRET'));
  }
  for (const ip of ['10.0.0.1/33', '256.0.0.1/24', '01.2.3.4/32', 'fd::g/64', '::/129', privateKey]) assert.equal(validCidr(ip), false);
  for (const ip of ['10.0.0.1/32', '::/0', '::ffff:192.0.2.1/128']) assert.equal(validCidr(ip), true);
});

test('unsupported scripts and routing, malformed sections, duplicate fields and invalid numbers are rejected', () => {
  for (const text of [conf.replace('MTU = 1380', 'PostUp = SECRET'), conf.replace('MTU = 1380', 'Table = off'), conf.replace('MTU = 1380', 'MTU = 1380\nMTU = 1400'), conf.replace('MTU = 1380', 'MTU = 0'), conf.replace('MTU = 1380', 'MTU = 65535'), conf.replace('25', '-1'), conf.replace('[Peer]', '[Private]'), conf.replace('[Peer]', '[Interface]'), conf.replace('PrivateKey =', 'InvalidKey =')]) {
    assert.throws(() => parseWireGuardConfig(text, 'home.conf'), error => !error.message.includes('SECRET'));
  }
  assert.throws(() => parseWireGuardConfig(conf.replace('Endpoint = [2001:db8::1]:51820', ''), 'home.conf'), /远端地址/);
  assert.throws(() => parseWireGuardConfig('', 'empty.conf'), /未找到/);
});

test('file reads bound size and type and do not expose native error contents', async () => {
  assert.equal((await readWireGuardFile(new File([conf], 'home.conf'))).endpoint.tag, 'home');
  assert.match((await readWireGuardFile(new File([conf], 'home.json'))).error, /conf/);
  assert.match((await readWireGuardFile(new File(['x'.repeat(256 * 1024 + 1)], 'huge.conf'))).error, /256 KiB/);
  const unreadable = { name: 'home.conf', size: 10, text: async () => { throw new Error('SECRET'); } };
  assert.equal((await readWireGuardFile(unreadable)).error, '文件读取失败，请重新选择。');
});
