import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
}

const overviewData = read('src/lib/services/overview-data.svelte.ts');
const page = read('src/routes/+page.svelte');
const overview = read('src/lib/components/tabs/OverviewTab.svelte');
const guiState = read('src/lib/services/gui-state.svelte.ts');
const runtimePerformance = read('src/lib/components/RuntimePerformance.svelte');

assert.ok(
  overviewData.includes('captureSessionActive = $state(false)') &&
    overviewData.includes('captureSessionUpBytes = $state(0)') &&
    overviewData.includes('captureSessionDownBytes = $state(0)') &&
    overviewData.includes('get captureSessionTotalBytes()'),
  'capture-session traffic should be tracked separately from core lifetime totals',
);
assert.ok(
  overviewData.includes('this._sessionLastTotalUp = this.totalUpBytes') &&
    overviewData.includes('this._sessionLastTotalDown = this.totalDownBytes') &&
    overviewData.includes('this._sessionHasBaseline = this.isLive') &&
    overviewData.includes('if (!this._sessionHasBaseline)') &&
    overviewData.includes('this.applyCaptureSessionCounters(totalUp, totalDown)'),
  'a new capture session should baseline against a real core counter and let the first real sample establish it when necessary',
);
assert.ok(
  overviewData.includes('totalUp >= this._sessionLastTotalUp') &&
    overviewData.includes('? totalUp - this._sessionLastTotalUp') &&
    overviewData.includes(': totalUp') &&
    overviewData.includes('totalDown >= this._sessionLastTotalDown') &&
    overviewData.includes(': totalDown'),
  'counter resets during a core restart should start a new counter epoch without resetting the GUI session total',
);
assert.ok(
  page.includes('const captureEnabled = guiState.isCaptureEnabled') &&
    guiState.includes('get isCaptureEnabled(): boolean') &&
    guiState.includes('return this.isTunEnabled || this.connection?.systemProxyEnabled === true;') &&
    page.includes('overviewData.beginCaptureSession()') &&
    page.includes('overviewData.endCaptureSession()') &&
    !page.includes('const proxyEnabled = guiState.isSystemProxyEnabled'),
  'one mode-neutral session should span system-proxy and TUN capture until both are disabled',
);
assert.ok(
  overview.includes('class="lite-power-orbit"') &&
    overview.includes('class="lite-traffic-ring"') &&
    overview.includes('style={sessionRingStyle}') &&
    overview.includes('background: conic-gradient(') &&
    overview.includes('from 180deg') &&
    overview.includes('--traffic-up-share') &&
    overview.includes('sessionTotalLabel'),
  'Lite Overview should render one closed CSS conic-gradient ring around the power switch with the session total above it',
);
assert.ok(
  overview.includes('total <= 0) return 50') &&
    overview.includes('trafficShare(overviewData.captureSessionUpBytes, sessionTotalBytes)') &&
    overview.includes('const sessionDownShare = $derived(100 - sessionUpShare)') &&
    !overview.includes('<svg class="lite-traffic-ring"') &&
    !overview.includes('stroke-dasharray={sessionUpDash}') &&
    !overview.includes('stroke-dasharray={sessionDownDash}') &&
    !overview.includes('stroke-dashoffset={sessionDownOffset}'),
  'an empty session should start 50/50 and the traffic ring must not regress to SVG dash segments that can expose visual gaps',
);
assert.ok(
  overview.includes('class="lite-traffic-totals"') &&
    overview.includes('class="lite-total-up lite-metric-help"') &&
    overview.includes('class="lite-total-down lite-metric-help"') &&
    overview.includes('sessionUpLabel') &&
    overview.includes('sessionDownLabel'),
  'the values beside the ring should be cumulative upload/download totals that match the ring composition',
);
assert.ok(
  overview.includes('class="lite-live-rates"') &&
    overview.includes('class="lite-live-up lite-metric-help"') &&
    overview.includes('class="lite-live-down lite-metric-help"') &&
    overview.includes('formatSpeed(currentUp)') &&
    overview.includes('formatSpeed(currentDown)'),
  'real-time upload/download speed should remain visible in a fixed row below the ring instead of replacing cumulative totals',
);
assert.ok(
  overview.includes('data-tooltip={guiState.supportsTrafficStats ? `本次总流量 ${sessionTotalLabel}`') &&
    overview.includes('data-tooltip={guiState.supportsTrafficStats ? `本次上传 ${sessionUpLabel}`') &&
    overview.includes('data-tooltip={guiState.supportsTrafficStats ? `本次下载 ${sessionDownLabel}`') &&
    overview.includes('data-tooltip={guiState.supportsTrafficStats ? `实时上传速率 ${formatSpeed(currentUp)}`') &&
    overview.includes('data-tooltip={guiState.supportsTrafficStats ? `实时下载速率 ${formatSpeed(currentDown)}`') &&
    overview.includes('<span class="sr-only">本次总流量：</span>') &&
    overview.includes('<span class="sr-only">本次上传：</span>') &&
    overview.includes('<span class="sr-only">本次下载：</span>') &&
    overview.includes('<span class="sr-only">实时上传速率：</span>') &&
    overview.includes('<span class="sr-only">实时下载速率：</span>') &&
    !overview.includes('本次代理会话总流量 ${sessionTotalLabel}，上传 ${sessionUpLabel}，下载 ${sessionDownLabel}'),
  'each Lite traffic metric should expose its own hover hint and screen-reader label instead of one aggregated explanation',
);
assert.ok(
  overview.includes('.lite-metric-help {') &&
    overview.includes('pointer-events: auto') &&
    overview.includes('content: attr(data-tooltip)') &&
    overview.includes('.lite-metric-help:hover::after'),
  'metric hover hints should remain individually hit-testable even when their layout container ignores pointer events',
);
assert.ok(
  overview.includes('class:flowing={currentUp > 0.001 || currentDown > 0.001}') &&
    overview.includes('@media (prefers-reduced-motion: reduce)'),
  'kernel real-time rates should drive the display without depending on either capture toggle',
);
assert.ok(
  overview.includes('const systemProxyEnabled = $derived(guiState.isSystemProxyEnabled)') &&
    overview.includes('const captureEnabled = $derived(guiState.isCaptureEnabled)') &&
    overview.includes('const liteConnected = $derived(guiState.isConnected)') &&
    overview.includes('class:on={liteConnected}') &&
    overview.includes('onclick={() => liteConnected ? guiState.disconnect() : guiState.connect()}') &&
    !overview.includes('supportsTrafficStats && proxyEnabled ? formatSpeed'),
  'system-proxy status, any-capture activity, Lite combined power and kernel rates should keep distinct semantics',
);
assert.ok(
  !overview.includes('{#if proxyEnabled && guiState.supportsTrafficStats}') &&
    !overview.includes('class="lite-session-breakdown"') &&
    !overview.includes('<span class="lite-session-label">'),
  'session traffic must not be conditionally inserted as a textual row that shifts the Lite layout',
);
assert.ok(
  !overviewData.includes('captureSessionUpBytes += upRate') &&
    !overviewData.includes('captureSessionDownBytes += downRate'),
  'session totals must come from core byte counters, never by integrating displayed rates',
);
assert.ok(
  runtimePerformance.includes('const coreRuntime = $derived(snapshot?.core ?? null)') &&
    runtimePerformance.includes('formatCpu(coreRuntime?.cpuPercent)') &&
    runtimePerformance.includes('formatMemory(coreRuntime?.memoryBytes)') &&
    !runtimePerformance.includes('formatCpu(snapshot?.totalCpuPercent)') &&
    !runtimePerformance.includes('formatMemory(snapshot?.totalMemoryBytes)'),
  'Lite and Pro CPU/memory displays should both report the managed Zero process instead of mixing process boundaries',
);

console.log('lite-session-traffic: ok');
