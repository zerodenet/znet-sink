import type { GuiZeroCapabilities } from '$lib/types/gui-api';

export type KernelFeatureState = 'supported' | 'unsupported' | 'unknown';

export interface KernelFeatureSupport {
  state: KernelFeatureState;
  feature: string;
  reason?: string;
}

export interface ClientKernelFeatures {
  tunDualStack: KernelFeatureSupport;
  tunDnsHijack: KernelFeatureSupport;
  tunDnsSystemAuto: KernelFeatureSupport;
  dnsSplitDispatch: KernelFeatureSupport;
  dnsFakeIpDualStack: KernelFeatureSupport;
  dnsAddressFamilyPolicy: KernelFeatureSupport;
  directCandidateFallback: KernelFeatureSupport;
  directDialAttempts: KernelFeatureSupport;
}

function supportsCapabilityContractV1(capabilities: GuiZeroCapabilities): boolean {
  const contract = capabilities.contracts?.capabilities;
  return Boolean(contract && contract.minimumSupported <= 1 && contract.current >= 1);
}

export function kernelFeatureSupport(
  capabilities: GuiZeroCapabilities | null | undefined,
  feature: string,
): KernelFeatureSupport {
  if (!capabilities?.available) {
    return { state: 'unknown', feature, reason: capabilities?.error || '内核当前不可用' };
  }
  if (!supportsCapabilityContractV1(capabilities)) {
    return { state: 'unknown', feature, reason: '内核未发布兼容的 V1 能力契约' };
  }
  const declared = new Set([...capabilities.features, ...capabilities.buildFeatures]);
  return declared.has(feature)
    ? { state: 'supported', feature }
    : { state: 'unsupported', feature, reason: `当前内核未声明 ${feature}` };
}

export function projectClientKernelFeatures(
  capabilities: GuiZeroCapabilities | null | undefined,
): ClientKernelFeatures {
  const support = (feature: string) => kernelFeatureSupport(capabilities, feature);
  return {
    tunDualStack: support('tun_dual_stack_ingress'),
    tunDnsHijack: support('tun_dns_hijack_udp_tcp'),
    tunDnsSystemAuto: support('tun_dns_system_auto'),
    dnsSplitDispatch: support('dns_split_dispatch'),
    dnsFakeIpDualStack: support('dns_fake_ip_dual_stack'),
    dnsAddressFamilyPolicy: support('dns_address_family_policy'),
    directCandidateFallback: support('direct_tcp_trusted_target_candidate_fallback'),
    directDialAttempts: support('direct_tcp_dial_attempt_observability_v1'),
  };
}

export function featureStateLabel(feature: KernelFeatureSupport): string {
  if (feature.state === 'supported') return '已支持';
  if (feature.state === 'unsupported') return '当前内核不支持';
  return '等待内核校验';
}
