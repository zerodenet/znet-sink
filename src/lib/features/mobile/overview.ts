import type { Observation } from '$lib/features/traffic/history';
import { metric } from '$lib/features/traffic/history';
import type { TrafficSnapshot } from '$lib/features/traffic/types';

/** Count only confirmed samples in one capture session. Counter periods stay separate. */
export class CaptureLedger {
  up: bigint | null = null;
  down: bigint | null = null;
  sampledAt: string | null = null;
  private active = false;
  private baseline: TrafficSnapshot | undefined;
  update(active: boolean, snapshot: TrafficSnapshot | undefined): void {
    if (active !== this.active) {
      this.active = active;
      this.baseline = undefined;
      if (active) { this.up = null; this.down = null; this.sampledAt = null; }
    }
    if (!active || !snapshot || snapshot.scope.kind !== 'global') return;
    const plane = snapshot.planes.find(row => row.plane === 'flow');
    const up = metric(plane, 'bytes_up');
    const down = metric(plane, 'bytes_down');
    // A missing counter is not a zero baseline and must never bridge a gap.
    if (up === null || down === null) { this.baseline = undefined; return; }
    const before = this.baseline;
    const samePeriod = before?.core_instance_id === snapshot.core_instance_id
      && before.config_revision === snapshot.config_revision
      && before.stats_epoch === snapshot.stats_epoch && before.generation === snapshot.generation;
    const monotonic = before && BigInt(snapshot.sampled_at_monotonic_ns) > BigInt(before.sampled_at_monotonic_ns);
    if (samePeriod && monotonic) {
      const oldPlane = before!.planes.find(row => row.plane === 'flow');
      const oldUp = metric(oldPlane, 'bytes_up');
      const oldDown = metric(oldPlane, 'bytes_down');
      if (oldUp !== null && oldDown !== null && up >= oldUp && down >= oldDown) {
        this.up = (this.up ?? 0n) + up - oldUp;
        this.down = (this.down ?? 0n) + down - oldDown;
      }
    } else if (!before && this.up === null && this.down === null) {
      this.up = 0n; this.down = 0n;
    }
    // Repeated/late samples cannot move the established baseline backwards.
    if (!samePeriod || !before || monotonic) {
      this.baseline = snapshot;
      this.sampledAt = snapshot.sampled_at_unix_ms;
    }
  }
  interrupt(): void { this.baseline = undefined; }
}

export function overviewCounters(row: Observation | undefined) {
  const plane = row?.snapshot.planes.find(value => value.plane === 'flow');
  const up = metric(plane, 'bytes_up');
  const down = metric(plane, 'bytes_down');
  return { up, down, total: up === null || down === null ? null : up + down };
}

export function trafficShare(up: bigint | null, down: bigint | null): number | null {
  if (up === null || down === null || up + down === 0n) return null;
  // Bounded presentation ratio; exact counters never become JS numbers.
  return Number(up * 10_000n / (up + down)) / 100;
}
