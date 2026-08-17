import { invoke } from '@tauri-apps/api/core';
import type { GuiTunStatus } from '$lib/types/gui-api';

export async function getGuiTunStatus(): Promise<GuiTunStatus> {
  return invoke('gui_tun_status');
}

export async function enableGuiTun(): Promise<GuiTunStatus> {
  return invoke('gui_tun_enable');
}

export async function disableGuiTun(): Promise<GuiTunStatus> {
  return invoke('gui_tun_disable');
}
