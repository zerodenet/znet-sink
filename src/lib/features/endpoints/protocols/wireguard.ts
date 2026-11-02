import type { UserEndpoint } from '../library';
export type WireGuardPeer = { public_key: string; endpoint?: string; allowed_ips: string[]; pre_shared_key?: string; keepalive_secs?: number; [key: string]: unknown };
export type WireGuardProtocol = {
  type: 'wireguard'; private_key: string; addresses: string[]; mtu?: number; peers: WireGuardPeer[]; [key: string]: unknown;
};
export const createWireGuardEndpoint = (): UserEndpoint => ({ tag: '', enabled: true,
  directions: { inbound: false, outbound: true },
  protocol: { type: 'wireguard', private_key: '', addresses: [], peers: [{ public_key: '', allowed_ips: ['0.0.0.0/0', '::/0'] }] },
});
export const parseAddressList = (value: string) => value.split(/[,\s]+/).filter(Boolean);

export type EndpointFieldIssue = { field: string; message: string };
export function validWireGuardKey(value: string): boolean {
  try { return /^[A-Za-z0-9+/]{43}=$/.test(value) && atob(value).length === 32 && btoa(atob(value)) === value; }
  catch { return false; }
}
export function validIp(value: string): boolean {
  if (!value.includes(':')) return /^(0|[1-9]\d{0,2})(\.(0|[1-9]\d{0,2})){3}$/.test(value) && value.split('.').every(part => Number(part) <= 255);
  if (!/^[0-9a-f:.]+$/i.test(value)) return false;
  try { return new URL(`http://[${value}]/`).hostname.startsWith('['); } catch { return false; }
}
export function validCidr(value: string): boolean {
  const parts = value.split('/');
  return parts.length === 2 && validIp(parts[0]) && /^\d{1,3}$/.test(parts[1]) && Number(parts[1]) <= (parts[0].includes(':') ? 128 : 32);
}
const integer = (value: unknown, min: number, max: number) => typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max;
export function validPeerEndpoint(value: string): boolean {
  const match = /^(\[[0-9a-f:.]+\]|[a-z0-9_.-]+):(\d{1,5})$/i.exec(value);
  return !!match && (!match[1].startsWith('[') || validIp(match[1].slice(1, -1))) && integer(Number(match[2]), 1, 65535);
}
export function validateWireGuardEndpoint(endpoint: UserEndpoint): EndpointFieldIssue[] {
  const p = endpoint.protocol as WireGuardProtocol;
  const issues: EndpointFieldIssue[] = [];
  const directions = endpoint.directions ?? { inbound: false, outbound: true };
  const check = (valid: boolean, field: string, message: string) => { if (!valid) issues.push({ field, message }); };
  check(validWireGuardKey(p.private_key), 'private_key', '请填写配置中 PrivateKey 的 Base64 密钥（32 字节）。');
  check(p.addresses.length > 0 && p.addresses.every(validCidr), 'addresses', '请填写配置中 Address 的 IP/前缀，例如 10.66.0.2/32；这里不能填写密钥。');
  const minMtu = p.addresses.some(address => address.includes(':')) ? 1280 : 68;
  check(p.mtu === undefined || integer(p.mtu, minMtu, 65488), 'mtu', `MTU 必须为 ${minMtu}～65488 的整数。`);
  check(directions.inbound || directions.outbound, 'directions', '请至少启用入站或出站中的一个方向。');
  const listen = endpoint.listen as { address?: string; port?: number } | undefined;
  if (directions.inbound || listen) {
    check(!!listen && typeof listen.address === 'string' && validIp(listen.address), 'listen_address', '请填写监听 IP，例如 0.0.0.0 或 ::。');
    check(!!listen && integer(listen.port, 0, 65535), 'listen_port', '监听端口必须为 0～65535 的整数，0 表示自动分配。');
  }
  check(p.peers.length > 0, 'peers', '请至少添加一个 Peer。');
  const keys = new Set<string>();
  p.peers.forEach((peer, i) => {
    const field = (name: string) => `peers.${i}.${name}`;
    check(validWireGuardKey(peer.public_key), field('public_key'), '请填写对端 PublicKey 的 Base64 公钥（32 字节）。');
    check(!keys.has(peer.public_key), field('public_key'), 'Peer 公钥不能重复。'); keys.add(peer.public_key);
    check(peer.allowed_ips.length > 0 && peer.allowed_ips.every(validCidr), field('allowed_ips'), '请填写 AllowedIPs 的 IP/前缀，例如 0.0.0.0/0, ::/0。');
    check(!peer.endpoint || validPeerEndpoint(peer.endpoint), field('endpoint'), '远端地址格式为主机:端口，IPv6 使用 [地址]:端口。');
    check(!directions.outbound || !!listen || !!peer.endpoint, field('endpoint'), '仅启用出站时，请填写此 Peer 的远端地址（Endpoint）。');
    check(peer.pre_shared_key === undefined || validWireGuardKey(peer.pre_shared_key), field('pre_shared_key'), '预共享密钥必须为 Base64 编码的 32 字节密钥。');
    check(peer.keepalive_secs === undefined || integer(peer.keepalive_secs, 0, 65535), field('keepalive_secs'), '保活间隔必须为 0～65535 秒，0 表示关闭。');
  });
  return issues;
}
