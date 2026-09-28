import type { GuiOutboundDeviceHealth } from '$lib/types/gui-api';

const deviceStates: Record<string, string> = {
  not_started: '未启动',
  endpoint_unresolved: '端点未解析',
  awaiting_handshake: '等待握手',
  recently_handshaken: '近期已握手',
  reachable: '近期收到认证数据',
  degraded: '连接退化',
  stopped: '已停止',
};

export function outboundDeviceStateLabel(device: GuiOutboundDeviceHealth): string {
  return deviceStates[device.state] ?? '未知状态';
}

export function healthAgeLabel(ageMs: number | null | undefined): string {
  if (ageMs == null || !Number.isFinite(ageMs) || ageMs < 0) return '暂无记录';
  if (ageMs < 1_000) return '不足 1 秒前';
  if (ageMs < 60_000) return `${Math.floor(ageMs / 1_000)} 秒前`;
  return `${Math.floor(ageMs / 60_000)} 分钟前`;
}
