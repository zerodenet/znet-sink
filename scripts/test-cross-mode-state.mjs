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
const appConfigCommand = read('src-tauri/src/commands/app_config.rs');
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
    && configService.includes("invoke<SubscriptionSyncAllOutcome>('subscription_sync_all'")
    && configService.includes('prepareGuiTunForProfileSwitch(target.content)')
    && configService.includes("reconcileTunAfterConfigMutation('profile activation')")
    && configService.includes('restoreGuiTunAfterFailedProfileSwitch(transition)'),
  'profile activation/subscription updates must invalidate config-backed surfaces and reconcile client-owned TUN around source changes',
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
  guiState.includes("tracedOperation('proxy', 'lite.system_proxy.enable', () => guiConnect())")
    && guiState.includes("tracedOperation('proxy', 'tun.enable', () => enableGuiTun())")
    && guiState.includes("tracedOperation('proxy', 'tun.disable', () => disableGuiTun())")
    && guiState.includes("tracedOperation('proxy', 'lite.system_proxy.disable', () => guiDisconnect())")
    && guiState.includes('get isConnected(): boolean')
    && guiState.includes('return this.isTunEnabled && this.connection?.systemProxyEnabled === true;')
    && guiState.includes('get isSystemProxyEnabled(): boolean')
    && guiState.includes('return this.connection?.systemProxyEnabled === true;'),
  'Lite power must require both the GUI-owned system proxy and the client-managed Zero TUN while the public system-proxy state reflects actual OS proxy ownership',
);

const liteConnectProxy = guiState.indexOf("tracedOperation('proxy', 'lite.system_proxy.enable', () => guiConnect())");
const liteConnectTun = guiState.indexOf("tracedOperation('proxy', 'tun.enable', () => enableGuiTun())", liteConnectProxy);
const liteDisconnectTun = guiState.indexOf("tracedOperation('proxy', 'tun.disable', () => disableGuiTun())");
const liteDisconnectProxy = guiState.indexOf("tracedOperation('proxy', 'lite.system_proxy.disable', () => guiDisconnect())", liteDisconnectTun);
assert.ok(
  liteConnectProxy >= 0
    && liteConnectTun > liteConnectProxy
    && liteDisconnectTun >= 0
    && liteDisconnectProxy > liteDisconnectTun,
  'Lite power-on should establish system proxy before TUN and power-off should stop TUN before releasing the system proxy',
);

const initialSnapshot = guiState.indexOf('await this.refreshAll();');
const initializationUnlock = guiState.indexOf('this.isInitializing = false;', initialSnapshot);
const autoConnect = guiState.indexOf('await this.autoConnectForMode(', initializationUnlock);
assert.ok(
  initialSnapshot >= 0 && initializationUnlock > initialSnapshot && autoConnect > initializationUnlock,
  'Lite default auto-connect should run only after the first trusted snapshot and after UI action guards are unlocked',
);

const handoffSystemProxy = guiState.indexOf("tracedOperation('proxy', 'lite.system_proxy.handoff', () => guiConnect())");
const handoffTun = guiState.indexOf("tracedOperation('proxy', 'lite.tun.handoff', () => enableGuiTun())");
assert.ok(
  guiState.includes('async prepareLiteCapture()')
    && guiState.includes('const systemProxyOwned = this.connection?.systemProxyEnabled === true')
    && guiState.includes('const tunEnabled = this.isTunEnabled')
    && guiState.includes('if (!systemProxyOwned && !tunEnabled) return;')
    && guiState.includes('if (systemProxyOwned && tunEnabled)')
    && handoffSystemProxy >= 0
    && handoffTun >= 0
    && !guiState.includes('lite.system_proxy.release')
    && guiState.includes('if (tunStarted)')
    && guiState.includes('if (systemProxyStarted)'),
  'Pro -> Lite should preserve an existing capture path, reconcile the missing side, and never release a working system proxy merely because TUN is active',
);

assert.ok(
  appStore.includes("const PRO_ONLY_SETTINGS = new Set<SettingsSection>(['tun', 'config'])")
    && appStore.includes("if (mode === 'lite' && guiState.isCaptureEnabled)")
    && appStore.includes('void this.prepareLiteCaptureInBackground(generation)')
    && appStore.includes("if (this.uiMode === 'lite' && !LITE_MODE_NAV.has(key)) return false;")
    && settingsPanel.includes("section.id !== 'config' && section.id !== 'tun'")
    && settingsPanel.includes("activeSection === 'config' || activeSection === 'tun'"),
  'TUN configuration and navigation must switch to Lite synchronously while capture reconciliation runs in the background',
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
    && tunSettings.includes('<span class="label-text">DNS 劫持</span>')
    && tunSettings.includes('dnsHijack,')
    && tunSettings.includes('已显式定义 <code>runtime.tun</code>')
    && !tunSettings.includes('autoRoute')
    && !tunSettings.includes('strictRoute'),
  'Pro TUN settings should expose local defaults and persist DNS hijack only through the completed DNS configuration surface',
);

