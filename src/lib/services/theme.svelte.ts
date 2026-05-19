import { browser } from '$app/environment';

export type ThemeMode = 'light' | 'dark' | 'system';

export function applyTheme(mode: ThemeMode) {
  if (!browser) return;
  
  const root = document.documentElement;
  
  // 如果是跟随系统
  if (mode === 'system') {
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    root.classList.toggle('dark', isDark);
    localStorage.removeItem('znet-theme');
  } else {
    // 手动强制锁定
    root.classList.toggle('dark', mode === 'dark');
    localStorage.setItem('znet-theme', mode);
  }
}

// 客户端刚拉起时的初始化钩子
export function initTheme() {
  if (!browser) return;
  const saved = localStorage.getItem('znet-theme') as ThemeMode | null;
  applyTheme(saved || 'system');
}
