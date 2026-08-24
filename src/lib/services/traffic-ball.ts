import { LogicalSize, PhysicalPosition } from '@tauri-apps/api/dpi';
import { emit, emitTo, listen } from '@tauri-apps/api/event';
import {
  currentMonitor,
  getAllWindows,
  getCurrentWindow,
  monitorFromPoint,
  type Window,
} from '@tauri-apps/api/window';

const MAIN_WINDOW_LABEL = 'main';
const TRAFFIC_BALL_LABEL = 'traffic-ball';
const TRAFFIC_BALL_SIZE_LOGICAL = 128;
const TRAFFIC_BALL_MARGIN_LOGICAL = 14;
const TRAFFIC_BALL_SNAP_GAP_LOGICAL = 6;
const TRAFFIC_BALL_SNAP_THRESHOLD_LOGICAL = 48;
const TRAFFIC_BALL_CREATE_TIMEOUT_MS = 4_000;
const TRAFFIC_BALL_POSITION_EVENT = 'traffic-ball:position';
const TRAFFIC_BALL_CREATE_REQUEST_EVENT = 'traffic-ball:create-request';
const TRAFFIC_BALL_READY_EVENT = 'traffic-ball:ready';
const TRAFFIC_BALL_DESTROY_REQUEST_EVENT = 'traffic-ball:destroy-request';
export const TRAFFIC_BALL_SHOWN_EVENT = 'traffic-ball:shown';
export const TRAFFIC_BALL_HIDDEN_EVENT = 'traffic-ball:hidden';

type SavedPosition = { x: number; y: number };
type TrafficBallReady = { ok: boolean; error?: string };

let savedPosition: SavedPosition | null = null;
let positionListenerInstalled = false;
let showTransition: Promise<void> | null = null;

async function getWindowByLabel(label: string): Promise<Window | null> {
  const windows = await getAllWindows();
  return windows.find((window) => window.label === label) ?? null;
}

async function requestTrafficBallWindow(): Promise<Window> {
  const existing = await getWindowByLabel(TRAFFIC_BALL_LABEL);
  if (existing) return existing;

  await new Promise<void>((resolve, reject) => {
    let settled = false;
    let unlisten: (() => void) | null = null;
    const timer = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      unlisten?.();
      reject(new Error('traffic-ball window creation timed out'));
    }, TRAFFIC_BALL_CREATE_TIMEOUT_MS);

    const settle = (callback: () => void) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      unlisten?.();
      callback();
    };

    void listen<TrafficBallReady>(TRAFFIC_BALL_READY_EVENT, (event) => {
      if (event.payload?.ok) settle(() => resolve());
      else settle(() => reject(new Error(event.payload?.error ?? 'traffic-ball window creation failed')));
    }).then((dispose) => {
      if (settled) {
        dispose();
        return;
      }
      unlisten = dispose;
      return emit(TRAFFIC_BALL_CREATE_REQUEST_EVENT);
    }).catch((error) => {
      settle(() => reject(error));
    });
  });

  const created = await getWindowByLabel(TRAFFIC_BALL_LABEL);
  if (!created) throw new Error('traffic-ball window was not registered after creation');
  return created;
}

async function destroyTrafficBallWindow(): Promise<void> {
  await emit(TRAFFIC_BALL_DESTROY_REQUEST_EVENT).catch(() => {});
}

async function ensurePositionListener(): Promise<void> {
  if (positionListenerInstalled || getCurrentWindow().label !== MAIN_WINDOW_LABEL) return;
  positionListenerInstalled = true;
  try {
    await listen<SavedPosition>(TRAFFIC_BALL_POSITION_EVENT, (event) => {
      if (Number.isFinite(event.payload?.x) && Number.isFinite(event.payload?.y)) {
        savedPosition = { x: event.payload.x, y: event.payload.y };
      }
    });
  } catch {
    positionListenerInstalled = false;
  }
}

async function rememberPosition(ball: Window): Promise<void> {
  try {
    const position = await ball.outerPosition();
    await emitTo(MAIN_WINDOW_LABEL, TRAFFIC_BALL_POSITION_EVENT, {
      x: position.x,
      y: position.y,
    } satisfies SavedPosition);
  } catch {
    // Position persistence is a convenience and must never block restore.
  }
}

async function resolveTrafficBallPosition(): Promise<PhysicalPosition> {
  const savedMonitor = savedPosition
    ? await monitorFromPoint(savedPosition.x + 1, savedPosition.y + 1).catch(() => null)
    : null;
  const monitor = savedMonitor ?? await currentMonitor();

  if (!monitor) {
    return new PhysicalPosition(savedPosition?.x ?? 24, savedPosition?.y ?? 24);
  }

  const size = Math.round(TRAFFIC_BALL_SIZE_LOGICAL * monitor.scaleFactor);
  const margin = Math.round(TRAFFIC_BALL_MARGIN_LOGICAL * monitor.scaleFactor);
  const workArea = monitor.workArea;
  const minX = workArea.position.x + margin;
  const minY = workArea.position.y + margin;
  const maxX = Math.max(minX, workArea.position.x + workArea.size.width - size - margin);
  const maxY = Math.max(minY, workArea.position.y + workArea.size.height - size - margin);

  if (savedPosition && savedMonitor) {
    return new PhysicalPosition(
      Math.max(minX, Math.min(maxX, savedPosition.x)),
      Math.max(minY, Math.min(maxY, savedPosition.y)),
    );
  }

  return new PhysicalPosition(maxX, maxY);
}