assert.ok(
  appConfigModel.includes('pub enabled: Option<bool>')
    && !ruleOverlay.includes('apply_tun_default(')
    && !ruleOverlay.includes('runtime.insert("tun".to_string()')
    && !appConfigCommand.includes('tun_defaults_changed')
    && !appConfigCommand.includes('tun_patch_requested'),
  'persisted local TUN desired state must stay outside the effective Zero configuration and app-config updates must not trigger TUN config.apply',
);

assert.ok(
  tunService.includes("updateAppConfig({ tun: { enabled: true } })")
    && tunService.includes("updateAppConfig({ tun: { enabled: false } })")
    && tunService.includes("await invoke('gui_tun_enable')")
    && tunService.includes("await invoke('gui_tun_disable')")
    && tunService.includes('export async function reconcileGuiTunRuntime()')
    && tunService.includes('export async function prepareGuiTunForProfileSwitch(content: unknown)')
    && tunService.includes('restoreGuiTunAfterFailedProfileSwitch')
    && tunService.includes('if (desired === undefined)')
    && tunService.includes('profileDesiredEnabled: runtime.tun !== null')
    && tunService.includes("code: 'tun_managed_by_profile'"),
  'GUI TUN actions must persist desired state while controlling app-owned TUN through tun.start/tun.stop and handing profile ownership off explicitly',
);

assert.ok(
  tunService.includes('async function waitForTunStateAfterTransientIpcError(')
    && tunService.includes("code === 'timeout' || code === 'connection_closed' || code === 'core_unavailable'")
    && tunService.includes('const reconciled = await waitForTunStateAfterTransientIpcError(true, error);')
    && tunService.includes('const reconciled = await waitForTunStateAfterTransientIpcError(false, error);'),
  'TUN commands must reconcile authoritative runtime state before treating a late IPC response as failure',
);

assert.ok(
  tunService.includes('async function validateAppDnsHijackPrecondition(policy: TunPolicy): Promise<void>')
    && tunService.includes('const readiness = await inspectTunDnsHijackReadiness(policy.appConfig.dns);')
    && tunService.includes("features?.tunDnsSystemAuto.state === 'unsupported'")
    && tunService.includes("code: 'tun_dns_hijack_requires_dns'")
    && tunService.includes('Object.keys(dns.config.servers).length === 0')
    && tunRuntime.includes('params.insert("dns_hijack".to_string(), json!(tun.dns_hijack));')
    && tunRuntime.includes('assert_eq!(params["dns_hijack"], true);'),
  'app-owned DNS hijack must require a saved DNS profile and pass the explicit value to tun.start',
);

assert.ok(
  !tunRuntime.includes('pub async fn prepare_tun_enable(')
    && tunRuntime.includes('let status = tun_status(options.clone()).await?;')
    && tunRuntime.includes('super::wintun_compat::ensure_for_current_runtime().await?;')
    && tunRuntime.includes('commands::run_command("tun.start", params, options.clone()).await?')
    && tunRuntime.includes('commands::run_command("tun.stop", json!({}), options.clone()).await?'),
  'app-owned TUN must prepare capability/Wintun immediately on the direct tun.start/tun.stop command path, without a declarative-config shim',
);

assert.ok(
  coreProcessCommand.includes('restore_app_tun_after_core_transition')
    && coreProcessCommand.includes('app_config.tun.enabled != Some(true)')
    && coreProcessCommand.includes('active_profile_defines_tun(state)?')
    && coreProcessCommand.includes('zero::runtime::enable_tun(app_config.tun.clone(), Some(options)).await?'),
  'managed Core start/restart must replay persisted app-owned TUN only when the active profile does not own runtime.tun',
);

assert.ok(
  guiState.includes('this.tunStatus = null;')
    && guiState.includes("await tracedOperation('kernel', 'kernel.restart', () => restartCoreProcess())")
    && guiState.indexOf('this.tunStatus = null;')
      < guiState.indexOf("await tracedOperation('kernel', 'kernel.restart', () => restartCoreProcess())"),
  'Core restart must invalidate the old TUN observation before rebuilding state from the new Core instance',
);

console.log('cross-mode-state: ok');
