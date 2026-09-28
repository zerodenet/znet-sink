import assert from 'node:assert/strict';
import { test } from 'node:test';
import { kernelFeatureSupport } from '../src/lib/services/kernel-capabilities.ts';
import { healthAgeLabel, outboundDeviceStateLabel } from '../src/lib/services/kernel-health.ts';

const contracts = (current = 1, minimumSupported = 1) => ({
  capabilities: { current, minimumSupported },
});
const capabilities = (features, contract = contracts()) => ({
  available: true, contracts: contract, features, buildFeatures: [],
});

test('both kernel generations enable only declared compatible capabilities', () => {
  // Both product generations publish V1; release versions are not feature gates.
  for (const version of ['0.0.2-rc.202609271132', '0.0.3-dev.202609271529']) {
    const caps = capabilities(['tun_dual_stack_ingress']);
    assert.equal(kernelFeatureSupport(caps, 'tun_dual_stack_ingress').state, 'supported', version);
    assert.equal(kernelFeatureSupport(caps, 'wireguard').state, 'unsupported', version);
  }
  assert.equal(kernelFeatureSupport(capabilities(['wireguard'], contracts(2, 1)), 'wireguard').state, 'supported');
  for (const contract of [undefined, contracts(2, 2), contracts(0, 0), contracts(1, 0)]) {
    const caps = capabilities(['wireguard']); caps.contracts = contract;
    assert.equal(kernelFeatureSupport(caps, 'wireguard').state, 'unknown');
  }
  assert.equal(kernelFeatureSupport(null, 'wireguard').state, 'unknown');
});

test('handshake, authenticated data, DNS degradation and unknown states remain distinct', () => {
  assert.equal(outboundDeviceStateLabel({ state: 'recently_handshaken' }), '近期已握手');
  assert.equal(outboundDeviceStateLabel({ state: 'reachable' }), '近期收到认证数据');
  assert.equal(outboundDeviceStateLabel({ state: 'reachable', endpointResolutionFailed: true }), '近期收到认证数据');
  assert.equal(outboundDeviceStateLabel({ state: 'future_state' }), '未知状态');
  assert.equal(healthAgeLabel(0), '不足 1 秒前');
  assert.equal(healthAgeLabel(1_200), '1 秒前');
  assert.equal(healthAgeLabel(60_000), '1 分钟前');
  for (const age of [null, undefined, -1, NaN]) assert.equal(healthAgeLabel(age), '暂无记录');
});
