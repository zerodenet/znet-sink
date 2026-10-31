import type { UserEndpoint } from '../library';
export type WireGuardPeer = { public_key: string; endpoint?: string; allowed_ips: string[]; [key: string]: unknown };
export type WireGuardProtocol = {
  type: 'wireguard'; private_key: string; addresses: string[]; peers: WireGuardPeer[]; [key: string]: unknown;
};
export const createWireGuardEndpoint = (): UserEndpoint => ({ tag: '', enabled: true,
  directions: { inbound: false, outbound: true },
  protocol: { type: 'wireguard', private_key: '', addresses: [], peers: [{ public_key: '', allowed_ips: ['0.0.0.0/0', '::/0'] }] },
});
export const parseAddressList = (value: string) => value.split(/[,\s]+/).filter(Boolean);
