import { invoke } from '@tauri-apps/api/core';
import type { PacketGateway } from './types';
export const packetGateway: PacketGateway = {
  snapshot: () => invoke('gui_packet_routes_snapshot'),
  detail: target => invoke('gui_packet_route_detail', { target }),
  close: target => invoke('gui_close_packet_route', { target }),
};
