import { createLatestRequestGate } from '$lib/services/latest-request-gate.js';
import type { ConnectionStatus, SelfTestSnapshot, CoreOverview } from '$lib/types/gui-api';
import type { GuiManagedTunStatus } from '$lib/types/tun';

/** One owner for runtime/capture observations. Intent and failed reads remain explicit. */
export class RuntimeObservation {
  selfTest = $state<SelfTestSnapshot | null>(null);
  selfTestUpdatedAt = $state(0);
  coreOverview = $state<CoreOverview | null>(null);
  supportsTrafficStats = $state(true);
  readonly selfTestGate = createLatestRequestGate();
  readonly overviewGate = createLatestRequestGate();
  readonly capabilitiesGate = createLatestRequestGate();
  connection = $state<ConnectionStatus | null>(null);
  connectionUpdatedAt = $state(0);
  connectionError = $state<string | null>(null);
  tunStatus = $state<GuiManagedTunStatus | null>(null);
  tunStatusError = $state<string | null>(null);
  savedTunEnabled = $state<boolean | undefined>(undefined);
  readonly connectionGate = createLatestRequestGate();
  readonly tunGate = createLatestRequestGate();

  invalidate() {
    this.selfTestGate.reset(); this.overviewGate.reset(); this.capabilitiesGate.reset();
    this.connectionGate.reset();
    this.tunGate.reset();
  }
}
