import type { ProxyMode } from '$lib/types/gui-api';
import type { Destination } from './model';

export interface OverviewFeedback { pending: string; target: string; message: string; error: boolean }
export interface OverviewProfiles { selected: string; loading: boolean; error: string | null; options: { value: string; label: string }[] }
export interface OverviewNetwork { ip: string; description: string; countryCode?: string; location?: string; loading: boolean; error: string | null }
export interface OverviewActions {
  navigate: (target: Destination) => void;
  refresh: () => void;
  chooseProfile: (id: string) => void;
  choosePolicy: (group: string, target: string) => void;
  setMode: (mode: ProxyMode) => void;
  toggleTun: () => void;
  recoverTun: () => void;
}
