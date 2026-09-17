import { invoke } from '@tauri-apps/api/core';

export async function openExternalUrl(url: string): Promise<void> {
  await invoke('platform_open_url', { url });
}

export async function openPath(path: string): Promise<void> {
  await invoke('platform_open_path', { path });
}

export async function revealItemInDir(path: string): Promise<void> {
  await invoke('platform_reveal_path', { path });
}
