// Scoped deterministic IO fixtures. These never invoke Tauri, DNS or a kernel.
export * from './config';
import * as base from './config';
import type { ProfileSettings } from '../../src/lib/services/core';
const enabled = () => new URLSearchParams(location.search).get('fixture') === 'settings-drafts';
interface Controls {
  reads: number;
  writes: number;
  holdWrites: boolean;
  failWrites: boolean;
  replaceSource: () => void;
  releaseWrite: () => void;
}
let settings: ProfileSettings | null = null;
let finishWrite: (() => void) | null = null;
const controls: Controls = {
  reads: 0, writes: 0, holdWrites: false, failWrites: false,
  replaceSource() { if (settings) { settings.settings.runtime.udpUpstreamIdleTimeoutSeconds = 77; settings.settings.dns.enabled = false; } },
  releaseWrite() { const finish = finishWrite; finishWrite = null; finish?.(); },
};
if (enabled()) Object.assign(window, { settingsDraftFixture: controls });
export async function getProfileSettings(): Promise<ProfileSettings> {
  if (!enabled()) return base.getProfileSettings() as Promise<ProfileSettings>;
  controls.reads++;
  settings ??= {
    profileId: 'fixture-profile', editedFields: [], applied: false,
    settings: {
      ...(await base.getProfileSettings()).settings,
      dns: { enabled: true, dnsHijack: false, config: {
        servers: { system: { type: 'system' } }, default_server: 'system', answer: { type: 'real' },
        policy: { node_server: 'system' }, dispatch: [],
      } },
      urlTest: { url: 'http://www.gstatic.com/generate_204', toleranceMs: 50 },
      runtime: { udpUpstreamIdleTimeoutSeconds: 30 },
    },
  } as ProfileSettings;
  return structuredClone(settings);
}
export async function applyProfileSettings(profileId: string, changes: Record<string, unknown>, reset: string[] = []): Promise<ProfileSettings> {
  if (!enabled()) return base.applyProfileSettings(profileId, changes, reset) as Promise<ProfileSettings>;
  controls.writes++;
  if (controls.holdWrites) await new Promise<void>(resolve => { finishWrite = resolve; });
  if (controls.failWrites) throw { code: 'unconfirmed', message: '原设置提交结果未确认；草稿保留，不自动重放' };
  const snapshot = await getProfileSettings();
  for (const [key, value] of Object.entries(changes)) {
    const [section, field] = key.split('.');
    const target = snapshot.settings as unknown as Record<string, unknown>;
    if (field) (target[section] as Record<string, unknown>)[field] = value;
    else target[section] = value;
  }
  snapshot.editedFields = [...new Set([...snapshot.editedFields, ...Object.keys(changes)])].filter(key => !reset.includes(key));
  snapshot.applied = false;
  settings = structuredClone(snapshot);
  return snapshot;
}
