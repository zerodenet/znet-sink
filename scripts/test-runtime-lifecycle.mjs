import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
}

const trafficBallPage = read('src/routes/traffic-ball/+page.svelte');
const trafficBallService = read('src/lib/services/traffic-ball.ts');
const trafficSampler = read('src-tauri/src/services/traffic_sampler.rs');
const coreEventsService = read('src/lib/services/core-events.svelte.ts');
const tauriConfig = JSON.parse(read('src-tauri/tauri.conf.json'));
const trafficBallWindow = tauriConfig.app.windows.find((window) => window.label === 'traffic-ball');

assert.deepEqual(
  {
    resizable: trafficBallWindow?.resizable,
    minWidth: trafficBallWindow?.minWidth,
    minHeight: trafficBallWindow?.minHeight,
    maxWidth: trafficBallWindow?.maxWidth,
    maxHeight: trafficBallWindow?.maxHeight,
  },
  { resizable: false, minWidth: 64, minHeight: 64, maxWidth: 256, maxHeight: 256 },
  'the floating traffic ball should use bounded in-surface size controls instead of native edge resizing',
);
assert.ok(
  trafficBallPage.includes('stepTrafficBallSize(direction, getCurrentWindow())')
    && trafficBallPage.includes('class="traffic-size-button"')
    && !trafficBallPage.includes('startResizeDragging(')
    && !trafficBallPage.includes('ballWindow.onResized(')
    && trafficBallPage.includes('surfaceVisible = true;')
    && trafficBallService.includes('TRAFFIC_BALL_MIN_SIZE_LOGICAL = 64')
    && trafficBallService.includes('TRAFFIC_BALL_MAX_SIZE_LOGICAL = 256')
    && trafficBallService.includes('TRAFFIC_BALL_SIZE_STORAGE_KEY'),
  'traffic-ball resizing should stay square, persist the selected size, and avoid the first-show listener race',
);
assert.ok(
  !trafficBallService.includes('traffic-ball:destroy-request')
    && !trafficSampler.includes('.destroy()')
    && trafficBallService.includes('await ball.hide().catch(() => {});'),
  'the lazily-created traffic-ball WebView should be hidden and reused without an ACL-gated destroy race',
);
assert.ok(
  trafficSampler.includes('TRAFFIC_RATE_SAMPLE_EVENT')
    && trafficSampler.includes('app.emit_to("main", TRAFFIC_RATE_SAMPLE_EVENT')
    && trafficSampler.includes('app.emit_to(TRAFFIC_BALL_LABEL, TRAFFIC_RATE_SAMPLE_EVENT')
    && coreEventsService.includes('overviewData.applyTrafficRateSample(event.payload)')
    && trafficBallPage.includes("listen<TrafficRateSample>('traffic:rate-sampled'")
    && !trafficBallPage.includes("listen<Record<string, unknown>>('traffic.sampled'"),
  'overview and traffic ball must consume the same Rust-calculated rate sample',
);

const appRuntime = read('src-tauri/src/application/mod.rs');
const tauriManifest = read('src-tauri/Cargo.toml');
const coreProcessService = read('src-tauri/src/runtime_host/shutdown.rs');
const tunShutdown = read('src-tauri/src/capture/shutdown.rs');
const lifecyclePhases = read('src-tauri/src/lifecycle/phases.rs');
assert.ok(
  tauriManifest.includes('tauri-plugin-single-instance = "2"')
    && appRuntime.includes('.plugin(tauri_plugin_single_instance::init(')
    && appRuntime.indexOf('.plugin(tauri_plugin_single_instance::init(')
      < appRuntime.indexOf('.manage(app_state)'),
  'single-instance rejection must run before managed state setup so a second GUI cannot disable proxy capture owned by the first GUI',
);
assert.ok(
  !lifecyclePhases.includes('system_proxy_guard::cleanup_on_startup();')
    && appRuntime.indexOf('system_proxy_guard::cleanup_on_startup();') > appRuntime.indexOf('.setup('),
  'stale proxy cleanup must wait until setup admits the sole client instance',
);
assert.ok(
  appRuntime.includes('tauri::RunEvent::ExitRequested')
    && appRuntime.includes('core_process::shutdown_managed_runtime(cleanup_app.clone()).await')
    && coreProcessService.indexOf('crate::capture::shutdown::stop_owned_tun(pid, endpoint)')
      < coreProcessService.indexOf('super::stop::stop_for_exit('),
  'application exit should stop TUN before clearing the proxy and stopping the managed core process',
);
assert.ok(
  tunShutdown.includes('next.tun.enabled = Some(false);')
    && coreProcessService.includes('if let Err(error) = saved_intent')
    && coreProcessService.indexOf('persist_exit_tun_intent(&stop_app);')
      < coreProcessService.indexOf('state.runtime_host().run("shutdown"'),
  'exit must persist TUN OFF even without an owned child, while storage failure must not abort runtime cleanup',
);

