// Node display helpers — emoji extraction, protocol badges, delay grading.
//
// Pure functions shared by NodesTab and related node-card components.
// Keeping them here avoids duplicating style/label maps across components
// (see NodeSelector / NodeTileGrid which each had their own copies).

import type { ProxyNode } from '$lib/types/protocol';

// ── Emoji / flag extraction ──────────────────────────────────────────

// Regional indicator symbols (country flags 🇦-🇿) and the core emoji
// pictographic / symbol blocks.  We use `u` (sticky) + `u` (unicode) flag
// so the regex covers surrogate-pair code points correctly.
const FLAG_RE = /[\u{1F1E6}-\u{1F1FF}]{2}/u;
// Leading emoji run — flags, pictographic emoji, and skin-tone modifiers.
// We capture 1–2 leading emoji so "🇭🇰 香港 01" → flag + "香港 01".
const LEADING_EMOJI_RE = /^((?:[\u{1F1E6}-\u{1F1FF}]{2})|(?:[\u{2190}-\u{2BFF}]|[\u{2E80}-\u{3243}]|[\u{1F000}-\u{1FAFF}]|[\u{2600}-\u{27BF}]|[\u{1F300}-\u{1F9FF}]))/u;

export interface ParsedName {
  emoji?: string;
  cleanName: string;
}

/** Split a raw node tag into leading emoji (if any) + remainder. */
export function parseNodeName(raw: string): ParsedName {
  if (!raw) return { cleanName: '' };
  const trimmed = raw.trim();

  // Try flag emoji first (exactly two regional indicators).
  const flag = trimmed.match(FLAG_RE);
  if (flag && flag.index === 0) {
    return { emoji: flag[0], cleanName: trimmed.slice(flag[0].length).trim() };
  }

  // Then a single leading pictographic emoji.
  const emojiMatch = trimmed.match(LEADING_EMOJI_RE);
  if (emojiMatch) {
    const emoji = emojiMatch[0];
    return { emoji, cleanName: trimmed.slice(emoji.length).trim() || trimmed };
  }

  return { cleanName: trimmed };
}

// ── Protocol badges ──────────────────────────────────────────────────

export interface ProtocolStyle {
  label: string;
  bg: string;
  color: string;
}

// Compact short labels for the node card badge.  Keys are matched by
// substring so "hysteria2" and "hysteria" both resolve correctly.
const PROTOCOL_LABELS: Record<string, string> = {
  shadowsocks: 'SS',
  ss: 'SS',
  vmess: 'VMESS',
  vless: 'VLESS',
  trojan: 'TROJAN',
  hysteria2: 'HY2',
  hysteria: 'HY',
  wireguard: 'WG',
  tuic: 'TUIC',
  socks: 'SOCKS',
  http: 'HTTP',
  direct: 'DIRECT',
  reject: 'BLOCK',
  selector: 'GROUP',
  urltest: 'TEST',
  url_test: 'TEST',
  fallback: 'FALLBACK',
  loadbalance: 'LB',
};

const PROTOCOL_COLORS: Record<string, { bg: string; color: string }> = {
  shadowsocks: { bg: 'rgba(139,92,246,0.14)', color: '#8B5CF6' },
  ss:          { bg: 'rgba(139,92,246,0.14)', color: '#8B5CF6' },
  vmess:       { bg: 'rgba(59,130,246,0.14)',  color: '#3B82F6' },
  vless:       { bg: 'rgba(16,185,129,0.14)',  color: '#10B981' },
  trojan:      { bg: 'rgba(239,68,68,0.14)',   color: '#EF4444' },
  hysteria2:   { bg: 'rgba(249,115,22,0.14)',  color: '#F97316' },
  hysteria:    { bg: 'rgba(249,115,22,0.14)',  color: '#F97316' },
  wireguard:   { bg: 'rgba(20,184,166,0.14)',  color: '#14B8A6' },
  tuic:        { bg: 'rgba(99,102,241,0.14)',  color: '#6366F1' },
  socks:       { bg: 'rgba(168,162,158,0.14)', color: '#A8A29E' },
  http:        { bg: 'rgba(168,162,158,0.14)', color: '#A8A29E' },
};

const DEFAULT_PROTOCOL_COLOR = { bg: 'rgba(107,114,128,0.10)', color: '#6B7280' };

function protocolKey(protocol: string): string {
  return protocol.toLowerCase().replace(/[-_\s]/g, '');
}

/** Return the compact label + color for a protocol family. */
export function getProtocolStyle(protocol: string): ProtocolStyle {
  const key = protocolKey(protocol);
  // Exact label match first, then substring match.
  let label = PROTOCOL_LABELS[key];
  if (!label) {
    const match = Object.keys(PROTOCOL_LABELS).find((k) => key.includes(k));
    label = match ? PROTOCOL_LABELS[match] : protocol.toUpperCase().slice(0, 6);
  }
  const colorEntry = PROTOCOL_COLORS[key]
    ?? Object.entries(PROTOCOL_COLORS).find(([k]) => key.includes(k))?.[1]
    ?? DEFAULT_PROTOCOL_COLOR;
  return { label, ...colorEntry };
}

