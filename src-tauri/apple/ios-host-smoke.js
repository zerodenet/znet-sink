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
const platform = await invoke('gui_mobile_platform_snapshot');
check(platform.mobile && platform.platform === 'ios', 'real-ios-rust-host');
check(!platform.vpnSupported && !platform.bundledKernelAvailable && platform.adaptationState === 'not-adapted', 'unsupported-vpn');
const identity = await invoke('plugin:app|identifier');
check(identity === 'org.zerodenet.znetsink', 'app-identity');
const licenses = await invoke('app_license_inventory');
check(licenses.native.status === 'unavailable' && licenses.native.inventory === null, 'honest-native-license-gap');
// Complete the actual onboarding using its controls; no storage seeding or UI fixture.
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
  aboutAndReturn: true, vpnUnsupported: true, nativeLicenseUnavailable: true,
  applicationId: identity, width: innerWidth, height: innerHeight };
