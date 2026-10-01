import type { GuiEventPayload } from '$lib/types/core';
import type { TrafficEvent, StreamStatus } from '$lib/features/traffic/types';
import { page, reset, u64 } from '$lib/features/traffic/wire';

/** Delivery ordering belongs to the whole GUI stream, not just traffic pages. */
export class TrafficStream {
  private generation?: number;
  private sequences = new Map<string, bigint>();
  receive(envelope: GuiEventPayload, event: (event: TrafficEvent) => void, status: (status: StreamStatus) => void) {
    if (this.generation !== undefined && envelope.generation < this.generation) return;
    if (this.generation !== envelope.generation) {
      if (this.generation !== undefined) status('gap');
      this.generation = envelope.generation;
      this.sequences.clear();
    }
    const payload = envelope.event;
    try {
      if (payload.sequenceExact && payload.coreInstanceId) {
        const next = BigInt(u64(payload.sequenceExact));
        const previous = this.sequences.get(payload.coreInstanceId);
        if (previous !== undefined && next <= previous) return;
        if (previous !== undefined && next > previous + 1n) status('gap');
        this.sequences.set(payload.coreInstanceId, next);
        if (this.sequences.size > 4) this.sequences.delete(this.sequences.keys().next().value!);
      }
      if (payload.eventType !== 'traffic.scopesSampled' && payload.eventType !== 'traffic.reset') return;
      if (payload.payload?.kind !== 'trafficObservation') throw new Error('统计事件载荷不兼容');
      const data = payload.eventType === 'traffic.reset' ? reset(payload.payload.data) : page(payload.payload.data);
      if (payload.coreInstanceId && payload.coreInstanceId !== data.core_instance_id) throw new Error('统计事件实例不匹配');
      event({ type: payload.eventType === 'traffic.reset' ? 'reset' : 'sample', instance: data.core_instance_id, payload: data });
    } catch { status('gap'); }
  }
  restart() { this.sequences.clear(); }
}
