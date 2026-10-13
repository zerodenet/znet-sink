export type RuntimePlatform = 'android' | 'ios' | 'desktop';

/** Presentation detection only. Native capabilities remain authoritative in Rust. */
export function detectRuntimePlatform(userAgent: string, platform = '', touchPoints = 0): RuntimePlatform {
  if (/Android/i.test(userAgent)) return 'android';
  if (/iPhone|iPad|iPod/i.test(userAgent) || (platform === 'MacIntel' && touchPoints > 1)) return 'ios';
  return 'desktop';
}

export const mobilePlatform = $state({
  runtimePlatform: 'desktop' as RuntimePlatform,
  isMobileLayout: false,
  responsivePreview: false,
});

export function observeMobilePresentation(): () => void {
  const runtimePlatform = detectRuntimePlatform(navigator.userAgent, navigator.platform, navigator.maxTouchPoints);
  const query = window.matchMedia('(max-width: 640px)');
  const apply = () => {
    mobilePlatform.runtimePlatform = runtimePlatform;
    mobilePlatform.isMobileLayout = runtimePlatform !== 'desktop' || query.matches;
    document.documentElement.dataset.znetPresentation = mobilePlatform.isMobileLayout ? 'mobile' : 'desktop';
    mobilePlatform.responsivePreview = runtimePlatform === 'desktop' && query.matches;
  };
  apply();
  // Android 7's original WebView does not provide MediaQueryList.addEventListener.
  if (query.addEventListener) query.addEventListener('change', apply);
  else query.addListener(apply);
  return () => {
    if (query.removeEventListener) query.removeEventListener('change', apply);
    else query.removeListener(apply);
  };
}
