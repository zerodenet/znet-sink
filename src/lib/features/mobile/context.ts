import { getContext, setContext } from 'svelte';
import { MobileNavigation } from './navigation.svelte';
export type { MobileTab, MobilePage } from './navigation.svelte';
const key = Symbol('mobile-navigation');
export function provideMobileNavigation(navigation = new MobileNavigation()) {
  setContext(key, navigation);
  return navigation;
}
export function useMobileNavigation(): MobileNavigation {
  const navigation = getContext<MobileNavigation | undefined>(key);
  if (!navigation) throw new Error('移动导航未初始化');
  return navigation;
}