const exitCoordinator = read('src-tauri/src/application/shutdown.rs');
const nativeExit = appRuntime.slice(appRuntime.indexOf('shutdown::complete_final_exit('), appRuntime.indexOf('if let tauri::RunEvent::ExitRequested'));
const requestedExit = appRuntime.slice(appRuntime.indexOf('if let tauri::RunEvent::ExitRequested'));
for (const path of [nativeExit, requestedExit]) {
  assert.ok(
    path.includes('shutdown::prepare(app_handle)')
      && path.indexOf('shutdown::prepare(app_handle)') < path.indexOf('shutdown::drain_plugins(plugin_executor).await')
      && path.indexOf('shutdown::drain_plugins(plugin_executor).await') < path.indexOf('core_process::shutdown_managed_runtime('),
    'both native and requested exits must seal and drain plugin entry points before runtime teardown',
  );
}
assert.ok(
  exitCoordinator.indexOf('shutting_down_handle().store(true') < exitCoordinator.indexOf('close_page_documents()')
    && exitCoordinator.indexOf('close_page_documents()') < exitCoordinator.indexOf('executor.close(std::time::Duration::ZERO)'),
  'shutdown preparation must block watchdogs, close trusted page channels, and seal new guest calls synchronously',
);

const connectionWorkspace = read('src/lib/components/tabs/ConnectionInspectorWorkspace.svelte');
const coreStatusCard = read('src/lib/components/core/CoreStatusCard.svelte');
const flowCloseOwner = read('src-tauri/src/services/flow_close.rs');
const flowCloseClient = read('src/lib/features/flow-close/client.ts');
assert.ok(
  connectionWorkspace.includes("closeWorkspace?.begin('list')")
    && connectionWorkspace.includes("closeWorkspace?.begin('legacy-row')")
    && !connectionWorkspace.includes('Promise.allSettled')
    && !connectionWorkspace.includes('guiCloseConnection(')
    && !connectionWorkspace.includes('await closeFlow(')
    && flowCloseClient.includes('gui_flow_close_preview')
    && flowCloseClient.includes('gui_flow_close_submit')
    && flowCloseOwner.includes('subscription: Subscription')
    && flowCloseOwner.includes('t.flow_id == request.flow_id && t.target_key == request.target_key'),
  'connection controls must use host-owned canonical selection and a fixed original plan, without fulfilled counting or low-level fallback',
);

const connectionDrawer = read('src/lib/components/ConnectionDetailsDrawer.svelte');
assert.ok(
  connectionWorkspace.includes('void store.refreshInteractionSurface();')
    && connectionWorkspace.includes("{#if connection.origin === 'active'}")
    && connectionWorkspace.includes('disabled={!canCloseConnections}')
    && connectionWorkspace.includes('terminateUnavailableReason={closeUnavailableReason}')
    && !connectionWorkspace.includes('group-hover:opacity-100')
    && connectionDrawer.includes("{#if connection.origin === 'active'}")
    && connectionDrawer.includes('disabled={terminating || !canTerminate}')
    && connectionDrawer.includes("terminateUnavailableReason ?? '当前内核未提供连接终止能力。'"),
  'active connections must expose a visible terminate action and explain unavailable kernel capability',
);
assert.ok(
  coreStatusCard.includes('关闭系统代理只撤销系统流量入口，不会停止内核或终止已有连接'),
  'system-proxy control should explain that it does not stop the kernel or close existing flows',
);

console.log('runtime lifecycle tests passed');
