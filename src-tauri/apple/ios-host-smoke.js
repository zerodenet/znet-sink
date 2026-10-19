let stage = 'bootstrap';
try {
// Explicit CI-only driver evaluated by the Debug native plugin in the real WKWebView.
// It uses the existing UI and IPC. It never supplies kernel samples or VPN success.
const wait = async (read, label) => {
  for (let i = 0; i < 180; i++) {
    const value = read(); if (value) return value;
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw Error(`ui-smoke-timeout-${label}`);
};
const check = (value, label) => { if (!value) throw Error(`ui-smoke-${label}`); };
const visible = element => element && element.getClientRects().length > 0;
const invoke = await wait(() => window.__TAURI_INTERNALS__?.invoke, 'ipc');
stage = 'platform';
const platform = await invoke('gui_mobile_platform_snapshot');
check(platform.mobile && platform.platform === 'ios', 'real-ios-rust-host');
check(!platform.vpnSupported && !platform.bundledKernelAvailable && platform.adaptationState === 'not-adapted', 'unsupported-vpn');
stage = 'identity';
const identity = await invoke('plugin:app|identifier');
check(identity === 'org.zerodenet.znetsink', 'app-identity');
let pluginHost;
stage = 'plugin-profile';
for (let i = 0; i < 60; i++) {
  pluginHost = await invoke('plugins_snapshot');
  if (pluginHost.platform?.state === 'ready') break;
  await new Promise(resolve => setTimeout(resolve, 250));
}
check(pluginHost?.platform?.executionAllowed && pluginHost.platform.target.os === 'ios', 'native-plugin-profile');
stage = 'plugin-vm';
const vm = await invoke('plugins_native_vm_smoke');
check(vm.executed && vm.arithmetic && vm.identity && vm.ungrantedRejected && vm.deadlineEnforced
  && vm.revocationEnforced && vm.target.os === 'ios', 'native-plugin-vm');
const storage = vm.secureStorage;
const storageAccepted = storage?.state === 'available';
check(storageAccepted
  ? storage.roundTrip && storage.lostKeyRejected && storage.plaintextFallbackRejected
  : storage?.state === 'unavailable' && storage.permissionsGated && storage.roundTrip === false,
  'native-secure-storage-contract');
stage = 'licenses';
const licenses = await invoke('app_license_inventory');
check(licenses.rust?.target?.platform === 'ios' && licenses.rust.entries.length > 0
  && licenses.rust.provenance?.kind !== 'inventory-unavailable', 'embedded-rust-license-inputs');
check(licenses.native.status === 'available' && licenses.native.inventory?.target?.platform === 'ios'
  && licenses.native.inventory.entries.length > 0 && Array.isArray(licenses.native.inventory.gaps), 'native-license-asset');
// Complete the actual onboarding using its controls; no storage seeding or UI fixture.
stage = 'ui';
await wait(() => document.querySelector('nav[aria-label="主导航"]') || document.querySelector('button[aria-label="步骤 4"]'), 'onboarding');
if (!document.querySelector('nav[aria-label="主导航"]')) {
  document.querySelector('button[aria-label="步骤 4"]').click();
  const start = await wait(() => [...document.querySelectorAll('button')].find(b => visible(b) && b.textContent.trim() === '开始使用'), 'start');
  start.click();
}
const nav = await wait(() => document.querySelector('nav[aria-label="主导航"]'), 'navigation');
const buttons = [...nav.querySelectorAll('button')];
check(JSON.stringify(buttons.map(b => b.textContent.trim())) === JSON.stringify(['概览', '节点', '配置', '更多']), 'four-tabs');
check(document.querySelector('.mobile-shell')?.dataset.platform === 'ios', 'native-presentation');
for (const button of buttons) {
  button.click();
  await wait(() => button.getAttribute('aria-current') === 'page', 'tab');
}
const about = await wait(() => [...document.querySelectorAll('button')].find(b => visible(b) && b.textContent.trim() === '关于'), 'about-entry');
about.click();
await wait(() => [...document.querySelectorAll('.secondary-pane:not(.inactive) h1')].some(e => e.textContent === '关于'), 'about');
await wait(() => document.body.textContent.includes('org.zerodenet.znetsink'), 'about-identity');
const back = [...document.querySelectorAll('button[aria-label="返回"]')].find(visible);
check(back, 'return-control'); back.click();
await wait(() => ![...document.querySelectorAll('.secondary-pane:not(.inactive)')].some(visible), 'return');
return { rustIpc: true, nativePresentation: true, fourTabs: true, onboarding: true,
  aboutAndReturn: true, vpnUnsupported: true, nativeLicenseAvailable: true, nativePluginVm: true,
  nativeSecureStorageContract: true, nativeSecureStorageAccepted: storageAccepted, nativeSecureStorageState: storage.state,
  applicationId: identity, width: innerWidth, height: innerHeight };

} catch (cause) {
  const message = String(cause?.message ?? "");
  const code = typeof cause?.code === 'string' && ['plugin_platform_unavailable','plugin_host_suspended','plugin_secure_store_unavailable','native_vm_smoke_failed','invalid_argument','internal','permission_denied'].includes(cause.code) ? cause.code : 'unclassified';
  const actual = typeof cause?.details?.actual === 'string' && ['PermissionDenied','Deadline','Revoked','GuestException','InvalidOutput','UnsupportedIsolation','IncompatibleDevice','IncompatibleVersion','AdmissionDenied','BudgetExceeded','HostFailure','Disabled','Busy','Cancelled','Expired','InvalidManifest','WrongHost','DigestMismatch','unexpected-value'].includes(cause.details.actual) ? cause.details.actual : undefined;
  const fixture = typeof cause?.details?.fixture === 'string' && ['arithmetic','identity','denied','deadline'].includes(cause.details.fixture) ? cause.details.fixture : undefined;
  const checks = ['real-ios-rust-host','unsupported-vpn','app-identity','native-plugin-profile','native-plugin-vm','native-secure-storage-contract','embedded-rust-license-inputs','native-license-asset','four-tabs','native-presentation','return-control'].map(label => `ui-smoke-${label}`);
  const timeouts = ['ipc','onboarding','start','navigation','tab','about-entry','about','about-identity','return'].map(label => `ui-smoke-timeout-${label}`);
  return { failedCheck: checks.includes(message) || timeouts.includes(message) ? message : `ui-smoke-${stage}-ipc`, ipcCode: code, ...(actual ? {actual} : {}), ...(fixture ? {fixture} : {}) };
}
