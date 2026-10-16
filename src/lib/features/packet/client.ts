import { invoke } from '@tauri-apps/api/core';
import type { PacketGateway } from './types';
export const packetGateway: PacketGateway = {
  snapshot: query => invoke('gui_packet_routes_snapshot', { query: query ?? null }),
  detail: target => invoke('gui_packet_route_detail', { target }),
  close: target => invoke('gui_close_packet_route', { target }),
};
