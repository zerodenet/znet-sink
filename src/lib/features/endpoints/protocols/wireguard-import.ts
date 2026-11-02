import { createWireGuardEndpoint, parseAddressList, validIp, validateWireGuardEndpoint, type WireGuardProtocol, type WireGuardPeer } from '$lib/features/endpoints/protocols/wireguard';
import type { UserEndpoint } from '../library';

export type WireGuardImport = { endpoint: UserEndpoint; warnings: string[] };
/** Import data only. wg-quick commands and host networking settings never execute. */
export function parseWireGuardConfig(text: string, filename: string): WireGuardImport {
  const endpoint = createWireGuardEndpoint();
  endpoint.tag = filename.replace(/\.(conf|ini)$/i, '').trim().slice(0, 128);
  const protocol = endpoint.protocol as WireGuardProtocol;
  protocol.peers = [];
  const warnings = new Set<string>();
  let section = '';
  let peer: WireGuardPeer | undefined;
  let seenInterface = false;
  let seen = new Set<string>();
  function fail(line: number, message: string): never { throw new Error(`第 ${line} 行：${message}`); }
  const number = (value: string, line: number, max: number) => {
    if (!/^\d+$/.test(value) || Number(value) > max) fail(line, '数值超出允许范围。');
    return Number(value);
  };
  const addresses = (value: string) => parseAddressList(value).map(ip => ip.includes('/') || !validIp(ip) ? ip : `${ip}/${ip.includes(':') ? 128 : 32}`);
  for (const [index, raw] of text.replace(/^\uFEFF/, '').split(/\r?\n/).entries()) {
    const line = raw.replace(/#.*/, '').trim();
    if (!line) continue;
    if (line.startsWith('[')) {
      if (/^\[Interface\]$/i.test(line)) {
        if (seenInterface) fail(index + 1, '只能有一个 [Interface]。');
        seenInterface = true; section = 'interface'; peer = undefined;
      } else if (/^\[Peer\]$/i.test(line)) {
        if (!seenInterface) fail(index + 1, '请先提供 [Interface]。');
        section = 'peer'; peer = { public_key: '', allowed_ips: [] }; protocol.peers.push(peer);
      } else fail(index + 1, '只支持 [Interface] 和 [Peer] 配置段。');
      seen = new Set(); continue;
    }
    const match = /^([A-Za-z][A-Za-z0-9]*)\s*=\s*(.*)$/.exec(line);
    if (!match || !section) fail(index + 1, '请使用 [Interface]/[Peer] 下的 字段 = 值 格式。');
    const key = match[1].toLowerCase(); const value = match[2].trim();
    const repeated = section === 'interface' ? ['address', 'dns'] : ['allowedips'];
    if (seen.has(key) && !repeated.includes(key)) fail(index + 1, '同一配置段中存在重复字段。');
    seen.add(key);
    if (section === 'interface') {
      switch (key) {
        case 'privatekey': protocol.private_key = value; break;
        case 'address': protocol.addresses.push(...addresses(value)); break;
        case 'mtu': protocol.mtu = number(value, index + 1, 65488); break;
        case 'listenport':
          endpoint.listen = { address: '0.0.0.0', port: number(value, index + 1, 65535) };
          endpoint.directions = { inbound: true, outbound: true }; break;
        case 'dns': {
          endpoint.dns ??= { servers: [], search_domains: [], use_as_default: true };
          for (const item of parseAddressList(value)) {
            if (validIp(item)) { if (!endpoint.dns.servers.includes(item)) endpoint.dns.servers.push(item); }
            else {
              const name = item.replace(/\.$/, '').toLowerCase();
              if (name.length > 253 || !name.split('.').every(label => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label))) fail(index + 1, 'DNS 必须填写服务器 IP 或有效搜索域。');
              if (!endpoint.dns.search_domains.includes(name)) endpoint.dns.search_domains.push(name);
            }
          }
          break;
        }
        case 'saveconfig': warnings.add('SaveConfig 未导入；客户端统一保存端点配置。'); break;
        case 'table': case 'fwmark': case 'preup': case 'postup': case 'predown': case 'postdown':
          fail(index + 1, '此配置包含系统路由、标记或启动脚本，无法作为端点完整导入。请移除相关字段，并在客户端配置路由。'); break;
        default: fail(index + 1, '不支持此 Interface 字段，请检查配置格式。');
      }
    } else if (peer) {
      switch (key) {
        case 'publickey': peer.public_key = value; break;
        case 'presharedkey': peer.pre_shared_key = value; break;
        case 'endpoint': peer.endpoint = value; break;
        case 'allowedips': peer.allowed_ips.push(...addresses(value)); break;
        case 'persistentkeepalive': peer.keepalive_secs = /^off$/i.test(value) ? 0 : number(value, index + 1, 65535); break;
        default: fail(index + 1, '不支持此 Peer 字段，请检查配置格式。');
      }
    }
  }
  if (!seenInterface) throw new Error('未找到 [Interface]；请选择 WireGuard .conf 配置文件。');
  if (endpoint.dns && !endpoint.dns.servers.length) throw new Error('DNS 字段需要至少一个 DNS 服务器 IP。');
  if (endpoint.dns?.search_domains.length) warnings.add('搜索域已保留；短主机名的自动补全仍取决于系统，完整域名会使用导入的 DNS。');
  const issue = validateWireGuardEndpoint(endpoint)[0];
  if (issue) throw new Error(issue.message);
  return { endpoint, warnings: [...warnings] };
}
