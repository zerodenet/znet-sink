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
const guiState = read('src/lib/services/gui-state.svelte.ts');
const appStore = read('src/lib/services/store.svelte.ts');
const settingsPanel = read('src/lib/components/SettingsPanel.svelte');
const tunSettings = read('src/lib/components/settings/TunSettingsPanel.svelte');
const tunRuntime = read('src-tauri/src/kernel/zero/runtime.rs');

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

assert.ok(
  guiState.includes("tracedOperation('proxy', 'tun.enable', () => enableGuiTun())")
    && guiState.includes("tracedOperation('proxy', 'tun.disable', () => disableGuiTun())")
    && guiState.includes('get isConnected(): boolean')
    && guiState.includes('return this.isTunEnabled;'),
  'Lite power must map to Zero TUN lifecycle rather than the system-proxy lifecycle',
);

const initialSnapshot = guiState.indexOf('await this.refreshAll();');
const initializationUnlock = guiState.indexOf('this.isInitializing = false;', initialSnapshot);
const autoConnect = guiState.indexOf('await this.autoConnectForMode(', initializationUnlock);
assert.ok(
  initialSnapshot >= 0 && initializationUnlock > initialSnapshot && autoConnect > initializationUnlock,
  'Lite default auto-connect should run only after the first trusted snapshot and after UI action guards are unlocked',
);

const handoffTun = guiState.indexOf("tracedOperation('proxy', 'lite.tun.handoff', () => enableGuiTun())");
const handoffSystemProxyRelease = guiState.indexOf("tracedOperation('proxy', 'lite.system_proxy.release', () => guiDisconnect())", handoffTun);
assert.ok(
  guiState.includes('async prepareLiteCapture()')
    && guiState.includes('const systemProxyOwned = this.connection?.systemProxyEnabled === true')
    && handoffTun >= 0
    && handoffSystemProxyRelease > handoffTun
    && guiState.includes('if (!systemProxyOwned) return;')
    && guiState.includes('if (tunStarted)')
    && guiState.includes('this.tunStatus = await disableGuiTun();'),
  'Pro -> Lite should start TUN before releasing an active GUI-owned system proxy, preserve an off session, and roll back TUN when handoff fails',
);

assert.ok(
  appStore.includes("const PRO_ONLY_SETTINGS = new Set<SettingsSection>(['tun', 'config'])")
    && appStore.includes("if (mode === 'lite')")
    && appStore.includes('await guiState.prepareLiteCapture()')
    && settingsPanel.includes("section.id !== 'config' && section.id !== 'tun'")
    && settingsPanel.includes("activeSection === 'config' || activeSection === 'tun'"),
  'TUN configuration must remain Pro-only while entering Lite reconciles the active capture path',
);

assert.ok(
  tunSettings.includes('bind:value={name}')
    && tunSettings.includes('bind:value={tag}')
    && tunSettings.includes('bind:value={addr}')
    && tunSettings.includes('bind:value={secondaryAddr}')
    && tunSettings.includes('bind:value={mtu}')
    && tunSettings.includes('checked={dualStack}')
    && tunSettings.includes('checked={dnsHijack}')
    && !tunSettings.includes('autoRoute')
    && !tunSettings.includes('strictRoute'),
  'Pro TUN settings should expose Zero interface/DNS preferences without presenting auto-route or strict-route as user options',
);

assert.ok(
  tunRuntime.includes('if (status.enabled)')
    && tunRuntime.includes('if (!status.enabled)')
    && tunRuntime.includes('params.insert("auto_route".to_string(), json!(true));')
    && tunRuntime.includes('params.insert("strict_route".to_string(), json!(true));')
    && tunRuntime.includes('params.insert("dual_stack".to_string(), json!(tun.dual_stack));')
    && tunRuntime.includes('params.insert("dns_hijack".to_string(), json!(tun.dns_hijack));'),
  'GUI TUN reconciliation should be idempotent and always request Zero automatic/strict routing while preserving configurable dual-stack and DNS-hijack values',
);

console.log('cross-mode-state: ok');
