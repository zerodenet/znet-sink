import { invoke } from '@tauri-apps/api/core';
import { validLocationSnapshot, type MobileLocationSnapshot } from './location';

export async function getMobileLocationSnapshot(): Promise<MobileLocationSnapshot> {
  const snapshot = await invoke<MobileLocationSnapshot>('gui_mobile_location_snapshot');
  if (!validLocationSnapshot(snapshot)) throw { code: 'mobile_location_invalid', message: '当前导航身份不可用，请重新读取。' };
  return snapshot;
}
