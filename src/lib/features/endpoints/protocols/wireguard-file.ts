import { parseWireGuardConfig } from '$lib/features/endpoints/protocols/wireguard-import';
import type { UserEndpoint } from '../library';

export type WireGuardFilePreview = { filename: string; endpoint?: UserEndpoint; warnings: string[]; error: string };
export async function readWireGuardFile(file: File): Promise<WireGuardFilePreview> {
  const preview: WireGuardFilePreview = { filename: file.name, warnings: [], error: '' };
  if (!/\.(conf|ini)$/i.test(file.name)) return { ...preview, error: '请选择 WireGuard .conf 或 .ini 配置文件。' };
  if (file.size > 256 * 1024) return { ...preview, error: '单个配置文件不能超过 256 KiB。' };
  let content: string;
  try { content = await file.text(); }
  catch { return { ...preview, error: '文件读取失败，请重新选择。' }; }
  try { return { ...preview, ...parseWireGuardConfig(content, file.name) }; }
  catch (error) { return { ...preview, error: error instanceof Error ? error.message : '配置格式不正确。' }; }
}
