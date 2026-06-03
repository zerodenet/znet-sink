import { browser } from '$app/environment';
import { store } from './store.svelte';

export type ThemeMode = 'light' | 'dark' | 'system';

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
}

/** Switch the browser tab favicon to match the current theme. */
function updateFavicon(isDark: boolean) {
  const link = document.querySelector('link[rel="icon"]') as HTMLLinkElement | null;
  if (link) {
    link.href = isDark ? '/app-icon.png' : '/app-icon-bg.png';
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