// ── Delay grading (Clash / zashboard style) ───────────────────────────

export type DelayLevel = 'excellent' | 'good' | 'medium' | 'slow' | 'dead' | 'idle';

export interface DelayStyle {
  level: DelayLevel;
  /** Foreground color for the numeric value. */
  color: string;
  /** Background tint for delay pills / bars. */
  bg: string;
  /** Solid bar fill color. */
  bar: string;
  /** Short human label, e.g. "优" / "中" / "慢". */
  grade: string;
}

/**
 * Grade a latency value into a display style bucket.
 *
 * Thresholds follow Clash conventions:
 *   - 0          → idle (not tested)
 *   - 1–100ms    → excellent
 *   - 100–200ms  → good
 *   - 200–500ms  → medium
 *   - > 500ms    → slow
 */
export function gradeDelay(delay: number, alive?: boolean): DelayStyle {
  if (delay < 0) {
    // Timeout / unreachable — e.g. kernel not running or probe failed.
    return { level: 'dead', color: 'var(--destructive)', bg: 'rgba(239,68,68,0.10)', bar: '#EF4444', grade: 'timeout' };
  }
  if (alive === false && delay <= 0) {
    return { level: 'dead', color: 'var(--destructive)', bg: 'rgba(239,68,68,0.10)', bar: '#EF4444', grade: '离线' };
  }
  if (delay <= 0) {
    return { level: 'idle', color: 'var(--muted-foreground)', bg: 'transparent', bar: 'transparent', grade: '—' };
  }
  if (delay < 100) {
    return { level: 'excellent', color: '#16A34A', bg: 'rgba(34,197,94,0.10)', bar: '#22C55E', grade: '优' };
  }
  if (delay < 200) {
    return { level: 'good', color: '#22C55E', bg: 'rgba(34,197,94,0.10)', bar: '#22C55E', grade: '良' };
  }
  if (delay < 500) {
    return { level: 'medium', color: '#D97706', bg: 'rgba(245,158,11,0.10)', bar: '#F59E0B', grade: '中' };
  }
  return { level: 'slow', color: '#DC2626', bg: 'rgba(239,68,68,0.10)', bar: '#EF4444', grade: '慢' };
}

/** Format a delay value for compact display. */
export function formatDelay(delay: number): string {
  if (delay < 0) return 'timeout';
  if (delay <= 0) return '—';
  if (delay < 1000) return String(delay);
  return `${(delay / 1000).toFixed(1)}s`;
}

/** Delay bar width as percentage (clamped, max 1000ms → 100%). */
export function delayBarWidth(delay: number): string {
  if (delay < 0) return '100%'; // timeout — full red bar
  if (delay <= 0) return '0%';
  return `${Math.min(100, (delay / 1000) * 100)}%`;
}

// ── Policy-group kind helpers ─────────────────────────────────────────

export interface GroupKindStyle {
  label: string;
  color: string;
}

const GROUP_KINDS: Array<{ match: string[]; label: string; color: string }> = [
  { match: ['selector'], label: 'Selector', color: '#6366F1' },
  { match: ['urltest', 'url_test'], label: 'URLTest', color: '#F59E0B' },
  { match: ['fallback'], label: 'Fallback', color: '#10B981' },
  { match: ['loadbalance', 'load_balance'], label: 'LB', color: '#EC4899' },
];

export function getGroupKindStyle(kind?: string): GroupKindStyle | null {
  if (!kind) return null;
  const k = kind.toLowerCase();
  for (const entry of GROUP_KINDS) {
    if (entry.match.some((m) => k.includes(m))) {
      return { label: entry.label, color: entry.color };
    }
  }
  return { label: kind, color: '' };
}

// ── Attribute chips (UDP / TLS / network) ─────────────────────────────

export interface NodeChip {
  key: string;
  label: string;
  title: string;
  tone: 'accent' | 'success' | 'warning' | 'muted';
}

/**
 * Build the ordered list of attribute chips for a node card.
 * Returns an empty array for direct / reject outbounds.
 */
export function getNodeChips(node: Pick<ProxyNode, 'protocol' | 'udp' | 'tls' | 'network'>): NodeChip[] {
  const proto = node.protocol.toLowerCase();
  if (proto === 'direct' || proto === 'reject' || proto === 'block') return [];

  const chips: NodeChip[] = [];

  if (node.udp === true) {
    chips.push({ key: 'udp', label: 'UDP', title: '支持 UDP 转发', tone: 'success' });
  }
  if (node.tls === true) {
    chips.push({ key: 'tls', label: 'TLS', title: '启用 TLS', tone: 'accent' });
  }
  if (node.network && node.network !== 'tcp') {
    chips.push({
      key: 'net',
      label: node.network.toUpperCase(),
      title: `传输层: ${node.network}`,
      tone: 'warning',
    });
  }
  return chips;
}
