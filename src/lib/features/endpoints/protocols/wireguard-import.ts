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
        case 'dns': warnings.add('文件中的 DNS 未应用；请在客户端 DNS 设置中配置。'); break;
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
  const issue = validateWireGuardEndpoint(endpoint)[0];
  if (issue) throw new Error(issue.message);
  return { endpoint, warnings: [...warnings] };
}
