type RefreshShortcut = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey'>;
type BrowserShortcut = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey'>;

export function isRefreshShortcut(event: RefreshShortcut): boolean {
  if (event.key === 'F5') return true;
  return (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'r';
}

export function isBrowserShortcut(event: BrowserShortcut): boolean {
  if (isRefreshShortcut(event)) return true;
  if (!(event.ctrlKey || event.metaKey)) return false;

  const key = event.key.toLowerCase();
  if (['f', 'p', 's', 'u', '+', '=', '-', '0'].includes(key)) return true;
  return event.shiftKey && ['c', 'i', 'j'].includes(key);
}

export function installDesktopWebviewGuards(
  isDevelopment = import.meta.env.DEV,
): () => void {
  if (isDevelopment || typeof document === 'undefined' || typeof window === 'undefined') {
    return () => {};
  }

  const preventBrowserContextMenu = (event: MouseEvent) => {
    event.preventDefault();
  };
  const preventBrowserShortcut = (event: KeyboardEvent) => {
    if (!isBrowserShortcut(event)) return;
    event.preventDefault();
    if (isRefreshShortcut(event) || (event.shiftKey && ['c', 'i', 'j'].includes(event.key.toLowerCase()))) {
      event.stopPropagation();
    }
  };

  document.addEventListener('contextmenu', preventBrowserContextMenu, true);
  window.addEventListener('keydown', preventBrowserShortcut, true);

  return () => {
    document.removeEventListener('contextmenu', preventBrowserContextMenu, true);
    window.removeEventListener('keydown', preventBrowserShortcut, true);
  };
}
