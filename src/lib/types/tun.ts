import type { GuiTunStatus } from './gui-api';

export type TunConfigSource = 'profile' | 'app' | 'runtime';

/**
 * Zero's observed TUN state plus ZNet-Sink's local desired-state/source view.
 * The extra fields are client-only and never change the Core wire contract.
 */
export interface GuiManagedTunStatus extends GuiTunStatus {
  desiredEnabled: boolean;
  configSource?: TunConfigSource;
  configSourceName?: string;
}
