import { compareAppVersions } from './app-update-policy';
import type { PluginRelease } from './plugins';

export type PluginChannel = 'stable' | 'rc' | 'dev';

export const pluginChannelLabel: Record<PluginChannel, string> = {
  stable: '正式版', rc: '候选版', dev: '开发版',
};

export function releaseChannel(release: PluginRelease): PluginChannel {
  if (release.channel === 'stable' || release.channel === 'rc' || release.channel === 'dev') return release.channel;
  return release.prerelease ? 'dev' : 'stable';
}

export function releasesInChannel(releases: PluginRelease[], channel: PluginChannel): PluginRelease[] {
  return releases.filter(release => releaseChannel(release) === channel)
    .sort((left, right) => compareAppVersions(right.tag_name, left.tag_name));
}

export function availablePluginUpdate(
  installedVersion: string,
  releases: PluginRelease[],
  channel: PluginChannel,
): PluginRelease | null {
  return releasesInChannel(releases, channel)
    .find(release => compareAppVersions(release.tag_name, installedVersion) > 0) ?? null;
}
