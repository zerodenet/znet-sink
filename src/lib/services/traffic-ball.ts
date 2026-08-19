import { PhysicalPosition } from '@tauri-apps/api/dpi';
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
const TRAFFIC_BALL_POSITION_KEY = 'znet-traffic-ball-position-v1';

type SavedPosition = { x: number; y: number };

async function getWindowByLabel(label: string): Promise<Window | null> {
  const windows = await getAllWindows();
  return windows.find((window) => window.label === label) ?? null;
}

function readSavedPosition(): SavedPosition | null {
  try {
    const raw = localStorage.getItem(TRAFFIC_BALL_POSITION_KEY);
    if (!raw) return null;
    const value = JSON.parse(raw) as Partial<SavedPosition>;
    if (!Number.isFinite(value.x) || !Number.isFinite(value.y)) return null;
    return { x: Number(value.x), y: Number(value.y) };
  } catch {
    return null;
  }
}

async function rememberPosition(ball: Window): Promise<void> {
  try {
    const position = await ball.outerPosition();
    localStorage.setItem(
      TRAFFIC_BALL_POSITION_KEY,
      JSON.stringify({ x: position.x, y: position.y } satisfies SavedPosition),
    );
  } catch {
    // Position persistence is a convenience and must never block restore.
  }
}

async function resolveTrafficBallPosition(): Promise<PhysicalPosition> {
  const saved = readSavedPosition();
  const savedMonitor = saved
    ? await monitorFromPoint(saved.x + TRAFFIC_BALL_SIZE_LOGICAL / 2, saved.y + TRAFFIC_BALL_SIZE_LOGICAL / 2)
        .catch(() => null)
    : null;
  const monitor = savedMonitor ?? await currentMonitor();

  if (!monitor) {
    return new PhysicalPosition(saved?.x ?? 24, saved?.y ?? 24);
  }

  const size = Math.round(TRAFFIC_BALL_SIZE_LOGICAL * monitor.scaleFactor);
  const margin = Math.round(TRAFFIC_BALL_MARGIN_LOGICAL * monitor.scaleFactor);
  const workArea = monitor.workArea;
  const minX = workArea.position.x + margin;
  const minY = workArea.position.y + margin;
  const maxX = Math.max(minX, workArea.position.x + workArea.size.width - size - margin);
  const maxY = Math.max(minY, workArea.position.y + workArea.size.height - size - margin);

  if (saved && savedMonitor) {
    return new PhysicalPosition(
      Math.max(minX, Math.min(maxX, saved.x)),
      Math.max(minY, Math.min(maxY, saved.y)),
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

export async function restoreMainWindow(): Promise<void> {
  const [main, ball] = await Promise.all([
    getWindowByLabel(MAIN_WINDOW_LABEL),
    getWindowByLabel(TRAFFIC_BALL_LABEL),
  ]);

  if (ball) await rememberPosition(ball);
  if (main) {
    await main.show();
    await main.setFocus().catch(() => {});
  }
  await ball?.destroy().catch(() => {});
}

export async function destroyTrafficBallIfMainVisible(): Promise<void> {
  const [main, ball] = await Promise.all([
    getWindowByLabel(MAIN_WINDOW_LABEL),
    getWindowByLabel(TRAFFIC_BALL_LABEL),
  ]);
  if (!main || !ball || !(await main.isVisible())) return;
  await rememberPosition(ball);
  await ball.destroy().catch(() => {});
}
