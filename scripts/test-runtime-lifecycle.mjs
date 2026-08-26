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

const appRuntime = read('src-tauri/src/lib.rs');
const coreProcessService = read('src-tauri/src/services/core_process.rs');
assert.ok(
  appRuntime.includes('tauri::RunEvent::ExitRequested')
    && appRuntime.includes('core_process::shutdown_managed_runtime(cleanup_app.clone()).await')
    && coreProcessService.indexOf('crate::kernel::zero::runtime::disable_tun(Some(options))')
      < coreProcessService.indexOf('stop(stop_app.clone(), stop_state)'),
  'application exit should stop TUN before restoring the proxy and stopping the managed core process',
);

const connectionWorkspace = read('src/lib/components/tabs/ConnectionInspectorWorkspace.svelte');
const closeAllBody = connectionWorkspace.slice(
  connectionWorkspace.indexOf('async function closeAllConnections()'),
  connectionWorkspace.indexOf('function requestCloseAllConnections()'),
);
assert.ok(
  connectionWorkspace.includes('closeAllSnapshotIds = [...new Set(liveView.map(')
    && closeAllBody.includes('const ids = [...closeAllSnapshotIds];')
    && !closeAllBody.includes('liveView.map(')
    && closeAllBody.indexOf('closeAllConfirm = false;') < closeAllBody.indexOf('closingAll = true;'),
  'close-all must freeze the confirmation-time flow IDs and dismiss the dialog before processing them',
);

console.log('runtime lifecycle tests passed');
