import assert from 'node:assert/strict';
import fs from 'node:fs';
import {
  classifyAppVersion,
  compareAppVersions,
  fetchAppReleases,
  shouldShowProminentUpdate,
} from '../src/lib/services/app-update-policy.ts';

assert.equal(classifyAppVersion('0.0.16'), 'stable');
assert.equal(classifyAppVersion('0.0.17-rc.1'), 'preview');
assert.equal(classifyAppVersion('0.0.17-beta.2'), 'test');
assert.equal(classifyAppVersion('0.0.17-dev'), 'test');
assert.equal(classifyAppVersion('dev'), 'test');

assert.equal(compareAppVersions('0.0.17', '0.0.17-rc.2'), 1);
assert.equal(compareAppVersions('0.0.17-rc.2', '0.0.17-beta.9'), 1);
assert.equal(compareAppVersions('0.0.16', '0.0.17-rc.1'), -1);
assert.equal(compareAppVersions('0.0.17-beta.10', '0.0.17-beta.2'), 1);

assert.equal(shouldShowProminentUpdate('0.0.16', '0.0.17'), true);
assert.equal(shouldShowProminentUpdate('0.0.16', '0.0.17-rc.1'), false);
assert.equal(shouldShowProminentUpdate('0.0.17-rc.1', '0.0.17'), false);

const releases = await fetchAppReleases(async () => new Response(JSON.stringify([
  {
    tag_name: 'v0.0.17-rc.1',
    draft: false,
    published_at: '2026-07-21T00:00:00Z',
    html_url: 'https://example.test/rc',
    assets: [{ name: 'latest.json' }],
  },
  {
    tag_name: 'v0.0.16',
    draft: false,
    published_at: '2026-07-20T00:00:00Z',
    html_url: 'https://example.test/stable',
    assets: [{ name: 'latest.json' }],
  },
  {
    tag_name: 'v0.0.18-beta.1',
    draft: false,
    assets: [{ name: 'installer.exe' }],
  },
]), { status: 200 }));

assert.deepEqual(releases.map(({ version, channel }) => ({ version, channel })), [
  { version: '0.0.17-rc.1', channel: 'preview' },
  { version: '0.0.16', channel: 'stable' },
]);

const updaterService = fs.readFileSync('src/lib/services/updater.svelte.ts', 'utf8');
assert.match(
  updaterService,
  /import \{ relaunch \} from '@tauri-apps\/plugin-process';/,
  'the updater must import the supported Tauri process relaunch API',
);
assert.equal(
  updaterService.match(/\(\) => relaunch\(\)/g)?.length,
  2,
  'both immediate updates and version-manager installs must relaunch after installation',
);

const capability = JSON.parse(fs.readFileSync('src-tauri/capabilities/default.json', 'utf8'));
assert.ok(
  capability.permissions.includes('process:allow-restart'),
  'the desktop capability must allow updater-triggered relaunches',
);

console.log('app-update-policy: ok');
