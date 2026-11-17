import type { ZeroRuleType } from '$lib/types/domain';
/** Shared vocabulary for condition resources and their routing consumers. */
export const RULE_MATCH_TYPES: {value:ZeroRuleType;label:string;placeholder:string}[] = [
  {value:'domain_exact',label:'精确域名',placeholder:'api.example.com'},
  {value:'domain_suffix',label:'域名后缀',placeholder:'example.com'},
  {value:'domain_keyword',label:'域名关键词',placeholder:'example'},
  {value:'ipv4_cidr',label:'IPv4 网段',placeholder:'192.168.1.0/24'},
  {value:'ipv6_cidr',label:'IPv6 网段',placeholder:'fd00::/8'},
];
