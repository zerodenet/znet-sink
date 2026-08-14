import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
}

const configService = read('src/lib/services/config.ts');
const configSignal = read('src/lib/services/proxy-config-signal.svelte.ts');
const profilesTab = read('src/lib/components/tabs/ProfilesTab.svelte');
const configEditor = read('src/lib/services/config-editor.svelte.ts');
const guiConnection = read('src-tauri/src/services/gui_connection.rs');
const coreProcessCommand = read('src-tauri/src/commands/core_process.rs');

assert.ok(
  configSignal.includes('revision = $state(0)')
    && configSignal.includes('markChanged(activeSourceMayHaveChanged = false)')
    && configSignal.includes('onActiveChanged('),
  'proxy configs should expose one shared reactive revision across Lite and Pro surfaces',
);

assert.ok(
  configService.includes('void proxyConfigSignal.revision')
    && configService.includes("invoke<ProxyConfigProfile>('proxy_config_set_active'")
    && configService.includes('proxyConfigSignal.markChanged(true)')
    && configService.includes("invoke<SubscriptionProfile>('subscription_sync'")
    && configService.includes("invoke<SubscriptionSyncAllOutcome>('subscription_sync_all'"),
  'successful profile activation and subscription sync must invalidate every config-backed surface',
);

assert.ok(
  profilesTab.includes('$effect(() => {')
    && profilesTab.includes('refresh();')
    && profilesTab.includes('listProxyConfigs()'),
  'ProfilesTab should keep loading through listProxyConfigs so the shared revision re-runs its existing effect',
);

assert.ok(
  configEditor.includes("import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte'")
    && configEditor.includes('private _sourceProfileId: string | null = null')
    && configEditor.includes('private _sourceProfileUpdatedAt: number | null = null')
    && configEditor.includes('proxyConfigSignal.onActiveChanged')
    && configEditor.includes('reconcileExternalSource()')
    && configEditor.includes('activeId === this._sourceProfileId')
    && configEditor.includes('activeUpdatedAt === this._sourceProfileUpdatedAt'),
  'the singleton Pro config editor must reconcile its source identity instead of retaining a stale profile after Lite switches',
);

assert.ok(
  guiConnection.includes('let mut process = core_process::refresh_status(state)?')
    && guiConnection.includes('if process.state != CoreProcessState::Running')
    && guiConnection.includes('process.pid = None;')
    && coreProcessCommand.includes('let mut status = core_process::status(state)?')
    && coreProcessCommand.includes('if status.state != CoreProcessState::Running')
    && coreProcessCommand.includes('status.pid = None;'),
  'both GUI connection status and direct process status must expose PID only for the currently running managed child',
);

console.log('cross-mode-state: ok');
