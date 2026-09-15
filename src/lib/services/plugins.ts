import { invoke } from '@tauri-apps/api/core';
export interface PluginPermission { capability: string; scope: string }
export interface PluginReview { key: string; identity: string; registration: number; revision: number }
export interface PluginComponent {
  plugin_id: string; name: string; component_id: string; version: string; publisher: string;
  review: PluginReview | null; enabled: boolean; running: boolean; blocked: string | null;
  permissions: Array<{ request: PluginPermission; required: boolean; supported: boolean; granted: boolean }>;
}
export interface PluginSnapshot { checked: boolean; components: PluginComponent[]; notices: string[] }
export interface PluginListing { product_id?: string; id: string; name: string; description: string; publisher: { id: string }; repository: string }
export interface PluginRelease { channel?: 'stable' | 'rc' | 'dev' | null; html_url?: string; tag_name: string; name: string | null; body: string | null; prerelease: boolean; published_at: string | null }
export const pluginApi = {
  catalog: () => invoke<PluginListing[]>('plugins_catalog'),
  releases: (id: string) => invoke<PluginRelease[]>('plugins_releases', { id }),
  installRelease: (id: string, tag: string) => invoke<PluginSnapshot>('plugins_install_release', { id, tag }),
  supported: () => invoke<boolean>('plugins_supported'),
  snapshot: () => invoke<PluginSnapshot>('plugins_snapshot'),
  refresh: () => invoke<PluginSnapshot>('plugins_refresh'),
  install: (path: string) => invoke<PluginSnapshot>('plugins_install', { path }),
  authorize: (review: PluginReview, grants: PluginPermission[]) => invoke<PluginSnapshot>('plugins_authorize', { review, grants }),
  stop: (key: string) => invoke<PluginSnapshot>('plugins_stop', { key }),
  run: (review: PluginReview) => invoke<unknown>('plugins_run', { review }),
  uninstall: (id: string) => invoke<PluginSnapshot>('plugins_uninstall', { id }),
};
