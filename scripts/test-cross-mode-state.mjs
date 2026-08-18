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
const tunService = read('src/lib/services/tun.ts');
const ruleOverlay = read('src-tauri/src/services/rule_overlay.rs');
const appConfigModel = read('src-tauri/src/models/app_config.rs');

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
  'Lite power must map to the client-managed Zero TUN lifecycle rather than the system-proxy lifecycle',
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
  'Pro -> Lite should establish TUN before releasing an active GUI-owned system proxy and roll back a newly established local TUN on handoff failure',
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
    && tunSettings.includes('oninput={markDirty}')
    && tunSettings.includes('checked={dualStack}')
    && tunSettings.includes('checked={dnsHijack}')
    && tunSettings.includes('已显式定义 <code>runtime.tun</code>')
    && !tunSettings.includes('autoRoute')
    && !tunSettings.includes('strictRoute'),
  'Pro TUN settings should expose local defaults, preserve dirty state, and explain when the active profile owns runtime.tun',
);

assert.ok(
  appConfigModel.includes('pub enabled: Option<bool>')
    && ruleOverlay.includes('pub(crate) fn profile_defines_tun(base: &Value) -> bool')
    && ruleOverlay.includes('tun.enabled != Some(true) || profile_defines_tun(base)')
    && ruleOverlay.includes('runtime.insert("tun".to_string(), Value::Object(value));')
    && ruleOverlay.includes('value.insert("auto_route".to_string(), json!(true));')
    && ruleOverlay.includes('value.insert("strict_route".to_string(), json!(true));'),
  'ZNet-Sink should persist a TUN desired state and inject runtime.tun only as an effective-config default when the source profile does not own it',
);

assert.ok(
  tunService.includes("updateAppConfig({ tun: { enabled: true } })")
    && tunService.includes("updateAppConfig({ tun: { enabled: false } })")
    && tunService.includes("profileManaged: true")
    && tunService.includes('profileDesiredEnabled: runtime.tun !== null')
    && tunService.includes("code: 'tun_managed_by_profile'")
    && tunService.includes("if (current.enabled && !current.managedByConfig)")
    && tunService.includes("await invoke('gui_tun_disable')"),
  'GUI TUN actions should drive persisted effective config, respect explicit profile ownership including runtime.tun:null, and retain legacy command cleanup only for migration',
);

assert.ok(
  tunRuntime.includes('pub async fn prepare_tun_enable(')
    && tunRuntime.includes('super::wintun_compat::ensure_for_current_runtime().await?;')
    && tunRuntime.includes('commands::run_command("tun.start", params, options.clone()).await?')
    && tunRuntime.includes('commands::run_command("tun.stop", json!({}), options.clone()).await?'),
  'capability-gated runtime preparation should be reusable without removing the direct tun.start/tun.stop compatibility surface',
);

assert.ok(
  guiState.includes('this.tunStatus = null;')
    && guiState.includes("await tracedOperation('kernel', 'kernel.restart', () => restartCoreProcess())")
    && guiState.indexOf('this.tunStatus = null;')
      < guiState.indexOf("await tracedOperation('kernel', 'kernel.restart', () => restartCoreProcess())"),
  'Core restart must invalidate the old TUN observation before rebuilding state from the new Core instance',
);

console.log('cross-mode-state: ok');
