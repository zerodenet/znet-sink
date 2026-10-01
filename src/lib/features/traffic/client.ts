import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { coreEvents } from '$lib/services/core-events.svelte';
import type { GuiEventPayload } from '$lib/types/core';
import type { TrafficGateway, TrafficDiscovery } from '$lib/features/traffic/types';
import { page, snapshot, reset } from '$lib/features/traffic/wire';

import { TrafficStream } from '$lib/features/traffic/stream';

export const trafficGateway: TrafficGateway = {
  discover: () => invoke<TrafficDiscovery>('gui_traffic_discover'),
  page: async (input) => page(await invoke('gui_traffic_page', { input })),
  get: async (scope) => snapshot(await invoke('gui_traffic_get', { scope })),
  reset: async (input) => reset(await invoke('gui_traffic_reset', { input })),
  async subscribe(event, status) {
    const stops: (() => void)[] = [];
    const stream = new TrafficStream();
    try {
      stops.push(await listen<GuiEventPayload>('gui:event', ({ payload }) => stream.receive(payload, event, status)));
      stops.push(await listen<{ status: string }>('gui:event-status', ({ payload }) => {
        stream.restart();
        status(payload.status === 'subscribed' ? 'subscribed' : 'reconnecting');
      }));
      status(coreEvents.isSubscribed ? 'subscribed' : 'offline');
      // The app owns the shared IPC stream. Leaving this view removes only
      // these listeners; it never stops other consumers' subscription.
      return () => stops.forEach(stop => stop());
    } catch (error) { stops.forEach(stop => stop()); throw error; }
  },
};
