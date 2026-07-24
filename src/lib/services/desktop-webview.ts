type RefreshShortcut = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey'>;

export function isRefreshShortcut(event: RefreshShortcut): boolean {
  if (event.key === 'F5') return true;
  return (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'r';
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
  const preventBrowserRefresh = (event: KeyboardEvent) => {
    if (!isRefreshShortcut(event)) return;
    event.preventDefault();
    event.stopPropagation();
  };

  document.addEventListener('contextmenu', preventBrowserContextMenu, true);
  window.addEventListener('keydown', preventBrowserRefresh, true);

  return () => {
    document.removeEventListener('contextmenu', preventBrowserContextMenu, true);
    window.removeEventListener('keydown', preventBrowserRefresh, true);
  };
}