async function performShowTrafficBall(mainWindow: Window): Promise<void> {
  try {
    await ensurePositionListener();
    const ball = await requestTrafficBallWindow();

    // Re-assert a square inner viewport before every show. The window config
    // declares 128x128, while this also protects against desktop/window-manager
    // client-size restoration differences after native creation.
    await ball.setSize(new LogicalSize(TRAFFIC_BALL_SIZE_LOGICAL, TRAFFIC_BALL_SIZE_LOGICAL));

    const position = await resolveTrafficBallPosition();
    await ball.setPosition(position);
    await ball.show();
    await emitTo(TRAFFIC_BALL_LABEL, TRAFFIC_BALL_SHOWN_EVENT);
    await mainWindow.hide();
  } catch {
    // Preserve a usable fallback if the floating window cannot be created on
    // a platform: keep main usable and fall back to normal minimization.
    await mainWindow.show().catch(() => {});
    await mainWindow.minimize().catch(() => {});
  }
}

export function showTrafficBall(mainWindow: Window = getCurrentWindow()): Promise<void> {
  if (showTransition) return showTransition;
  const transition = performShowTrafficBall(mainWindow).finally(() => {
    if (showTransition === transition) showTransition = null;
  });
  showTransition = transition;
  return transition;
}

export async function snapTrafficBallToEdge(
  ball: Window = getCurrentWindow(),
  position?: PhysicalPosition,
): Promise<void> {
  try {
    const current = position ?? await ball.outerPosition();
    const outerSize = await ball.outerSize();
    const centerX = current.x + outerSize.width / 2;
    const centerY = current.y + outerSize.height / 2;
    const monitor = await monitorFromPoint(centerX, centerY).catch(() => null)
      ?? await currentMonitor();
    if (!monitor) return;

    const scale = monitor.scaleFactor;
    const gap = Math.round(TRAFFIC_BALL_SNAP_GAP_LOGICAL * scale);
    const threshold = Math.round(TRAFFIC_BALL_SNAP_THRESHOLD_LOGICAL * scale);
    const workArea = monitor.workArea;
    const workLeft = workArea.position.x;
    const workTop = workArea.position.y;
    const workRight = workLeft + workArea.size.width;
    const workBottom = workTop + workArea.size.height;

    const leftGap = current.x - workLeft;
    const rightGap = workRight - (current.x + outerSize.width);
    const nearLeft = Math.abs(leftGap) <= threshold;
    const nearRight = Math.abs(rightGap) <= threshold;
    if (!nearLeft && !nearRight) return;

    const leftX = workLeft + gap;
    const rightX = workRight - outerSize.width - gap;
    const targetX = nearLeft && nearRight
      ? (Math.abs(leftGap) <= Math.abs(rightGap) ? leftX : rightX)
      : nearLeft ? leftX : rightX;
    const minY = workTop + gap;
    const maxY = Math.max(minY, workBottom - outerSize.height - gap);
    const targetY = Math.max(minY, Math.min(maxY, current.y));

    if (Math.abs(targetX - current.x) < 1 && Math.abs(targetY - current.y) < 1) return;
    await ball.setPosition(new PhysicalPosition(targetX, targetY));
  } catch {
    // Window managers vary in positioning support; dragging must remain usable
    // even when edge snapping is unavailable.
  }
}

export async function hideTrafficBall(): Promise<void> {
  const ball = await getWindowByLabel(TRAFFIC_BALL_LABEL);
  if (!ball) return;
  await rememberPosition(ball);
  await emitTo(TRAFFIC_BALL_LABEL, TRAFFIC_BALL_HIDDEN_EVENT).catch(() => {});
  await ball.hide().catch(() => {});
  await destroyTrafficBallWindow();
}

export async function restoreMainWindow(): Promise<void> {
  const main = await getWindowByLabel(MAIN_WINDOW_LABEL);
  const ball = await getWindowByLabel(TRAFFIC_BALL_LABEL);

  if (ball) await rememberPosition(ball);
  if (main) {
    await main.show();
    await main.setFocus().catch(() => {});
  }
  if (ball) {
    await emitTo(TRAFFIC_BALL_LABEL, TRAFFIC_BALL_HIDDEN_EVENT).catch(() => {});
    await ball.hide().catch(() => {});
    await destroyTrafficBallWindow();
  }
}
