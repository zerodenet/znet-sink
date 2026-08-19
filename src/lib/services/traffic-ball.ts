import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { emitTo, listen } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import {
  currentMonitor,
  getAllWindows,
  getCurrentWindow,
  monitorFromPoint,
  type Window,
} from '@tauri-apps/api/window';

const MAIN_WINDOW_LABEL = 'main';
const TRAFFIC_BALL_LABEL = 'traffic-ball';
const TRAFFIC_BALL_SIZE_LOGICAL = 112;
const TRAFFIC_BALL_MARGIN_LOGICAL = 18;
const TRAFFIC_BALL_SNAP_GAP_LOGICAL = 6;
const TRAFFIC_BALL_SNAP_THRESHOLD_LOGICAL = 48;
const TRAFFIC_BALL_POSITION_EVENT = 'traffic-ball:position';
export const TRAFFIC_BALL_READY_EVENT = 'traffic-ball:ready';

type SavedPosition = { x: number; y: number };

let savedPosition: SavedPosition | null = null;
let positionListenerInstalled = false;

async function getWindowByLabel(label: string): Promise<Window | null> {
  const windows = await getAllWindows();
  return windows.find((window) => window.label === label) ?? null;
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

async function createTrafficBall(): Promise<WebviewWindow> {
  let ready = false;
  let resolveReady: (() => void) | null = null;
  const readyPromise = new Promise<void>((resolve) => {
    resolveReady = resolve;
  });
  const unlistenReady = await listen(TRAFFIC_BALL_READY_EVENT, () => {
    ready = true;
    resolveReady?.();
  });

  const ball = new WebviewWindow(TRAFFIC_BALL_LABEL, {
    url: '/traffic-ball',
    width: TRAFFIC_BALL_SIZE_LOGICAL,
    height: TRAFFIC_BALL_SIZE_LOGICAL,
    decorations: false,
    transparent: true,
    visible: false,
    resizable: false,
    maximizable: false,
    minimizable: false,
    closable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    shadow: false,
  });

  try {
    await new Promise<void>((resolve, reject) => {
      void ball.once('tauri://created', () => resolve());
      void ball.once('tauri://error', (event) => reject(new Error(String(event.payload))));
    });

    // The page makes the WebView layer transparent before emitting ready.
    // Keep the native window hidden until then so Windows never exposes the
    // default WebView background as a square flash around the circle.
    if (!ready) {
      await Promise.race([
        readyPromise,
        new Promise<void>((resolve) => window.setTimeout(resolve, 1_200)),
      ]);
    }
  } finally {
    unlistenReady();
  }

  return ball;
}

export async function showTrafficBall(mainWindow: Window = getCurrentWindow()): Promise<void> {
  try {
    await ensurePositionListener();
    const position = await resolveTrafficBallPosition();
    const existing = await getWindowByLabel(TRAFFIC_BALL_LABEL);
    const ball = existing ?? await createTrafficBall();
    await ball.setPosition(position);
    await ball.show();
    await mainWindow.hide();
  } catch {
    // Preserve a usable fallback even if a platform refuses the floating
    // WebView: restore the main window before falling back to taskbar minimize.
    await mainWindow.show().catch(() => {});
    await mainWindow.minimize().catch(() => {});
  }
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
      ?? await ball.currentMonitor();
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

export async function destroyTrafficBall(): Promise<void> {
  const ball = await getWindowByLabel(TRAFFIC_BALL_LABEL);
  if (!ball) return;
  await rememberPosition(ball);
  await ball.destroy().catch(() => {});
}

export async function restoreMainWindow(): Promise<void> {
  const main = await getWindowByLabel(MAIN_WINDOW_LABEL);
  const ball = await getWindowByLabel(TRAFFIC_BALL_LABEL);

  if (ball) await rememberPosition(ball);
  if (main) {
    await main.show();
    await main.setFocus().catch(() => {});
  }
  await ball?.destroy().catch(() => {});
}
