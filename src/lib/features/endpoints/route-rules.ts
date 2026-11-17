import type { ZeroRule, ZeroRuleType } from '$lib/types/domain';
/** The kernel compiler remains authoritative for domain/CIDR validation. */
export function endpointRuleLines(text: string, type: ZeroRuleType): ZeroRule[] {
  const values = [...new Set(text.split(/\r?\n/).map(v => v.trim()).filter(Boolean))];
  if (!values.length) throw new Error('请至少填写一条域名或 IP 网段');
  if (values.length > 1000) throw new Error('一次最多填写 1000 条规则');
  return values.map(value => ({type, value: type === 'ipv4_cidr' && !value.includes('/') ? `${value}/32`
    : type === 'ipv6_cidr' && !value.includes('/') ? `${value}/128` : value.toLowerCase()}));
}
