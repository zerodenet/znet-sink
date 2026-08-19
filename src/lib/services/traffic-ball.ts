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
const TRAFFIC_BALL_SNAP_MARGIN_LOGICAL = 10;
const TRAFFIC_BALL_SNAP_THRESHOLD_LOGICAL = 32;
const TRAFFIC_BALL_POSITION_EVENT = 'traffic-ball:position';

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

  await new Promise<void>((resolve, reject) => {
    void ball.once('tauri://created', () => resolve());
    void ball.once('tauri://error', (event) => reject(new Error(String(event.payload))));
  });
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
    // Keep the window controls usable in browser/dev environments or if the
    // dedicated surface could not be created by the platform.
    await mainWindow.minimize().catch(() => {});
  }
}

export async function snapTrafficBallToEdge(
  ball: Window = getCurrentWindow(),
  position?: PhysicalPosition,
): Promise<void> {
  try {
    const monitor = await currentMonitor();
    if (!monitor) return;

    const current = position ?? await ball.outerPosition();
    const size = Math.round(TRAFFIC_BALL_SIZE_LOGICAL * monitor.scaleFactor);
    const margin = Math.round(TRAFFIC_BALL_SNAP_MARGIN_LOGICAL * monitor.scaleFactor);
    const threshold = Math.round(TRAFFIC_BALL_SNAP_THRESHOLD_LOGICAL * monitor.scaleFactor);
    const workArea = monitor.workArea;
    const left = workArea.position.x + margin;
    const right = Math.max(left, workArea.position.x + workArea.size.width - size - margin);
    const top = workArea.position.y + margin;
    const bottom = Math.max(top, workArea.position.y + workArea.size.height - size - margin);
    const leftDistance = Math.abs(current.x - left);
    const rightDistance = Math.abs(current.x - right);

    let targetX = current.x;
    if (leftDistance <= threshold || rightDistance <= threshold) {
      targetX = leftDistance <= rightDistance ? left : right;
    } else {
      return;
    }
    const targetY = Math.max(top, Math.min(bottom, current.y));

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
