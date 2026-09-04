import { browser } from '$app/environment';
import { TrayIcon } from '@tauri-apps/api/tray';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { store } from './store.svelte';

export type ThemeMode = 'light' | 'dark' | 'system';

const MAIN_TRAY_ID = 'main-tray';
const nativeIconCache = new Map<string, Uint8Array>();
let nativeIconRequestId = 0;

export function applyTheme(mode: ThemeMode) {
  if (!browser) return;

  const root = document.documentElement;
  let isDark: boolean;
  if (mode === 'system') {
    isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  } else {
    isDark = mode === 'dark';
  }
  root.classList.toggle('dark', isDark);
  updateFavicon(isDark);
  void updateNativeIcon(isDark);
}

/** Switch the browser tab favicon to match the current theme. */
function updateFavicon(isDark: boolean) {
  const link = document.querySelector('link[rel="icon"]') as HTMLLinkElement | null;
  if (link) {
    link.href = isDark ? '/app-icon.png' : '/app-icon-bg.png';
  }
}

/** Keep the native window/taskbar and tray icons aligned with the UI theme. */
async function updateNativeIcon(isDark: boolean) {
  if (!(window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) return;

  const requestId = ++nativeIconRequestId;
  const iconPath = isDark ? '/app-icon.png' : '/app-icon-bg.png';

  try {
    let iconBytes = nativeIconCache.get(iconPath);
    if (!iconBytes) {
      const response = await fetch(iconPath);
      if (!response.ok) throw new Error(`failed to load ${iconPath}: HTTP ${response.status}`);
      iconBytes = new Uint8Array(await response.arrayBuffer());
      nativeIconCache.set(iconPath, iconBytes);
    }
    if (requestId !== nativeIconRequestId) return;

    const tray = await TrayIcon.getById(MAIN_TRAY_ID);
    if (requestId !== nativeIconRequestId) return;
    await Promise.allSettled([
      getCurrentWindow().setIcon(iconBytes),
      tray?.setIcon(iconBytes) ?? Promise.resolve(),
    ]);
  } catch (error) {
    console.warn('[ZNet] failed to update native theme icon', error);
  }
}

export function initTheme() {
  if (!browser) return;
  const saved = (localStorage.getItem('znet-theme') as ThemeMode | null) || store.selectedTheme;
  applyTheme(saved || 'system');
}

export function setTheme(mode: ThemeMode) {
  applyTheme(mode);
  store.persistTheme(mode);
}
