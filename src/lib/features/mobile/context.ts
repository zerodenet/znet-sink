import { getContext, setContext } from 'svelte';
import { MobileNavigation } from './navigation.svelte';
export type { MobileTab, MobilePage } from './navigation.svelte';
const key = Symbol('mobile-navigation');
const routeKey = Symbol('mobile-route');
export type MobileRouteOwner = { key: number; tab: import('./navigation.svelte').MobileTab; page: import('./navigation.svelte').MobilePage; trafficVisit?: import('$lib/features/endpoints/traffic-navigation').EndpointTrafficVisit };
export function provideMobileRoute(route: MobileRouteOwner) { setContext(routeKey, route); }
export function optionalMobileRoute(): MobileRouteOwner | undefined { return getContext(routeKey); }
export function optionalMobileNavigation(): MobileNavigation | undefined { return getContext(key); }
export function provideMobileNavigation(navigation = new MobileNavigation()) {
  setContext(key, navigation);
  return navigation;
}
export function useMobileNavigation(): MobileNavigation {
  const navigation = getContext<MobileNavigation | undefined>(key);
  if (!navigation) throw new Error('移动导航未初始化');
  return navigation;
}
