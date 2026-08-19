import { PhysicalPosition } from '@tauri-apps/api/dpi';
import {
  currentMonitor,
  getAllWindows,
  getCurrentWindow,
  type Window,
} from '@tauri-apps/api/window';

const MAIN_WINDOW_LABEL = 'main';
const TRAFFIC_BALL_LABEL = 'traffic-ball';
const TRAFFIC_BALL_SIZE_LOGICAL = 112;
const TRAFFIC_BALL_MARGIN_LOGICAL = 18;

let positionedForSession = false;

async function getWindowByLabel(label: string): Promise<Window | null> {
  const windows = await getAllWindows();
  return windows.find((window) => window.label === label) ?? null;
}

async function positionTrafficBall(ball: Window): Promise<void> {
  if (positionedForSession) return;
  const monitor = await currentMonitor();
  if (!monitor) return;

  const size = Math.round(TRAFFIC_BALL_SIZE_LOGICAL * monitor.scaleFactor);
  const margin = Math.round(TRAFFIC_BALL_MARGIN_LOGICAL * monitor.scaleFactor);
  const workArea = monitor.workArea;
  const x = workArea.position.x + workArea.size.width - size - margin;
  const y = workArea.position.y + workArea.size.height - size - margin;

  await ball.setPosition(new PhysicalPosition(x, y));
  positionedForSession = true;
}

export async function showTrafficBall(mainWindow: Window = getCurrentWindow()): Promise<void> {
  try {
    const ball = await getWindowByLabel(TRAFFIC_BALL_LABEL);
    if (!ball) {
      await mainWindow.minimize();
      return;
    }

    await positionTrafficBall(ball);
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

  if (main) {
    await main.show();
    await main.setFocus().catch(() => {});
  }
  await ball?.hide().catch(() => {});
}
