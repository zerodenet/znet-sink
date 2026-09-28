const parameters = () => new URLSearchParams(location.search);
let healthCalls = 0;
export function capabilityFixture(): GuiZeroCapabilities {
  const supported = { supported: true, level: 'experimental', notes: ['shared_peer_tcp_tunnel'] };
  const disabled = { supported: false, level: 'unsupported', notes: [] };
  const old = parameters().get('version') === 'v002';
  const compiled = parameters().get('compiled') !== 'false';
  return { available: true, features: [], buildFeatures: [], globalLimitations: [],
    errorCodes: [], permissions: [], adapters: [], sinks: [],
    contracts: { capabilities: { current: 1, minimumSupported: 1 },
      controlApi: { current: 1, minimumSupported: 1 }, configSchema: { current: 1, minimumSupported: 1 },
      errorCodes: { current: 1, minimumSupported: 1 } },
    protocols: old ? [] : [{ name: 'wireguard', status: 'experimental', compiled,
      compatibilityBaseline: 'gotatun_v0.9.2', inboundTcp: compiled, inboundUdp: compiled,
      outboundTcp: compiled, outboundUdp: compiled, mux: false,
      inboundTcpState: compiled ? supported : disabled, inboundUdpState: compiled ? supported : disabled,
      outboundTcpState: compiled ? supported : disabled, outboundUdpState: compiled ? supported : disabled,
      muxState: disabled, limitations: [] }] };
}
export function healthFixture(): GuiCoreHealth {
  if (++healthCalls > 1 && parameters().has('fail-health')) throw new Error('connection_closed');
  const old = parameters().get('version') === 'v002';
  return { healthy: true, engineVersion: old ? '0.0.2-rc.202609271132' : '0.0.3-dev.202609271529',
    outboundDevices: old ? undefined : [
      { tag: 'wg-a', peerIndex: 0, state: 'awaiting_handshake', endpointResolutionFailed: false },
      { tag: 'wg-b', peerIndex: 1, state: 'reachable', lastHandshakeAgeMs: 0,
        lastAuthenticatedPacketAgeMs: 1200, endpointResolutionFailed: true },
    ] };
}
import type { GuiCoreHealth, GuiZeroCapabilities } from '../../src/lib/types/gui-api';
